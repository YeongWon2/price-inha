//! (39차) 메신저 한 벌: 토큰 · 페어링 장부 · 받기 루프의 손잡이 · 상태. 경로(`http.rs`) · 루프(`runner.rs`) ·
//! 명령(`commands.rs`) · 나가는 알림(`outbound.rs`)이 이것을 쥔다(계약은 `crate::messenger` 문서).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::Instant;

use pi_channel::{Gatekeeper, PairingBook, Telegram, TelegramConfig};
use pi_market::Clock;
use pi_office::EventSink;
use pi_secrets::SecretStore;
use pi_store::{Store, UnixMillis};
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use super::book::FileBook;
use super::status::{KIND_MESSENGER, LoopState};
use super::{MessengerWiring, TELEGRAM_KEYS};
use crate::reach::{Linked, OfficeDesk};
use crate::setup::AttemptLimit;
use crate::{Settings, Trading};

/// 끌 때 루프가 스스로 끝나기를 기다리는 여유(요청 상한 위에).
const STOP_GRACE_SECS: u64 = 2;

/// 메신저를 여는 데 드는 값.
pub(crate) struct MessengerParts {
    pub(crate) wiring: MessengerWiring,
    pub(crate) store: Store,
    pub(crate) secrets: Arc<SecretStore>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) trading: Arc<Trading>,
    pub(crate) linked: Arc<Linked>,
}

/// 루프의 지금 모습(상태 · 봇 이름).
struct Live {
    state: LoopState,
    bot: Option<String>,
}

/// 도는 루프 하나.
struct Running {
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
}

/// 메신저 한 벌.
pub(crate) struct Messenger {
    pub(crate) wiring: MessengerWiring,
    pub(crate) home: PathBuf,
    pub(crate) demo: bool,
    pub(crate) secrets: Arc<SecretStore>,
    pub(crate) store: Store,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) trading: Arc<Trading>,
    pub(crate) linked: Arc<Linked>,
    gatekeeper: Gatekeeper,
    /// 장부 읽고-고치고-쓰기를 한 번에 하나만(루프의 판정과 경로의 승인이 겹치지 않게).
    book_lock: Mutex<()>,
    pub(crate) limit: Mutex<AttemptLimit>,
    live: Mutex<Live>,
    run: tokio::sync::Mutex<Option<Running>>,
    closing: AtomicBool,
    /// (41차 3 · 4번) 묻기 · 공지를 넘길 사무실(세션 경로를 만들 때 붙는다).
    desk: OnceLock<Weak<OfficeDesk>>,
    /// (41차 1번) 열린 논의 글 묶음(묶음 창이 닫히면 메시지 하나로 보낸다).
    pub(crate) talk_lines: Mutex<Option<Vec<String>>>,
    /// (41차 3 · 4번) 묻기 · 공지를 마지막으로 받은 시각(사무실 보내기 간격과 같은 규칙).
    last_ask: Mutex<Option<UnixMillis>>,
}

