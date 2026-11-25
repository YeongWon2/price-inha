//! (W5 3차) 검색 · 백업 · 사무실 · 폰을 앱에 잇는다([`crate::Reach`]).

use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::Duration;

use axum::Router;
use pi_approval::ApprovalNumber;
use pi_gate::{GateServer, LockAlert};
use pi_knowledge::Knowledge;
use pi_market::Clock;
use pi_notify::NoticeKind;
use pi_office::EventSink;
use pi_secrets::SecretStore;
use pi_store::{ApprovalStatus, Store};
use serde_json::json;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::App;
use crate::feed::StaleHook;
use crate::messenger::{Messenger, MessengerParts};
use crate::reach::{
    Linked, LockNotices, Looks, Me, OfficeDesk, Phone, Semantic, backup_routes, daily_summary,
    looks_routes, me_routes, office_routes, phone_routes, semantic_routes, talk_routes,
};
use crate::team::CrewLinks;
use crate::trading::{
    Hooks, PROPOSAL_UNAVAILABLE, ProposalHook, approval_agent, approval_state_word,
    approval_status_word, rejected,
};
use crate::{
    AppError, Crew, KIND_OFFICE_APPROVAL, Launch, Reach, SecretsRedactor, Settings, Trading, Wiring,
};

/// 사무실이 팀 상태를 다시 보는 간격.
const OFFICE_SYNC: Duration = Duration::from_millis(100);
/// 미뤄 둔 알림을 다시 보내 볼 간격(앱 시계 기준). 조용한 시간이 끝나면 이 간격 안에 보낸다.
const HELD_RELEASE_MS: i64 = 60_000;

/// 3차 조립의 부품을 연다(`reach`가 없으면 없다).
pub(super) fn open_linked(
    reach: Option<Reach>,
    store: &Store,
    secrets: &Arc<SecretStore>,
    clock: &Arc<dyn Clock>,
) -> Result<Option<Arc<Linked>>, AppError> {
    reach
        .map(|reach| Linked::open(store, secrets, clock, reach).map(Arc::new))
        .transpose()
}

/// 접속 관문에 더 끼울 것: 3차 세션 경로와 잠금 알림(부품이 없으면 둘 다 없다).
pub(super) fn session_parts(
    linked: Option<&Arc<Linked>>,
    trading: &Arc<Trading>,
    crew: Option<&Arc<Crew>>,
    store: &Store,
) -> (Option<Router>, Option<Arc<dyn LockAlert>>) {
    let Some(linked) = linked else {
        return (None, None);
    };
    let desk = Arc::new(OfficeDesk::new(
        Arc::clone(linked),
        crew.cloned(),
        store.clone(),
        Some(Arc::clone(trading)),
    ));
    let phone = Arc::new(Phone {
        linked: Arc::clone(linked),
        trading: Arc::clone(trading),
        crew: crew.cloned(),
        store: store.clone(),
    });
    let alert: Arc<dyn LockAlert> = Arc::new(LockNotices(Arc::clone(linked)));
    let mut routes = office_routes(desk)
        .merge(talk_routes(Arc::clone(&linked.talk)))
        .merge(phone_routes(phone))
        .merge(backup_routes(Arc::clone(linked)));
    // (39차) 메신저를 이었으면 그 경로(읽기는 세션, 바꾸기는 그 컴퓨터에서만)도 단다.
    if let Some(messenger) = linked.messenger() {
        routes = routes.merge(crate::messenger::routes(messenger));
    }
    (Some(routes), Some(alert))
}

/// (39차) 메신저를 연다(메신저 조립 값과 3차 부품이 있을 때만). 알림 자리에 바로 잇는다 — 루프는
/// [`attach`]가 관문이 뜬 뒤에 켠다.
pub(super) fn open_messenger(
    wiring: Option<crate::MessengerWiring>,
    settings: &Settings,
    linked: Option<&Arc<Linked>>,
    trading: &Arc<Trading>,
    store: &Store,
) -> Option<Arc<Messenger>> {
    let (wiring, linked) = (wiring?, linked?);
    let messenger = Messenger::open(
        settings,
        MessengerParts {
            wiring,
            store: store.clone(),
            secrets: Arc::clone(&linked.secrets),
            clock: Arc::clone(&linked.clock),
            trading: Arc::clone(trading),
            linked: Arc::clone(linked),
        },
    );
    linked.attach_messenger(&messenger);
    Some(messenger)
}

