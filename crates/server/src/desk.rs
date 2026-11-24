//! 설정 책상: 필수 설정의 지금 상태를 한곳에서 알고, 키 · 종목 등록을 받아 시세 흐름을 깨운다.
//!
//! 화면 쪽 경로([`crate::setup_http`])와 시세 흐름([`crate::feed`])이 함께 쓴다.
//! 받은 키 값은 `pi-secrets`로만 넘기고 어디에도 남기지 않는다(기록 · 오류 · 응답).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use pi_ai::{AiProvider, ClaudeCliProvider, CliStatus};
use pi_market::Symbol;
use pi_secrets::{KeyKind, SecretStore, SecretsError, VerifyFailure};
use tokio::sync::Notify;

mod ai;
mod keys;
mod limit;

use crate::ai_link::Leftovers;
use crate::keys::KeyPresence;
use crate::setup::{
    AttemptLimit, ITEM_AI, ITEM_BROKER, ITEM_DATA, ITEM_SYMBOLS, SETUP_ITEMS, SetupItem,
    SetupStatus, SymbolsSource, parse_symbols, save_watchlist,
};
use crate::{
    AiChoice, AiMethod, AiStatus, AppError, KeyVerifier, SetupRefusal, cli_refusal, find_claude,
    legacy_auth_file, save_ai_choice,
};
use ai::{AiNow, Held, hold};
pub(crate) use ai::{AiParts, Work, prepare_work};
pub(crate) use limit::limit_request;
pub use limit::{AI_DAILY_LIMIT_KEY, SETUP_AI_DAILY_LIMIT};

/// 키 확인 하나가 끝나기를 기다리는 가장 긴 시간. 넘으면 "닿지 않음"이고 저장하지 않는다.
/// (`pi-ai` · `pi-market`의 요청마다 시간 제한이 따로 있다. 이것은 그 바깥의 끝이다.)
const VERIFY_TIMEOUT: Duration = Duration::from_secs(30);

/// 지금 지켜보는 종목과 그 출처.
struct Watching {
    symbols: Vec<Symbol>,
    source: SymbolsSource,
}

/// 설정 책상을 만드는 데 드는 것.
pub(crate) struct DeskParts {
    pub(crate) secrets: Arc<SecretStore>,
    pub(crate) verifier: KeyVerifier,
    pub(crate) home: PathBuf,
    pub(crate) demo: bool,
    pub(crate) symbols: Vec<Symbol>,
    pub(crate) source: SymbolsSource,
    pub(crate) limit: AttemptLimit,
    pub(crate) symbols_limit: AttemptLimit,
    pub(crate) ai: AiParts,
    /// 띄울 때 고른 것의 작업 폴더 준비 결과([`prepare_work`], 막히는 작업 자리에서 미리 한다).
    pub(crate) work: Work,
}

/// 설정 책상.
pub(crate) struct Desk {
    secrets: Arc<SecretStore>,
    presence: KeyPresence,
    verifier: KeyVerifier,
    home: PathBuf,
    demo: bool,
    watching: Mutex<Watching>,
    /// 키 등록 시도의 횟수 제한(모든 종류가 함께 쓴다).
    limit: Mutex<AttemptLimit>,
    /// 종목 등록 시도의 횟수 제한(키 등록과 따로 센다).
    symbols_limit: Mutex<AttemptLimit>,
    /// 시세 받기를 시작했는가(시세 흐름이 적는다).
    started: AtomicBool,
    /// 등록이 시세 흐름을 깨우는 알림.
    wake: Notify,
    /// (6차) AI 연결을 잇는 데 드는 것(바뀌지 않는다).
    ai_parts: AiParts,
    /// (6차) 지금 고른 AI 연결.
    ai: Mutex<AiNow>,
    /// (6차) AI 연결 고르기를 하나씩 한다(저장 · 공급자 · 확인 결과가 어긋나지 않게).
    choosing: tokio::sync::Mutex<()>,
    /// (7차 5번) 다시 고르기로 물러난 CLI 공급자. 아직 도는 호출이 있을 수 있어 바로 끝내지 않고,
    /// 서버가 꺼질 때 함께 끝낸다.
    retired: Mutex<Vec<Arc<ClaudeCliProvider>>>,
    /// (8차 3번) 서버가 꺼지기 시작했다. 고르기 잠금을 기다리기 전에 세운다. 선 뒤의 고르기는 거부한다.
    closing: AtomicBool,
    /// (22차 6번) 하루 AI 호출 상한(켤 때 값 · 저장한 값).
    ai_limit: limit::AiLimit,
}

