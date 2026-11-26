//! 투자 팀을 만든다: 담당 6명 설정, 실행기 포트(가리는 공급자 · 도구 · 저장소 · 가리기 · 시계), 자료 어댑터, 승인.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_agent::{Ports, Role, RoleConfig, Team, TimeLimits};
use pi_approval::{Approvals, WaitTimes};
use pi_data::{DataAdapter, DataClient, KeySource};
use pi_journal::Journal;
use pi_market::Clock;
use pi_secrets::SecretStore;
use pi_store::{Store, UnixMillis};

use super::guard::{Guarded, Source};
use super::tools::CrewTools;
use super::{Crew, DemoAi, TeamWiring};
use crate::desk::Desk;
use crate::{SecretsRedactor, TeamError, Trading};

/// [`Crew::open`]의 재료.
pub(crate) struct CrewParts {
    pub(crate) store: Store,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) trading: Arc<Trading>,
    pub(crate) desk: Arc<Desk>,
    pub(crate) secrets: Arc<SecretStore>,
    pub(crate) wiring: TeamWiring,
    /// (24차 4번) 참이면 바깥 자료(DART · ECOS · FRED · 네이버)를 부르지 않는다(키가 등록돼 있어도). 실행 파일의
    /// 시연 조립만 켠다.
    pub(crate) offline: bool,
    /// (25차 3번) 시연용 가짜 AI가 답마다 두는 뜸(실행 파일의 시연 조립만 0이 아니다).
    pub(crate) demo_ai_pause: Duration,
}

/// 같은 시계를 실행기 · 자료 · 승인의 시계 자리에 끼운다.
struct Shared(Arc<dyn Clock>);

impl pi_agent::Clock for Shared {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.now().timestamp_millis())
    }
}

impl pi_data::Clock for Shared {
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

impl pi_approval::Clock for Shared {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.now().timestamp_millis())
    }
}

/// 담당 이름(사람이 읽는 말).
pub(crate) fn role_name(role: Role) -> &'static str {
    match role {
        Role::Scout => "뉴스 공시 시세 담당",
        Role::Strategy => "전략 담당",
        Role::Risk => "리스크 담당",
        Role::Review => "복기 담당",
        Role::Trader => "매매 담당",
        Role::Leader => "팀장",
    }
}

/// 담당의 성격 글.
fn soul(role: Role) -> &'static str {
    match role {
        Role::Scout => "뉴스 · 공시 · 시세를 먼저 보고 관련 담당을 부른다.",
        Role::Strategy => "템플릿의 매매 방식에 맞는지 본다.",
        Role::Risk => "손절 · 비중 · 하루 한도를 지키는지 본다.",
        Role::Review => "끝난 매매를 돌아보고 반복되는 실수를 찾는다.",
        Role::Trader => "판단된 매매의 주문 안을 만든다. 수량 · 가격은 규칙 엔진이 정한다.",
        Role::Leader => "합의 점수를 보고 종목 판단과 하루 운용 판단을 정한다.",
    }
}

fn roles(model: &str) -> Vec<RoleConfig> {
    Role::ALL
        .into_iter()
        .map(|role| {
            let mut config = RoleConfig::new(role, model);
            config.soul = soul(role).to_string();
            config
        })
        .collect()
}

/// 자료 어댑터: 출처 주소 · 키로 만든 `DataClient`를 (10b 보강 1번) DART 하루 상한으로 감싼다(주기 가져오기 ·
/// 공시 보기 · 논의가 함께 센다).
fn data_adapter(
    wiring: &TeamWiring,
    keys: Arc<dyn KeySource>,
    shared: &Arc<Shared>,
    store: &Store,
    clock: &Arc<dyn Clock>,
) -> Result<Arc<dyn DataAdapter>, TeamError> {
    let data_clock = Arc::clone(shared) as Arc<dyn pi_data::Clock>;
    let client = DataClient::new(wiring.data.clone(), keys, data_clock)
        .map_err(|error| TeamError::Agent(error.to_string()))?;
    let quota = super::quota::DailyQuota::new(
        store.clone(),
        super::quota::DART_USED_KEY,
        wiring.dart_daily_limit,
    );
    Ok(Arc::new(super::quota::DartQuota::new(
        Arc::new(client),
        quota,
        Arc::clone(clock),
    )))
}

/// 뉴스 창구와 자료 어댑터. (24차 4번) `offline`(실행 파일의 시연 조립)이면 바깥에 묻지 않는다: 뉴스는 키가 없는
/// 것으로 보고, 자료는 시연 자료([`super::demo_data::DemoData`])를 쓴다.
fn sources(
    wiring: &TeamWiring,
    (secrets, offline): (Arc<SecretStore>, bool),
    shared: &Arc<Shared>,
    store: &Store,
    clock: &Arc<dyn Clock>,
) -> Result<(Arc<super::news::NewsDesk>, Arc<dyn DataAdapter>), TeamError> {
    let news = super::news::NewsDesk::open(
        wiring.news.clone(),
        wiring.news_daily_limit,
        Arc::clone(&secrets),
        store.clone(),
        Arc::clone(shared) as Arc<dyn pi_data::Clock>,
    );
    let news = Arc::new(news.offline(offline));
    if offline {
        return Ok((
            news,
            Arc::new(super::demo_data::DemoData::new(Arc::clone(clock))),
        ));
    }
    let keys: Arc<dyn KeySource> = secrets;
    Ok((news, data_adapter(wiring, keys, shared, store, clock)?))
}