/// (10c 14번) 사무실 승인 카드 이벤트([`KIND_OFFICE_APPROVAL`])를 낸다: 번호 · 가린 내용(폰 카드와 같은 것) ·
/// 지문 앞 6자 · 지금 결과. 전체 지문은 싣지 않는다. `answered`면 아직 기다리는 것은 내지 않는다.
async fn office_card(linked: &Linked, trading: &Trading, number: &ApprovalNumber, answered: bool) {
    let view = match trading.approval_view(number.as_str()) {
        Ok(Some(view)) => view,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(%error, "승인 요청을 읽지 못해 사무실 카드를 내지 못했습니다");
            return;
        }
    };
    if answered && view.status == ApprovalStatus::Pending {
        return;
    }
    let card = json!({
        "number": view.number.as_str(),
        "summary": linked.secrets.redact(&view.reason),
        "short_code": view.short_code,
        "status": approval_status_word(view.status),
        // (34차 1번, C27) ✋를 띄울 담당 · 기다림/답함/만료.
        "agent": approval_agent(&view.subject),
        "state": approval_state_word(view.status),
    });
    if let Err(error) = EventSink::publish(linked.sink.as_ref(), KIND_OFFICE_APPROVAL, card).await {
        tracing::warn!(%error, "사무실 승인 카드를 내지 못했습니다");
    }
}

/// (10c 14번) 승인 요청에 답이 나면 같은 번호의 사무실 승인 카드로 결과를 낸다(`answered`).
/// (35차 5번) 규칙 개선안의 승인 요청이 생기면 기다림 카드를 낸다(`answered` 거짓 — 폰 알림은 없다).
fn card_hook(
    linked: &Arc<Linked>,
    trading: Weak<Trading>,
    answered: bool,
) -> crate::trading::AnsweredHook {
    let linked = Arc::clone(linked);
    Box::new(move |number| {
        let (linked, trading) = (Arc::clone(&linked), trading.clone());
        tokio::spawn(async move {
            if let Some(trading) = trading.upgrade() {
                office_card(&linked, &trading, &number, answered).await;
            }
        });
    })
}

/// 승인 요청이 생기면 사무실에 카드를 내고(10c 14번), 폰에 알리고 보낸 결과를 승인 모듈에 남긴다(V10 · P20).
fn approval_hook(linked: &Arc<Linked>, trading: Weak<Trading>) -> crate::trading::ApprovalHook {
    let linked = Arc::clone(linked);
    Box::new(move |number, summary| {
        let (linked, trading) = (Arc::clone(&linked), trading.clone());
        tokio::spawn(async move {
            if let Some(trading) = trading.upgrade() {
                office_card(&linked, &trading, &number, false).await;
            }
            let link = format!("/phone/#/approvals/{}", number.as_str());
            let title = "승인이 필요해요";
            // 구독이 하나도 없으면 "갔다"가 아니다(P20: 승인 경로가 막혔으면 답이 없어도 사지 않는다).
            let short = number.as_str().chars().take(crate::MESSENGER_SHORT_ID_LEN);
            let sent = linked
                .notice_reached(
                    NoticeKind::ApprovalRequest,
                    (title, &summary),
                    Some(link),
                    Some(short.collect()),
                )
                .await;
            if let Some(trading) = trading.upgrade() {
                trading.record_approval_notice(&number, sent);
            }
        });
    })
}

/// (10b 0번 라) 자료 이상 멈춤 알림(N5 · V11). 같은 까닭을 묶는 것은 주문 길이 한다(한 시간에 한 번).
fn data_halt_hook(linked: &Arc<Linked>) -> crate::trading::DataHaltHook {
    let linked = Arc::clone(linked);
    Box::new(move || {
        let body = "자료(공시 · 금리 · 환율 등)를 이어서 받지 못해 이상 감지 멈춤이 됐습니다. \
자료가 다시 들어오면 스스로 다시 시작합니다(같은 까닭의 알림은 한 시간에 한 번 보냅니다)";
        linked.notice_later(
            NoticeKind::Anomaly,
            "자료 이상".to_string(),
            body.to_string(),
            None,
        );
    })
}