impl Desk {
    /// 아무것도 띄우지 않고 파일도 건드리지 않는다. 확인은 [`Desk::check_cli`]가 한다.
    pub(crate) fn new(parts: DeskParts) -> Self {
        let held = hold(
            &parts.ai,
            &parts.secrets,
            &parts.home,
            &parts.ai.choice,
            &parts.work,
        );
        let ai = AiNow {
            choice: parts.ai.choice.clone(),
            cli: None,
            work_blocked: matches!(parts.work, Work::NotEmpty),
            held,
        };
        let ai_limit = limit::AiLimit::load(&parts.home);
        let desk = Self {
            ai_limit,
            presence: KeyPresence::new(Arc::clone(&parts.secrets)),
            secrets: parts.secrets,
            verifier: parts.verifier,
            home: parts.home,
            demo: parts.demo,
            watching: Mutex::new(Watching {
                symbols: parts.symbols,
                source: parts.source,
            }),
            limit: Mutex::new(parts.limit),
            symbols_limit: Mutex::new(parts.symbols_limit),
            started: AtomicBool::new(false),
            wake: Notify::new(),
            ai_parts: parts.ai,
            ai: Mutex::new(ai),
            choosing: tokio::sync::Mutex::new(()),
            retired: Mutex::new(Vec::new()),
            closing: AtomicBool::new(false),
        };
        // 띄우자마자 읽는 설정 상태도 사실과 같게 한다(시세 흐름이 곧 이어서 적는다).
        desk.set_started(desk.demo || desk.ready());
        desk
    }

    /// (6차) 띄울 때: 고른 것이 CLI면 한 번 확인하고 그 결과를 쥔다(모델을 부르지 않는다).
    /// (7차 11번) 서버는 이것을 기다리지 않는다(뒤에서 돈다). 끝나면 시세 흐름을 깨운다.
    /// 작업 폴더가 비어 있지 않으면 확인하지 않는다(CLI를 띄우지 않는다).
    pub(crate) async fn check_cli(&self) {
        let _turn = self.choosing.lock().await;
        let held = {
            let now = self.ai_now();
            if now.work_blocked || self.closing.load(Ordering::SeqCst) {
                return;
            }
            now.held.clone()
        };
        let Held::Cli(cli) = held else {
            return;
        };
        let status = check(&cli).await;
        tracing::info!(
            state = crate::cli_state(&status),
            "고른 Claude Code CLI를 확인했습니다"
        );
        self.ai_now().cli = Some(status);
        self.set_started(self.demo || self.ready());
        self.wake.notify_one();
    }

    /// (8차 3번) 꺼지기 시작했다고 표시한다. 이 뒤에 고르기 잠금을 얻은 고르기는 거부된다.
    /// 잠금을 기다리지 않는다(잠금을 기다리던 고르기도 표시를 본다).
    pub(crate) fn begin_closing(&self) {
        self.closing.store(true, Ordering::SeqCst);
    }

    /// (7차 5번) 서버가 꺼질 때: 쥔 CLI 공급자와 물러난 CLI 공급자를 모두 끝내고 자식이 거둬질 때까지 기다린다.
    /// (8차 3번) 먼저 꺼지기 시작 표시를 세우고, 고르기와 같은 잠금을 잡은 뒤에 끝낸다: 도는 고르기가
    /// 공급자를 바꿔 끼운 뒤에 끝내므로 끝내지 않은 공급자가 남지 않는다.
    pub(crate) async fn shutdown_ai(&self) {
        self.begin_closing();
        let _turn = self.choosing.lock().await;
        let mut all =
            std::mem::take(&mut *self.retired.lock().unwrap_or_else(PoisonError::into_inner));
        if let Held::Cli(cli) = self.ai_now().held.clone() {
            all.push(cli);
        }
        for cli in all {
            cli.shutdown().await;
        }
    }