impl Crew {
    /// 만든다. 시연 모드면 가짜 AI를, 아니면 고른 AI 연결을 쓴다(둘 다 가리는 공급자로 감싼다).
    pub(crate) fn open(parts: CrewParts) -> Result<Arc<Crew>, TeamError> {
        let CrewParts {
            store,
            clock,
            trading,
            desk,
            secrets,
            wiring,
            offline,
            demo_ai_pause,
        } = parts;
        let shared = Arc::new(Shared(Arc::clone(&clock)));
        let redactor = Arc::new(SecretsRedactor::new(Arc::clone(&secrets)));
        let demo = desk
            .demo()
            .then(|| Arc::new(DemoAi::with_pause(demo_ai_pause)));
        let source = match &demo {
            Some(demo) => Source::Demo(Arc::clone(demo)),
            None => Source::Desk(Arc::clone(&desk)),
        };
        let ai = Arc::new(Guarded {
            source,
            redactor: Arc::clone(&redactor),
        });
        let tools = Arc::new(CrewTools {
            trading: Arc::clone(&trading),
            demo: demo.is_some(),
            placed: Mutex::new(HashMap::new()),
            crew: std::sync::OnceLock::new(),
        });
        let ports = Ports {
            ai,
            tools: Arc::clone(&tools) as Arc<dyn pi_agent::Tools>,
            store: store.clone(),
            redactor: Arc::clone(&redactor) as Arc<dyn pi_ai::Redactor>,
            clock: Arc::clone(&shared) as Arc<dyn pi_agent::Clock>,
        };
        let roles = roles(&wiring.model);
        let team = Team::new(ports.clone(), roles.clone(), TimeLimits::MARKET)
            .map_err(|error| TeamError::Agent(error.to_string()))?;
        let (news, data) = sources(&wiring, (secrets, offline), &shared, &store, &clock)?;
        let approvals = Approvals::new(store.clone(), shared, WaitTimes::default());
        // (22차 6번) 상한 값은 논의를 열 때마다 정한다(켤 때 값, 없으면 설정 화면이 저장한 값).
        let ai_cap =
            super::quota::DailyQuota::new(store.clone(), super::quota::AI_USED_KEY, u32::MAX);
        let meter = super::budget::Meter::new(store.clone());
        let guide = super::guide_book::GuideBook::open(desk.home());
        let crew = Arc::new(Crew {
            guide,
            journal: Journal::new(store.clone()),
            store,
            clock,
            trading,
            desk,
            team,
            roles,
            tools,
            data,
            wiring,
            demo,
            redactor,
            news,
            approvals,
            day_turn: tokio::sync::Mutex::new(()),
            review_turn: tokio::sync::Mutex::new(()),
            scan_turn: tokio::sync::Mutex::new(()),
            links: std::sync::OnceLock::new(),
            discussing: Arc::default(),
            ai_cap,
            ai_cap_told: Mutex::new(None),
            meter,
            ports,
        });
        let _ = crew.tools.crew.set(Arc::downgrade(&crew));
        Ok(crew)
    }

    /// 그 담당의 설정. (57차) 모델은 **지금** 고른 값이다(설정 화면에서 저장한 값, 없으면 조립의
    /// [`TeamWiring::model`]). 그래서 바꾼 모델은 다시 켜지 않아도 다음 턴부터 쓰인다.
    pub(super) fn role(&self, role: Role) -> RoleConfig {
        let mut config = self
            .roles
            .iter()
            .find(|config| config.role == role)
            .unwrap_or(&self.roles[0])
            .clone();
        if let Some(model) = self.desk.team_models.saved_for(role.id()) {
            config.model = model;
        }
        config
    }

    /// (57차) 논의 하나에 쓰는 팀: 지금 고른 담당 모델로 짠다. 논의는 시작할 때 이것을 한 번 받아 끝까지 쓰므로,
    /// 도는 중에 모델을 바꿔도 그 논의는 시작할 때의 모델로 끝난다. 짤 수 없으면(일어나지 않는다) 켤 때의 팀.
    pub(super) fn team_now(&self) -> Team {
        let roles = Role::ALL.into_iter().map(|role| self.role(role)).collect();
        Team::new(self.ports.clone(), roles, TimeLimits::MARKET)
            .unwrap_or_else(|_| self.team.clone())
    }
}