/// (10b 보강 5번) 손절을 다시 내지 못하는 알림(N5). 종목 · 수량만 싣는다(까닭 글은 싣지 않는다).
fn stuck_stop_hook(linked: &Arc<Linked>) -> crate::trading::StuckStopHook {
    let linked = Arc::clone(linked);
    Box::new(move |symbol, quantity| {
        let body = format!(
            "손절을 다시 내지 못하고 있습니다: {symbol} {quantity}주. 팀 상태와 장 시간을 확인하세요"
        );
        linked.notice_later(
            NoticeKind::Anomaly,
            "손절을 다시 내지 못하고 있습니다".to_string(),
            body,
            None,
        );
    })
}

/// (16차 3번) 하루 AI 호출 상한에 닿아 논의를 쉬는 알림(N5). 같은 날 한 번은 부르는 쪽이 지킨다.
fn ai_capped_hook(linked: &Arc<Linked>) -> Box<dyn Fn() + Send + Sync> {
    let linked = Arc::clone(linked);
    Box::new(move || {
        linked.notice_later(
            NoticeKind::Anomaly,
            "AI 호출 상한".to_string(),
            "오늘 AI 호출 상한에 닿아 논의를 쉽니다. 손절 · 청산은 그대로 돕니다. 내일 다시 엽니다"
                .to_string(),
            None,
        );
    })
}

/// 하루 마감 뒤: 백업(있으면) → 하루 정리 알림(V48 · V12 · E15).
fn close_hook(
    linked: &Arc<Linked>,
    trading: Weak<Trading>,
    crew: Option<Weak<Crew>>,
) -> crate::trading::CloseHook {
    let linked = Arc::clone(linked);
    Box::new(move |_date| {
        let (linked, trading, crew) = (Arc::clone(&linked), trading.clone(), crew.clone());
        tokio::spawn(async move {
            if linked.backup().is_some()
                && let Err(error) = linked.back_up().await
            {
                tracing::warn!(%error, "마감 뒤 백업이 실패했습니다");
            }
            let Some(trading) = trading.upgrade() else {
                return;
            };
            let crew = crew.and_then(|c| c.upgrade());
            match daily_summary(&linked, &trading, crew.as_ref(), None).await {
                Ok(summary) => {
                    let body = format!(
                        "{} · {}",
                        summary["done"].as_str().unwrap_or_default(),
                        summary["pnl"].as_str().unwrap_or_default()
                    );
                    let link = Some("/phone/#daily-summary".to_string());
                    linked
                        .notice(NoticeKind::DailySummary, "하루 정리", &body, link)
                        .await;
                }
                Err(error) => tracing::warn!(%error, "하루 정리를 만들지 못했습니다"),
            }
        });
    })
}

/// (28차 3번) 폰 · 사무실의 승인 카드 길로 온 규칙 개선안 답을 팀의 개선안 답하기 함수로 보낸다(두 길이 한
/// 함수를 탄다). 팀이 이미 닫혔으면 승인 번호를 건드리지 않고 거절한다.
pub(super) fn proposal_hook(crew: Weak<Crew>) -> ProposalHook {
    Box::new(
        move |proposal, number, shown, decision| match crew.upgrade() {
            Some(crew) => crew.answer_by_card(proposal, number, shown, decision),
            None => Ok(rejected(PROPOSAL_UNAVAILABLE)),
        },
    )
}

