//! 투자 팀을 만든다: 담당 6명 설정, 실행기 포트(가리는 공급자 · 도구 · 저장소 · 가리기 · 시계), 자료 어댑터, 승인.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
pub(super) fn role_name(role: Role) -> &'static str {
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
        } = parts;
        let shared = Arc::new(Shared(Arc::clone(&clock)));
        let redactor = Arc::new(SecretsRedactor::new(Arc::clone(&secrets)));
        let demo = desk.demo().then(|| Arc::new(DemoAi::new()));
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
        let team = Team::new(ports, roles.clone(), TimeLimits::MARKET)
            .map_err(|error| TeamError::Agent(error.to_string()))?;
        let news = Arc::new(super::news::NewsDesk::open(
            wiring.news.clone(),
            wiring.news_daily_limit,
            Arc::clone(&secrets),
            store.clone(),
            Arc::clone(&shared) as Arc<dyn pi_data::Clock>,
        ));
        let keys: Arc<dyn KeySource> = secrets;
        let data_clock = Arc::clone(&shared) as Arc<dyn pi_data::Clock>;
        let client = DataClient::new(wiring.data.clone(), keys, data_clock)
            .map_err(|error| TeamError::Agent(error.to_string()))?;
        let approvals = Approvals::new(store.clone(), shared, WaitTimes::default());
        let crew = Arc::new(Crew {
            journal: Journal::new(store.clone()),
            store,
            clock,
            trading,
            desk,
            team,
            roles,
            tools,
            data: Arc::new(client) as Arc<dyn DataAdapter>,
            wiring,
            demo,
            redactor,
            news,
            approvals,
            day_turn: tokio::sync::Mutex::new(()),
            review_turn: tokio::sync::Mutex::new(()),
            scan_turn: tokio::sync::Mutex::new(()),
            links: std::sync::OnceLock::new(),
        });
        let _ = crew.tools.crew.set(Arc::downgrade(&crew));
        Ok(crew)
    }

    /// 그 담당의 설정.
    pub(super) fn role(&self, role: Role) -> &RoleConfig {
        self.roles
            .iter()
            .find(|config| config.role == role)
            .unwrap_or(&self.roles[0])
    }
}