    /// 잠금을 쥔 채 죽은 스레드가 있어도 고른 연결은 온전하다(통째로 바꿔 쓴다).
    fn ai_now(&self) -> MutexGuard<'_, AiNow> {
        self.ai.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 잠금을 쥔 채 죽은 스레드가 있어도 종목 목록은 온전하다(통째로 바꿔 쓴다).
    fn watching(&self) -> MutexGuard<'_, Watching> {
        self.watching.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// (10차 10a 5번) 등록된 비밀 값을 가린 글. 저장소에 남길 까닭 글에 쓴다(E38).
    pub(crate) fn redact(&self, text: &str) -> String {
        self.secrets.redact(text)
    }

    /// 지금 지켜보는 종목.
    pub(crate) fn symbols(&self) -> Vec<Symbol> {
        self.watching().symbols.clone()
    }

    /// AI 연결이 갖춰졌는가. 고른 연결만 본다(둘 다 있어도 넘어가지 않는다):
    /// API 키면 그 키가 등록됐을 때, CLI면 마지막 확인이 로그인된 구독일 때.
    fn has_ai(&self) -> bool {
        let now = self.ai_now();
        match now.choice.method {
            AiMethod::ApiKey => {
                drop(now);
                self.presence.has(KeyKind::AiApiKey)
            }
            AiMethod::Cli => {
                !now.work_blocked && matches!(now.cli, Some(CliStatus::LoggedIn { .. }))
            }
        }
    }

    /// 설정 상태의 `ai` 칸. CLI를 띄우지 않는다.
    fn ai_status(&self) -> AiStatus {
        let (choice, cli, work_blocked) = {
            let now = self.ai_now();
            (now.choice.clone(), now.cli.clone(), now.work_blocked)
        };
        let command = match choice.method {
            AiMethod::Cli => choice.command,
            AiMethod::ApiKey => {
                let path_var = self
                    .ai_parts
                    .cli_env
                    .iter()
                    .find(|(name, _)| name == "PATH")
                    .map(|(_, value)| value.as_str());
                find_claude(
                    path_var,
                    self.ai_parts.user_home.as_deref(),
                    &self.ai_parts.cli_locations,
                )
            }
        };
        AiStatus {
            method: choice.method,
            command,
            config_dir: choice.config_dir,
            cli: if choice.method == AiMethod::Cli {
                cli
            } else {
                None
            },
            work_dir_not_empty: choice.method == AiMethod::Cli && work_blocked,
            legacy_auth_file: legacy_auth_file(&self.home),
        }
    }

    /// 그 항목이 등록됐는가. 모르는 이름은 등록되지 않은 것이다.
    fn registered(&self, name: &str) -> bool {
        match name {
            ITEM_BROKER => self.presence.has(KeyKind::Broker),
            ITEM_AI => self.has_ai(),
            ITEM_SYMBOLS => !self.watching().symbols.is_empty(),
            ITEM_DATA => self.presence.has(KeyKind::Data),
            _ => false,
        }
    }

    /// 필수 항목이 모두 등록됐는가([`crate::SETUP_ITEMS`]의 필수 항목만 본다).
    pub(crate) fn ready(&self) -> bool {
        SETUP_ITEMS
            .iter()
            .filter(|spec| spec.required)
            .all(|spec| self.registered(spec.name))
    }

    /// (W5 1차) 필수 키(증권사 · AI)가 모두 등록돼 있는가(다시 시작, E9).
    pub(crate) fn keys_ready(&self) -> bool {
        self.registered(ITEM_BROKER) && self.registered(ITEM_AI)
    }

    /// 설정 상태. 값 · 값의 일부 · 길이는 없다(V46).
    pub(crate) fn status(&self) -> SetupStatus {
        let items: Vec<SetupItem> = SETUP_ITEMS
            .iter()
            .map(|spec| SetupItem {
                name: spec.name,
                required: spec.required,
                registered: self.registered(spec.name),
            })
            .collect();
        let ready = items.iter().all(|item| !item.required || item.registered);
        let (symbols, symbols_source) = {
            let watching = self.watching();
            (watching.symbols.clone(), watching.source)
        };
        SetupStatus {
            items,
            ready,
            started: self.demo || self.started.load(Ordering::SeqCst),
            demo: self.demo,
            symbols,
            symbols_source,
            ai: self.ai_status(),
        }
    }