/// 관문이 뜬 뒤에 붙인다: 사무실 이벤트 포트, 팀의 지식 검색 · 사무실, 주문 길의 알림 자리.
pub(super) fn attach(
    linked: &Arc<Linked>,
    gate: &GateServer,
    trading: &Arc<Trading>,
    crew: Option<&Arc<Crew>>,
    memory: &Memory,
) {
    linked.sink.attach(gate.publisher());
    if let Some(crew) = crew {
        linked.budget.attach(crew);
        crew.link(CrewLinks {
            knowledge: Arc::clone(&memory.knowledge),
            office: Some(Arc::clone(&linked.office)),
            semantic: Some(Arc::clone(&memory.semantic)),
            ai_capped: Some(ai_capped_hook(linked)),
            talk: Some(Arc::clone(&linked.talk)),
        });
    }
    // (39차) 저장된 토큰이 있으면(시연이 아니면) 받기 루프를 켠다(관문이 이벤트를 받을 수 있게 된 뒤).
    if let Some(messenger) = linked.messenger() {
        tokio::spawn(async move { messenger.start_saved().await });
    }
    trading.hook(Hooks {
        approval: approval_hook(linked, Arc::downgrade(trading)),
        answered: card_hook(linked, Arc::downgrade(trading), true),
        proposal: card_hook(linked, Arc::downgrade(trading), false),
        closed: close_hook(linked, Arc::downgrade(trading), crew.map(Arc::downgrade)),
        data_halt: data_halt_hook(linked),
        stuck_stop: stuck_stop_hook(linked),
    });
}

/// 지식 검색 창구(임베딩 없음)와 (10c 13번) 뜻 검색 설정.
pub(super) struct Memory {
    pub(super) knowledge: Arc<Knowledge>,
    pub(super) semantic: Arc<Semantic>,
}

/// 지식 검색 창구와 뜻 검색 설정을 연다(남긴 설정을 읽기만 한다, 아무것도 묻지 않는다).
pub(super) fn open_memory(
    home: &Path,
    store: &Store,
    secrets: &Arc<SecretStore>,
    crew: Option<&Arc<Crew>>,
) -> Memory {
    let knowledge = Knowledge::new(store.clone(), pi_knowledge::SearchConfig::default());
    let redactor = Arc::new(SecretsRedactor::new(Arc::clone(secrets)));
    let discussing = crew.map(|crew| crew.discussing()).unwrap_or_default();
    Memory {
        knowledge: Arc::new(knowledge),
        semantic: Arc::new(Semantic::open(home, store.clone(), redactor, discussing)),
    }
}

/// 세션 경로에 뜻 검색 설정 화면([`crate::SETUP_SEMANTIC_PATH`])을 더한다.
/// (20차 4번) 담당 외형 경로를 더한다. 사무실 잇기가 없어도(시연 · 2차 조립) 단다 — (21차 6번) 저장소에 남긴다.
/// (32차 1번) 내 캐릭터 경로도 같은 자리에 단다(같은 관문 · 같은 저장소).
pub(super) fn with_looks(
    extra: Option<Router>,
    home: &Path,
    store: &Store,
    clock: &Arc<dyn Clock>,
) -> Option<Router> {
    let routes = looks_routes(Arc::new(Looks::open(
        home,
        store.clone(),
        Arc::clone(clock),
    )))
    .merge(me_routes(Arc::new(Me::new(
        store.clone(),
        Arc::clone(clock),
    ))));
    Some(match extra {
        Some(extra) => extra.merge(routes),
        None => routes,
    })
}

pub(super) fn with_semantic(extra: Option<Router>, semantic: &Arc<Semantic>) -> Option<Router> {
    let routes = semantic_routes(Arc::clone(semantic));
    Some(match extra {
        Some(extra) => extra.merge(routes),
        None => routes,
    })
}

/// 뜻 검색 다시 만들기를 뒤에서 돌린다(걸음 간격 · 한 걸음 조각 수는 팀 조립 값, 팀이 없으면 기본값).
pub(super) fn spawn_semantic(
    semantic: &Arc<Semantic>,
    crew: Option<&Arc<Crew>>,
    clock: &Arc<dyn Clock>,
    stop: &watch::Sender<bool>,
) -> JoinHandle<()> {
    let pace = crew.map_or(
        (
            crate::DEFAULT_REEMBED_INTERVAL,
            crate::DEFAULT_REEMBED_BATCH,
        ),
        |crew| crew.reembed_pace(),
    );
    semantic.spawn(Arc::clone(clock), pace, stop.subscribe())
}

/// 늦은 종목 알림을 이상 알림으로(V11).
pub(super) fn stale_hook(linked: &Arc<Linked>) -> StaleHook {
    let linked = Arc::clone(linked);
    Arc::new(move |notice| linked.on_stale(notice))
}