impl std::fmt::Debug for Messenger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Messenger").finish_non_exhaustive()
    }
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Messenger {
    /// 연다(루프는 [`Messenger::start_saved`]가 켠다).
    pub(crate) fn open(settings: &Settings, parts: MessengerParts) -> Arc<Messenger> {
        let book: Arc<dyn PairingBook> = Arc::new(FileBook::new(&settings.home));
        Arc::new(Messenger {
            wiring: parts.wiring,
            home: settings.home.clone(),
            demo: settings.demo,
            secrets: parts.secrets,
            store: parts.store,
            clock: parts.clock,
            trading: parts.trading,
            linked: parts.linked,
            gatekeeper: Gatekeeper::new(book),
            book_lock: Mutex::new(()),
            limit: Mutex::new(AttemptLimit::new(
                settings.register_attempts,
                settings.register_window,
            )),
            live: Mutex::new(Live {
                state: LoopState::Off,
                bot: None,
            }),
            run: tokio::sync::Mutex::new(None),
            closing: AtomicBool::new(false),
            desk: OnceLock::new(),
            talk_lines: Mutex::new(None),
            last_ask: Mutex::new(None),
        })
    }

    /// (41차 3 · 4번) 묻기 · 공지를 넘길 사무실을 붙인다(한 번만).
    pub(crate) fn attach_desk(&self, desk: &Arc<OfficeDesk>) {
        let _ = self.desk.set(Arc::downgrade(desk));
    }

    /// 붙은 사무실(없거나 이미 놓였으면 없음).
    pub(crate) fn desk(&self) -> Option<Arc<OfficeDesk>> {
        self.desk.get().and_then(Weak::upgrade)
    }

    /// 끄는 중인가(뒤에서 답을 기다리는 일은 그만둔다).
    pub(crate) fn is_closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst)
    }

    /// (41차 3 · 4번) 사무실 보내기 간격([`crate::OFFICE_SEND_INTERVAL`], 시계는 `Wiring::clock`) 안에 또 왔으면
    /// 거짓. 받으면 시각을 남긴다.
    pub(crate) fn pace(&self) -> bool {
        let now = self.linked.now();
        let gap = i64::try_from(crate::OFFICE_SEND_INTERVAL.as_millis()).unwrap_or(i64::MAX);
        let mut last = locked(&self.last_ask);
        if last.is_some_and(|at| now.0 - at.0 < gap) {
            return false;
        }
        *last = Some(now);
        true
    }

    /// 장부를 잡고 문지기에게 일을 시킨다.
    pub(crate) fn with_gate<T>(&self, work: impl FnOnce(&Gatekeeper) -> T) -> T {
        let _guard = locked(&self.book_lock);
        work(&self.gatekeeper)
    }

    /// 장부 지우기처럼 문지기를 거치지 않는 일도 같은 잠금 안에서.
    pub(crate) fn with_book_lock<T>(&self, work: impl FnOnce() -> T) -> T {
        let _guard = locked(&self.book_lock);
        work()
    }

    /// 저장된 토큰(없거나 읽지 못하면 `None`).
    pub(crate) fn token(&self) -> Option<Zeroizing<String>> {
        let saved = self.secrets.read_provider(TELEGRAM_KEYS).ok().flatten()?;
        let token = saved.get("bot_token")?;
        Some(Zeroizing::new(token.expose().to_string()))
    }

    /// 시도 하나를 센다(키 등록과 같은 규칙). 넘었으면 `false`.
    pub(crate) fn allow_attempt(&self) -> bool {
        locked(&self.limit).allow(Instant::now())
    }

    pub(crate) fn state(&self) -> LoopState {
        locked(&self.live).state
    }

    pub(crate) fn bot(&self) -> Option<String> {
        locked(&self.live).bot.clone()
    }

    pub(crate) fn set_state(&self, state: LoopState) {
        locked(&self.live).state = state;
    }

    pub(crate) fn set_bot(&self, bot: Option<String>) {
        locked(&self.live).bot = bot;
    }

    /// 지금 시각.
    pub(crate) fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.clock.now()
    }

    /// 텔레그램 어댑터를 만든다. 주소를 읽을 수 없거나 만들 수 없으면 `None`(까닭은 기록만).
    pub(crate) fn telegram(&self, token: &str) -> Option<Telegram> {
        let Ok(api_base) = url::Url::parse(&self.wiring.telegram_api_base) else {
            tracing::warn!("메신저 주소를 읽을 수 없어 메신저를 켜지 않습니다");
            return None;
        };
        let config = TelegramConfig {
            api_base,
            token: pi_channel::BotToken::new(token.to_string()),
            poll_timeout_secs: self.wiring.poll_timeout_secs,
            request_timeout: self.wiring.request_timeout,
        };
        match Telegram::new(config) {
            Ok(telegram) => Some(telegram),
            Err(error) => {
                tracing::warn!(%error, "메신저 어댑터를 만들지 못했습니다");
                None
            }
        }
    }

    /// 이벤트 하나([`KIND_MESSENGER`]). 상태는 부르는 쪽이 먼저 맞춘다.
    pub(crate) async fn publish(&self, action: &str, tail: Option<String>) {
        let mut payload = json!({ "action": action, "channel": pi_channel::TELEGRAM_CHANNEL });
        if let Some(tail) = tail {
            payload["sender_tail"] = Value::String(tail);
        }
        if let Err(error) = self.linked.sink.publish(KIND_MESSENGER, payload).await {
            tracing::warn!(%error, "메신저 이벤트를 내지 못했습니다");
        }
    }

    /// 띄울 때: 저장된 토큰이 있고 시연이 아니면 루프를 켠다(봇 이름은 루프가 확인한다).
    pub(crate) async fn start_saved(self: &Arc<Self>) {
        if self.demo {
            return;
        }
        let mut run = self.run.lock().await;
        // 그 사이 등록이 이미 켰으면 그대로 둔다.
        if run.is_some() {
            return;
        }
        if let Some(token) = self.token() {
            self.launch(&mut run, &token, None);
        }
    }

    /// 루프를 (다시) 켠다. 돌던 것은 먼저 멈춘다.
    pub(crate) async fn start(self: &Arc<Self>, token: &str, bot: Option<String>) {
        let mut run = self.run.lock().await;
        if let Some(old) = run.take() {
            finish(old, self.stop_wait()).await;
        }
        self.launch(&mut run, token, bot);
    }

    /// 루프 하나를 띄워 자리에 둔다(자리는 비어 있다).
    fn launch(self: &Arc<Self>, run: &mut Option<Running>, token: &str, bot: Option<String>) {
        if self.demo || self.closing.load(Ordering::SeqCst) {
            return;
        }
        self.set_bot(bot);
        let Some(telegram) = self.telegram(token) else {
            self.set_state(LoopState::Error);
            return;
        };
        self.set_state(LoopState::Polling);
        let (stop, stopping) = watch::channel(false);
        let task = tokio::spawn(super::runner::run(Arc::clone(self), telegram, stopping));
        *run = Some(Running { stop, task });
    }

    /// 루프를 멈춘다(돌지 않으면 그대로). 상태는 꺼짐.
    pub(crate) async fn stop(&self) {
        let old = self.run.lock().await.take();
        if let Some(old) = old {
            finish(old, self.stop_wait()).await;
        }
        self.set_state(LoopState::Off);
        self.set_bot(None);
    }

    /// 끈다: 다시 켜지 않게 표시하고 루프를 멈춘다(자식 태스크가 남지 않는다).
    pub(crate) async fn shutdown(&self) {
        self.closing.store(true, Ordering::SeqCst);
        self.stop().await;
    }

    fn stop_wait(&self) -> std::time::Duration {
        self.wiring.request_timeout + std::time::Duration::from_secs(STOP_GRACE_SECS)
    }

    /// `GET` 상태 몸통(보낸 사람 ID · 토큰 · 코드는 싣지 않는다).
    pub(crate) fn status_json(&self, editable: bool) -> Value {
        let configured = self.token().is_some();
        let book = self
            .with_gate(|_| FileBook::new(&self.home).load())
            .unwrap_or_default();
        let now = self.now();
        let ttl = chrono::TimeDelta::seconds(pi_channel::PAIRING_TTL_SECS);
        let pending = book
            .pending
            .iter()
            .filter(|p| p.created_at + ttl > now)
            .count();
        let locked_until = book
            .locked_until
            .filter(|until| *until > now)
            .map(|until| until.timestamp_millis());
        let state = if configured && !self.demo {
            self.state()
        } else {
            LoopState::Off
        };
        json!({
            "editable": editable,
            "telegram": {
                "configured": configured,
                "bot": if configured { self.bot() } else { None },
                "state": state.word(),
                "paired": !book.allowed.is_empty(),
                "pending": pending,
                "locked_until": locked_until,
            }
        })
    }
}

/// 루프에 멈춤을 알리고 끝나기를 기다린다. 시간 안에 끝나지 않으면 그만두게 하고 거둔다.
async fn finish(running: Running, wait: std::time::Duration) {
    let Running { stop, mut task } = running;
    let _ = stop.send(true);
    if tokio::time::timeout(wait, &mut task).await.is_err() {
        task.abort();
        let _ = task.await;
    }
}