    /// 시연 모드인가.
    pub(crate) fn demo(&self) -> bool {
        self.demo
    }

    /// 시세 받기를 시작했는지 적는다(시세 흐름이 부른다).
    pub(crate) fn set_started(&self, started: bool) {
        self.started.store(started, Ordering::SeqCst);
    }

    /// 등록이 있을 때까지 기다린다. 기다리기 전에 있었던 등록도 한 번은 알려 준다.
    pub(crate) async fn woken(&self) {
        self.wake.notified().await;
    }

    /// (6차) AI 연결을 고른다: 입력 검사 → 시도 횟수(7차: API 키도 든다) → 작업 폴더 준비 · 저장
    /// (막히는 작업 자리에서) → (CLI면) 확인(모델을 부르지 않는다).
    /// 갖춰졌으면 `Ok(true)`, API 키를 골랐는데 키가 아직 없으면 `Ok(false)`, CLI 확인이 로그인된 구독이
    /// 아니면 그 까닭([`crate::cli_refusal`]), 작업 폴더가 비어 있지 않으면
    /// [`SetupRefusal::CliWorkDirNotEmpty`](고른 것은 저장되고 CLI를 띄우지 않는다),
    /// 꺼지기 시작한 뒤에 잠금을 얻었으면 [`SetupRefusal::ShuttingDown`](아무것도 저장하지 않는다, 8차 3번).
    /// (8차 1번) 호출이 돌 수 있으므로 작업 폴더의 자기 임시 파일은 지우지 않고 비어 있음으로 본다.
    /// 고른 연결로 공급자를 다시 만들고 시세 흐름을 깨운다.
    pub(crate) async fn choose_ai(&self, body: &[u8]) -> Result<bool, SetupRefusal> {
        let choice = AiChoice::parse(body)?;
        let _turn = self.choosing.lock().await;
        if self.closing.load(Ordering::SeqCst) {
            tracing::warn!("서버가 꺼지는 중이라 AI 연결 고르기를 거부합니다");
            return Err(SetupRefusal::ShuttingDown);
        }
        if !allow(&self.limit) {
            tracing::warn!("AI 연결 고르기 시도가 너무 많아 거부합니다");
            return Err(SetupRefusal::TooManyAttempts);
        }
        let work = self.prepare_and_save(&choice).await?;
        let work_blocked = matches!(work, Work::NotEmpty);
        let held = hold(&self.ai_parts, &self.secrets, &self.home, &choice, &work);
        let method = choice.method;
        let new = AiNow {
            choice,
            cli: None,
            work_blocked,
            held: held.clone(),
        };
        let old = std::mem::replace(&mut *self.ai_now(), new);
        if let Held::Cli(cli) = old.held {
            let mut retired = self.retired.lock().unwrap_or_else(PoisonError::into_inner);
            // 목록만 쥔 공급자는 도는 호출이 없다(호출은 공급자를 빌려 쓴다). 빼서 끝없이 쌓이지 않게 한다.
            // 호출이 버려지면 그 자식 그룹은 `pi-ai`가 거둔다.
            retired.retain(|cli| Arc::strong_count(cli) > 1);
            retired.push(cli);
        }
        let outcome = match held {
            _ if work_blocked => Err(SetupRefusal::CliWorkDirNotEmpty),
            Held::Api(_) => Ok(self.has_ai()),
            Held::Cli(cli) => {
                let status = check(&cli).await;
                let refusal = cli_refusal(&status);
                self.ai_now().cli = Some(status);
                refusal.map_or(Ok(true), Err)
            }
        };
        match &outcome {
            Ok(ready) => tracing::info!(method = method.as_str(), ready, "AI 연결을 골랐습니다"),
            Err(why) => {
                tracing::warn!(method = method.as_str(), %why, "고른 AI 연결이 갖춰지지 않았습니다")
            }
        }
        // 갖춰졌으면 시작하고, 빠졌으면 시세 흐름이 다음에 볼 때 멈춘다.
        self.wake.notify_one();
        outcome
    }

