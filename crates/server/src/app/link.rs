//! (W5 3차) 검색 · 백업 · 사무실 · 폰을 앱에 잇는다([`crate::Reach`]).

use std::sync::{Arc, Weak};
use std::time::Duration;

use axum::Router;
use pi_gate::{GateServer, LockAlert};
use pi_knowledge::Knowledge;
use pi_market::Clock;
use pi_notify::NoticeKind;
use pi_secrets::SecretStore;
use pi_store::Store;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::App;
use crate::feed::StaleHook;
use crate::reach::{
    Linked, LockNotices, OfficeDesk, Phone, backup_routes, daily_summary, office_routes,
    phone_routes,
};
use crate::team::CrewLinks;
use crate::trading::Hooks;
use crate::{AppError, Crew, Launch, Reach, Settings, Trading, Wiring};

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
    ));
    let phone = Arc::new(Phone {
        linked: Arc::clone(linked),
        trading: Arc::clone(trading),
        crew: crew.cloned(),
    });
    let alert: Arc<dyn LockAlert> = Arc::new(LockNotices(Arc::clone(linked)));
    (
        Some(
            office_routes(desk)
                .merge(phone_routes(phone))
                .merge(backup_routes(Arc::clone(linked))),
        ),
        Some(alert),
    )
}

/// 승인 요청이 생기면 폰에 알리고 보낸 결과를 승인 모듈에 남긴다(V10 · P20).
fn approval_hook(linked: &Arc<Linked>, trading: Weak<Trading>) -> crate::trading::ApprovalHook {
    let linked = Arc::clone(linked);
    Box::new(move |number, summary| {
        let (linked, trading) = (Arc::clone(&linked), trading.clone());
        tokio::spawn(async move {
            let link = format!("/phone/#/approvals/{}", number.as_str());
            let title = "승인이 필요해요";
            let sent = linked
                .notice(NoticeKind::ApprovalRequest, title, &summary, Some(link))
                .await;
            if let Some(trading) = trading.upgrade() {
                trading.record_approval_notice(&number, sent);
            }
        });
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
            match daily_summary(&linked, &trading, crew.as_ref()).await {
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

/// 관문이 뜬 뒤에 붙인다: 사무실 이벤트 포트, 팀의 지식 검색 · 사무실, 주문 길의 알림 자리.
pub(super) fn attach(
    linked: &Arc<Linked>,
    gate: &GateServer,
    trading: &Arc<Trading>,
    crew: Option<&Arc<Crew>>,
    knowledge: &Arc<Knowledge>,
) {
    linked.sink.attach(gate.publisher());
    if let Some(crew) = crew {
        crew.link(CrewLinks {
            knowledge: Arc::clone(knowledge),
            office: Some(Arc::clone(&linked.office)),
        });
    }
    trading.hook(Hooks {
        approval: approval_hook(linked, Arc::downgrade(trading)),
        closed: close_hook(linked, Arc::downgrade(trading), crew.map(Arc::downgrade)),
    });
}

/// 늦은 종목 알림을 이상 알림으로(V11).
pub(super) fn stale_hook(linked: &Arc<Linked>) -> StaleHook {
    let linked = Arc::clone(linked);
    Arc::new(move |notice| linked.on_stale(notice))
}

/// 팀 상태가 바뀌면 사무실 모습을 맞춘다(V15). 조용한 시간이 끝났으면 미뤄 둔 알림을 보낸다
/// ([`HELD_RELEASE_MS`]마다 본다, `Notifier::release_held`). `stop`이 참이 되면 끝난다.
pub(super) fn spawn_office_sync(
    linked: &Arc<Linked>,
    store: Store,
    mut stop: watch::Receiver<bool>,
) -> JoinHandle<()> {
    let linked = Arc::clone(linked);
    tokio::spawn(async move {
        let mut shown = store.team_state().ok();
        let mut released: Option<i64> = None;
        loop {
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
        Box::pin(App::assemble(settings, wiring, launch, Some(reach))).await
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