/// 팀 상태가 바뀌면 사무실 모습을 맞춘다(V15). 조용한 시간이 끝났으면 미뤄 둔 알림을 보낸다
/// ([`HELD_RELEASE_MS`]마다 본다, `Notifier::release_held`). (33차 4번) [`crate::OFFICE_SUMMON_TICK`]마다
/// 사무실 시계를 굴린다(부르기 10초 · 30초, 깨우기 엔진과 따로). `stop`이 참이 되면 끝난다.
pub(super) fn spawn_office_sync(
    linked: &Arc<Linked>,
    store: Store,
    mut stop: watch::Receiver<bool>,
) -> JoinHandle<()> {
    let linked = Arc::clone(linked);
    tokio::spawn(async move {
        let mut shown = store.team_state().ok();
        let mut released: Option<i64> = None;
        let mut ticked = tokio::time::Instant::now();
        loop {
            if ticked.elapsed() >= crate::OFFICE_SUMMON_TICK {
                ticked = tokio::time::Instant::now();
                if let Err(error) = linked.office.tick().await {
                    tracing::warn!(%error, "사무실 시계를 굴리지 못했습니다");
                }
            }
            let now = linked.now().0;
            if released.is_none_or(|at| now - at >= HELD_RELEASE_MS) {
                released = Some(now);
                linked.release_held().await;
            }
            if let Ok(state) = store.team_state()
                && shown != Some(state)
            {
                match linked.office.set_team_state(state).await {
                    Ok(_) => shown = Some(state),
                    Err(error) => tracing::warn!(%error, "사무실 모습을 맞추지 못했습니다"),
                }
            }
            tokio::select! {
                _ = stop.wait_for(|stop| *stop) => return,
                () = tokio::time::sleep(OFFICE_SYNC) => {}
            }
        }
    })
}

impl App {
    /// (W5 3차) [`App::launch`]가 하는 일에 더해 사무실 · 알림 · 지식 검색 · 백업을 잇는다
    /// (`crate::reach` 모듈 문서). 실행 파일은 이것으로 띄운다.
    pub async fn launch_with(
        settings: Settings,
        wiring: Wiring,
        launch: Launch,
        reach: Reach,
    ) -> Result<App, AppError> {
        let messenger = crate::MessengerWiring::default();
        Box::pin(App::launch_with_messenger(
            settings, wiring, launch, reach, messenger,
        ))
        .await
    }

    /// (39차) [`App::launch_with`]가 하는 일에 더해 메신저를 `messenger` 조립 값으로 잇는다(`crate::messenger`
    /// 문서의 계약: 경로 · 받기 루프 · 명령 · 나가는 알림). [`App::launch_with`]는 이것을
    /// [`crate::MessengerWiring::default`]로 부른 것과 같다. 시연 모드(`settings.demo`)면 루프를 돌리지 않는다.
    pub async fn launch_with_messenger(
        settings: Settings,
        wiring: Wiring,
        launch: Launch,
        reach: Reach,
        messenger: crate::MessengerWiring,
    ) -> Result<App, AppError> {
        let demo = super::DemoRun::default();
        let reach = Some((reach, messenger));
        Box::pin(App::assemble(settings, wiring, launch, reach, demo)).await
    }

    /// (W5 3차) 지식 검색 창구(같은 저장소). 임베딩은 없다.
    pub fn knowledge(&self) -> Arc<Knowledge> {
        Arc::clone(&self.knowledge)
    }

    fn linked(&self) -> Result<&Arc<Linked>, AppError> {
        self.linked.as_ref().ok_or(AppError::BackupNotConfigured)
    }

    /// (W5 3차) 지금 한 번 백업한다(하루 마감 뒤에 하는 것과 같은 길). 시각은 [`crate::Wiring::clock`].
    /// 백업을 잇지 않았으면 [`AppError::BackupNotConfigured`], 실패하면 백업 실패 알림을 내고
    /// [`AppError::Backup`].
    pub async fn back_up_now(&self) -> Result<pi_backup::BackupReport, AppError> {
        self.linked()?.back_up().await
    }

    /// (W5 3차) 백업 상태. 백업을 잇지 않았으면 [`AppError::BackupNotConfigured`].
    pub fn backup_status(&self) -> Result<pi_backup::BackupStatus, AppError> {
        self.linked()?.backup_status()
    }
}