    /// (7차 9번) 고른 것의 작업 폴더를 준비하고 고른 것을 저장한다. 막히는 작업 자리에서 한다.
    async fn prepare_and_save(&self, choice: &AiChoice) -> Result<Work, SetupRefusal> {
        let (home, choice) = (self.home.clone(), choice.clone());
        let done = tokio::task::spawn_blocking(move || {
            let work = prepare_work(&home, &choice, Leftovers::Keep)?;
            save_ai_choice(&home, &choice)?;
            Ok::<Work, AppError>(work)
        })
        .await;
        match done {
            Ok(Ok(work)) => Ok(work),
            Ok(Err(error)) => {
                tracing::warn!(%error, "고른 AI 연결을 준비하거나 저장하지 못했습니다");
                Err(SetupRefusal::SaveFailed)
            }
            Err(_) => Err(SetupRefusal::SaveFailed),
        }
    }

    /// (6차) 고른 연결로 만든 공급자.
    pub(crate) fn ai_provider(&self) -> Arc<dyn AiProvider> {
        self.ai_now().held.provider()
    }

    /// 지켜볼 종목을 등록한다: 입력 검사 → 시도 횟수 → 저장하고, 쓰는 종목을 바꾸고,
    /// 목록이 실제로 바뀌었을 때만 시세 흐름을 깨운다(4차).
    ///
    /// 파일을 쓰므로 막히는 작업 자리(`spawn_blocking`)에서 부른다.
    pub(crate) fn register_symbols(&self, body: &[u8]) -> Result<(), SetupRefusal> {
        let symbols = parse_symbols(body)?;
        // 저장과 바꾸기를 한 잠금 안에서 한다(함께 온 등록의 파일과 쓰는 종목이 어긋나지 않게).
        let mut watching = self.watching();
        if watching.source == SymbolsSource::Argument {
            return Err(SetupRefusal::FixedByArgument);
        }
        if !allow(&self.symbols_limit) {
            tracing::warn!("종목 등록 시도가 너무 많아 저장하지 않고 거부합니다");
            return Err(SetupRefusal::TooManyAttempts);
        }
        let changed = watching.symbols != symbols;
        save_watchlist(&self.home, &symbols).map_err(|error| {
            tracing::warn!(%error, "지켜볼 종목을 저장하지 못했습니다");
            SetupRefusal::SaveFailed
        })?;
        watching.symbols = symbols;
        watching.source = SymbolsSource::Saved;
        drop(watching);
        if changed {
            self.wake.notify_one();
        }
        Ok(())
    }
}

/// CLI를 확인한다(버전 → 로그인, 모델을 부르지 않는다). 바깥 끝을 넘으면 확인하지 못한 것이다.
async fn check(cli: &ClaudeCliProvider) -> CliStatus {
    tokio::time::timeout(VERIFY_TIMEOUT, cli.check())
        .await
        .unwrap_or(CliStatus::TimedOut)
}

/// 시도 하나를 센다. 정한 횟수를 넘었으면 `false`.
fn allow(limit: &Mutex<AttemptLimit>) -> bool {
    limit
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .allow(Instant::now())
}

/// 등록 실패를 화면에 알릴 까닭으로 옮긴다.
fn refusal(error: &SecretsError) -> SetupRefusal {
    match error {
        SecretsError::VerifyFailed(VerifyFailure::Rejected) => SetupRefusal::KeyRejected,
        SecretsError::VerifyFailed(VerifyFailure::Unreachable) => SetupRefusal::Unreachable,
        SecretsError::MissingField { .. }
        | SecretsError::UnknownField
        | SecretsError::TooShort { .. }
        | SecretsError::InvalidValue { .. } => SetupRefusal::InvalidInput,
        _ => SetupRefusal::SaveFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 등록_실패는_까닭으로만_옮긴다() {
        let cases = [
            (
                SecretsError::VerifyFailed(VerifyFailure::Rejected),
                SetupRefusal::KeyRejected,
            ),
            (
                SecretsError::VerifyFailed(VerifyFailure::Unreachable),
                SetupRefusal::Unreachable,
            ),
            (SecretsError::UnknownField, SetupRefusal::InvalidInput),
            (
                SecretsError::Io {
                    op: "쓰기",
                    kind: std::io::ErrorKind::PermissionDenied,
                },
                SetupRefusal::SaveFailed,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(refusal(&error), expected, "{error}");
        }
    }
}
