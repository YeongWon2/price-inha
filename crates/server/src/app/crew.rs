//! 투자 팀 조립: 하루 AI 호출 상한 · 팀 열기 · 깨우기 엔진(조립 몸통 [`super::App::assemble`]이 부른다).

use std::sync::Arc;
use std::time::Duration;

use pi_market::Clock;
use pi_secrets::SecretStore;
use pi_store::Store;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::{Wiring, link};
use crate::demo::DemoWiring;
use crate::desk::Desk;
use crate::team::{CrewParts, spawn_trigger};
use crate::{AppError, Crew, Settings, Trading};

/// 팀이 있으면 깨우기 엔진을 띄운다.
/// (24차) `demo`는 시연 값(꺼짐이면 지금 동작 그대로).
pub(super) async fn crew_trigger(
    crew: Option<&Arc<Crew>>,
    clock: &Arc<dyn Clock>,
    stop: &watch::Sender<bool>,
    demo: &DemoWiring,
) -> Result<Option<JoinHandle<()>>, AppError> {
    match crew {
        Some(crew) => Ok(Some(
            spawn_trigger(crew, Arc::clone(clock), stop.subscribe(), demo).await?,
        )),
        None => Ok(None),
    }
}

/// 투자 팀을 잇는다(`team`이 없으면 잇지 않는다).
/// (17차 4번) 설정의 하루 AI 호출 상한을 팀 조립에 잇는다(팀 조립에 이미 적힌 값이 앞선다). (22차 6번) 그 켤 때 값을
/// 설정 책상에도 적는다(설정 상태에 보이고, 화면에서 저장한 값보다 앞선다).
pub(super) fn team_with_limit(
    team: Option<crate::TeamWiring>,
    settings: &Settings,
    desk: &Desk,
) -> Option<crate::TeamWiring> {
    team.map(|mut team| {
        team.ai_daily_limit = team.ai_daily_limit.or(settings.ai_daily_limit);
        desk.set_launch_ai_limit(team.ai_daily_limit);
        // (57차) 조립의 담당 모델이 설정 화면의 기본값이다.
        desk.team_models.set_default(&team.model);
        team
    })
}

/// (24차) `stored.1`(offline)이 참이면 팀이 바깥 자료(DART · ECOS · FRED · 네이버)를 부르지 않는다.
/// (25차 3번) `stored.2`는 시연용 가짜 AI가 답마다 두는 뜸이다.
pub(super) fn open_crew(
    team: Option<crate::TeamWiring>,
    stored: (&Store, bool, Duration),
    wiring: &Wiring,
    trading: &Arc<Trading>,
    desk: &Arc<Desk>,
    secrets: &Arc<SecretStore>,
) -> Result<Option<Arc<Crew>>, AppError> {
    let Some(team) = team else {
        return Ok(None);
    };
    let (store, offline, demo_ai_pause) = stored;
    let crew = Crew::open(CrewParts {
        store: store.clone(),
        clock: Arc::clone(&wiring.clock),
        trading: Arc::clone(trading),
        desk: Arc::clone(desk),
        secrets: Arc::clone(secrets),
        wiring: team,
        offline,
        demo_ai_pause,
    })?;
    // (28차 3번) 승인 카드 길로 온 개선안 답이 개선안 답하기와 같은 함수를 타게 잇는다.
    trading.hook_proposals(link::proposal_hook(Arc::downgrade(&crew)));
    Ok(Some(crew))
}
