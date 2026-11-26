//! (W5 2차) 에이전트 · 자료 · 트리거 · 복기 잇기: T9 · T10 · T11 · T12 · T15 연결.
//!
//! 작업 지시서: `docs/dev/modules/server.md` "2차"와 "트리거를 조립할 때".
//!
//! - 에이전트(`pi_agent::Team`): 담당 6명. AI는 고른 연결의 공급자(시연 모드면 [`DemoAi`])를 쓰되,
//!   **가리기 공급자**로 감싼다: 나가는 글뿐 아니라 받은 답 · 공급자 오류 글도 비밀 보관소로 가린 뒤에
//!   실행기에 넘긴다. 그래서 방 로그 · 세션 · 복기 기록에는 가린 글만 저장된다(agent 1차 검토 참고,
//!   knowledge 1차 검토 참고: 기록은 가린 글만). 도구 결과도 가린 뒤 넘긴다.
//! - 자료(`pi_data::DataClient`): 공시 · 재무(DART) · 금리 · 환율(ECOS). 키는 비밀 보관소의 출처별 칸.
//!   받은 자료는 저장소에 쌓고(`Store::save_facts`), 판단된 매매에 붙인다(`Store::attach_trade_facts`, V33~V36).
//!   저장하는 자료의 종류 이름은 `disclosure` · `financials` · `korea_rate` · `korea_fx` · `us_rate`이고,
//!   출처 이름은 `pi_data::SourceName::as_str`(`DART` · `ECOS` · `FRED`), 출처 주소는 받은 주소(키는 가린 것)다.
//! - 트리거(`pi_trigger::Trigger`): 장 시작 · 마감 뒤 일정으로 팀을 깨운다. 작업 상태와 영수증은 저장소에 둔다
//!   (다시 켜도 같은 회차를 두 번 돌리지 않는다, E16). `Trigger::run`만 돌리고 `settle`은 `select!` 안에 두지 않는다.
//! - 복기(`pi_journal::Journal`): 마감 뒤 팔린 매매를 복기 담당이 돌아보고(V19), 같은 실수가 2건이면
//!   개선안을 내고 승인 요청을 만든다(V20). 답은 [`PROPOSAL_ANSWER_PATH`](V21 · E35 · E28).
//! - 다른 모듈의 로직을 다시 만들지 않는다(조립만 한다). 시각은 [`crate::Wiring::clock`]에서만 읽는다.

mod build;
mod changes;
mod day;
mod demo_ai;
mod demo_data;
mod demo_feed;
mod facts;
mod guard;
mod http;
mod link;
mod memo;
mod news;
mod notes;
mod notices;
mod poll;
mod quota;
mod review;
mod rules;
mod scout;
mod seen;
mod talk;
mod tools;
mod wake;
// (50차) 근거 카드 칸: 근거 경로 · 히스토리의 매매 하나가 함께 쓴다.
pub(crate) mod card;
// (51차) 하루 AI 잔량 · 논의 진행 단계.
mod budget;
mod progress;

use std::collections::HashMap;
use std::sync::Arc;

use pi_agent::{DiscussionOutcome, RoleConfig, Team};
use pi_approval::Approvals;
use pi_data::{CorpCode, DataAdapter, DataConfig, EcosCycle, EcosSeries, NaverNewsConfig};
use pi_journal::{Journal, ReviewRecord};
use pi_market::{Clock, Symbol};
use pi_store::{ProposalId, Store, TradeId};
use pi_trigger::ScheduleTimes;

use crate::desk::Desk;
use crate::{DayPlan, Placed, SecretsRedactor, TeamError, Trading};

pub(crate) use build::CrewParts;
pub(crate) use build::role_name;
pub(crate) use http::routes;
pub(crate) use link::{CrewLinks, role_of};
pub(crate) use wake::spawn_trigger;

pub use budget::{KIND_TEAM_AI_USED, TEAM_BUDGET_PATH};
pub use demo_ai::{DemoAi, DemoTurn};
pub use progress::KIND_OFFICE_PROGRESS;

/// 투자 팀 담당 명단(V1). `GET` → `{"members": [{"id": 담당 id, "name": 담당 이름}, …]}`.
/// 명단 순서(`pi_agent::Role::ALL`)로 6명이다. 세션이 있어야 열린다.
pub const TEAM_MEMBERS_PATH: &str = "/api/team/members";

/// 매매 근거 읽기(C6 · V18). `GET /api/trades/{id}/basis` → `{"id": 매매 번호, "summary": 쉬운 말 요약}`.
/// 요약은 `pi_journal::Journal::read_basis`의 것 그대로다. 없는 매매는 404 `{"error": "not_found"}`.
/// 세션이 있어야 열린다.
pub const TRADE_BASIS_PATH: &str = "/api/trades/{id}/basis";

/// 규칙 개선안 목록. `GET` → `{"proposals": [{"id", "state": "proposed" | "cooling" | "applied" | "rejected",
/// "mistake": 실수(글자), "increases_risk": bool, "evidence": [매매 번호], "number": 승인 번호 | null,
/// "fingerprint": 지문 | null, "changes": [{"key", "before", "after", "widened"}]}]}`(번호 순서).
/// 세션이 있어야 열린다.
///
/// (28차 2번) `changes`는 개선안의 앞 → 뒤 값 차이다: 복기 개선안이면 내용(`pi_journal::Improvement`)의
/// `before` → `after`를 `pi_template::diff_params`로 낸 것(칸 순서 · 값 모양이 [`crate::TEMPLATES_VIEW_PATH`]의
/// 판 이력 `changes`와 같다), 읽을 수 없는 내용이면 빈 목록이다.
pub const PROPOSALS_PATH: &str = "/api/proposals";

/// (28차 1번 · C29 · V82) 규칙 읽기. `GET` → 200
///
/// ```text
/// {
///   "template": 지금 쓰는 템플릿 번호 | null,
///   "fields": [{"key", "value", "cap": 십진수 글자 | null}],
///   "caps": {"<상한이 있는 칸 키>": 십진수 글자, …},
///   "cooling": [{"proposal": 개선안 번호, "changes": [{"key", "before", "after", "widened"}],
///                "applies_on": "next_trading_day"}],
///   "recent": [{"proposal": 개선안 번호, "applied_at": 유닉스 밀리초,
///               "changes": [{"key", "before", "after", "widened"}]}]
/// }
/// ```
///
/// - `fields`는 지금 쓰는 템플릿(가장 나중에 고른 것)의 RiskParams를 `pi_template::template_view`의 칸 순서 ·
///   키 · 값 · 상한으로 낸 것이다(편집 여부 칸은 없다 — 규칙 화면은 보기만 한다). 템플릿이 없으면 `template`은
///   `null`이고 `fields`는 빈 목록이다.
/// - `caps`는 원칙의 절대 상한(`pi_template::AbsoluteCaps::FIXED`)이다: `template_view`에서 상한이 있는 여섯 칸의
///   키 → 상한. 템플릿이 없어도 늘 있다.
/// - `cooling`은 냉각중인 개선안(승인됐고 위험을 키워 다음 거래일 시작에 반영된다, E28)의 앞 → 뒤 값이다
///   (번호 순서). `changes`는 [`PROPOSALS_PATH`]의 것과 같다.
/// - `recent`는 반영된 개선안을 반영 시각이 늦은 것부터 최대 [`RULES_RECENT_LIMIT`]개 낸다(F5-8 "바뀐 규칙이
///   보인다"). `applied_at`은 저장소의 반영 시각이다. 제안됨 · 거절됨 개선안은 `cooling` · `recent` 어디에도 없다.
/// - 세션이 있어야 열린다(원격은 세션 없이 401). 개선안 목록 · 답하기는 [`PROPOSALS_PATH`] ·
///   [`PROPOSAL_ANSWER_PATH`]를 그대로 쓴다.
pub const RULES_VIEW_PATH: &str = "/api/rules";

/// 규칙 읽기의 `recent`에 담는 반영된 개선안의 가장 많은 수.
pub const RULES_RECENT_LIMIT: usize = 5;

/// 규칙 개선안 답하기(V21 · E35). `POST` `{"id": 개선안 번호, "number": 승인 번호, "fingerprint": 지문,
/// "answer": "approve" | 그 밖}`. 세션과 같은 출처가 있어야 한다. 응답은 언제나 200
/// `{"result": "applied" | "cooling" | "rejected", "reason": 글자 | null}`.
///
/// - 승인이고 승인 번호 · 지문이 그 개선안의 승인 요청과 맞으면 승인 모듈이 승인을 소비한 뒤에만 반영한다.
///   위험을 키우지 않으면 `"applied"`: 지금 쓰는 템플릿의 새 판이 개선안의 값이 된다(다음 매매부터 적용).
///   위험을 키우면 `"cooling"`: 템플릿은 그대로이고 다음 거래일 시작에 반영한다(E28).
/// - 승인 번호가 없거나 맞지 않으면 `"rejected"`, 이유 [`crate::ANSWER_NOT_FOUND`] 등(E35). 개선안 · 템플릿은 그대로다.
/// - 거절이면 `"rejected"`, 이유 `"user_rejected"`. 개선안은 거절됨이고 템플릿은 그대로다.
/// - (10a 보강) 승인인데 쓰는 템플릿의 값이 잠겨 있으면(승격대기 · 실전중) 승인을 소비하기 전에 거절한다:
///   `"rejected"`, 이유 `"template_locked"`. 승인 번호는 그대로 기다리고 개선안 · 템플릿도 그대로다(V21 · E28).
///
/// (28차 3번) 폰 · 사무실의 승인 카드 길([`crate::APPROVAL_DECIDE_PATH`] · [`crate::APPROVAL_ANSWER_PATH`])로
/// 규칙 개선안의 승인 번호에 답해도 이 경로와 같은 길을 탄다(승인 소비 · 반영 · 냉각 · 거절이 한 길이다).
/// 승인 카드 길의 응답은 그 길의 말을 쓴다: 반영 · 냉각중이면 200 `{"result": "approved", "reason": null}`,
/// 거절이면 200 `{"result": "rejected", "reason": "user_rejected"}`, 승인인데 반영되지 않았으면(이미 답함 ·
/// 지문 다름 · 템플릿 잠김 등) 409 `{"result": "rejected", "reason": 이 경로와 같은 이유}`.
pub const PROPOSAL_ANSWER_PATH: &str = "/api/proposals/answer";

/// 저장되는 이벤트 종류: 팀장의 하루 운용 판단 논의(V5, 1차에서 넘긴 몫).
/// 내용 `{"date": "YYYY-MM-DD", "plan": "trade" | "rest", "reason": 쉬는 이유 | null, "text": 팀장의 글(가린 것)}`.
/// 팀장의 글은 방 [`DAY_ROOM`]의 방 로그에도 `message.member`(쓴 쪽 `leader`)로 남는다.
pub const KIND_DAY_DISCUSSION: &str = "day_discussion";

/// 저장되는 이벤트 종류: 규칙 개선안을 내고 승인 요청을 만들었다(V20).
/// 내용 `{"id": 개선안 번호, "number": 승인 번호, "fingerprint": 지문, "mistake": 실수(글자)}`.
pub const KIND_PROPOSAL: &str = "rule_proposal";

/// 하루 운용 판단을 남기는 방 이름.
pub const DAY_ROOM: &str = "day";

/// 매매 논의 방 이름의 머리(`trade-<매매 번호>`).
pub const TRADE_ROOM_PREFIX: &str = "trade-";

/// 합의 점수 기준의 기본값. 템플릿에 칸이 없어 조립이 정한다: 점수를 내는 담당 3명(뉴스 공시 시세 ·
/// 전략 · 리스크)의 평균 60점을 넘어야 한다(합 180 초과, E27).
pub const DEFAULT_CONSENSUS_REQUIRED: i32 = 180;

/// 담당이 쓰는 모델 이름의 기본값. 조정자 결정, docs/dev/modules/server.md 2차 보강.
pub const DEFAULT_TEAM_MODEL: &str = "claude-sonnet-5-5";

/// (10차 10b 8번) 자료(새 공시 · 새 뉴스 · 재무/거시 값)를 다시 보는 간격의 기본값(시계 기준).
/// 네이버 하루 한도([`DEFAULT_NEWS_DAILY_LIMIT`])를 넘지 않게 실제 간격은 종목 수로 더 늘어날 수 있다.
pub const DEFAULT_DATA_POLL: std::time::Duration = std::time::Duration::from_secs(60);

/// (10차 10b 8번) 네이버 뉴스 검색의 하루 요청 상한 기본값(네이버 공식 한도 25,000회).
pub const DEFAULT_NEWS_DAILY_LIMIT: u32 = 25_000;

/// (10차 10b 보강 1번) OpenDART 하루 요청 상한 기본값(개인 키 한도 약 20,000건의 절반 — 공식 문서로 다시 확인).
pub const DEFAULT_DART_DAILY_LIMIT: u32 = 10_000;

/// (10차 10c 13번) 뜻 검색이 켜져 있을 때 기존 기록의 임베딩을 다시 만드는 한 걸음의 간격(시계 기준). 한가할 때만 돈다.
pub const DEFAULT_REEMBED_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// (10차 10c 13번) 임베딩을 다시 만드는 한 걸음에서 많아야 몇 조각(`pi_knowledge::Knowledge::reembed_step`의 `batch`).
pub const DEFAULT_REEMBED_BATCH: usize = 32;

/// 투자 팀 조립의 바깥 값. [`crate::Launch::team`]에 준다.
#[derive(Debug, Clone)]
pub struct TeamWiring {
    /// 자료 출처 주소(DART · ECOS · FRED). 테스트는 로컬 가짜 서버를 넣는다.
    pub data: DataConfig,
    /// 종목 → DART 고유번호. 없는 종목은 공시 · 재무를 받을 수 없다(자료가 비어 지켜보기, E32).
    pub corp_codes: HashMap<Symbol, CorpCode>,
    /// 국내 금리 계열. 통계표 · 항목 · 주기만 쓰고 기간(`start` · `end`)은 그날 기준으로 조립이 채운다
    /// (월: 석 달 전 ~ 이번 달, 일: 열흘 전 ~ 오늘).
    pub korea_rate: EcosSeries,
    /// 환율 계열. 기간은 금리와 같은 규칙으로 채운다.
    pub korea_fx: EcosSeries,
    /// 공시를 볼 날 수(오늘을 포함해 거꾸로).
    pub disclosure_days: u32,
    /// 합의 점수 기준(E27).
    pub consensus_required: i32,
    /// 일정 간격(장 시작 전 브리핑 · 마감 뒤).
    pub schedule: ScheduleTimes,
    /// 담당 6명이 쓰는 모델 이름(고른 AI 연결에 그대로 넘긴다).
    pub model: String,
    /// (10차 10b 9번) 네이버 뉴스 검색 설정(기본 주소 · 시간 한도). 테스트는 로컬 가짜 서버를 넣는다.
    /// 키는 비밀 보관소의 `pi_data::NAVER_NEWS_KEYS` 칸이다(`NewsKeySource`).
    pub news: NaverNewsConfig,
    /// (10차 10b 8번) 자료를 다시 보는 간격(시계 기준). 기본 [`DEFAULT_DATA_POLL`].
    pub data_poll: std::time::Duration,
    /// (10차 10b 8번) 네이버 뉴스 검색의 하루 요청 상한. 주기와 종목 수로 이 수를 넘지 않게 한다.
    /// 기본 [`DEFAULT_NEWS_DAILY_LIMIT`].
    pub news_daily_limit: u32,
    /// (10차 10b 보강 1번) DART 하루 요청 상한(주기 가져오기 · 논의의 자료 받기를 함께 센다).
    /// 간격을 `하루 × 종목 수 ÷ 상한` 이상으로 두고 하루 수를 세어 저장소에 남긴다. 상한에 닿으면 그날은
    /// "공시 없음(한도)"으로 두고 자료 이상으로 세지 않는다. 기본 [`DEFAULT_DART_DAILY_LIMIT`].
    pub dart_daily_limit: u32,
    /// (10차 10c 13번) 뜻 검색이 켜졌을 때 임베딩을 다시 만드는 걸음 간격(시계 기준). 기본 [`DEFAULT_REEMBED_INTERVAL`].
    pub reembed_interval: std::time::Duration,
    /// (10차 10c 13번) 한 걸음에서 다시 만드는 조각 수 상한. 기본 [`DEFAULT_REEMBED_BATCH`].
    pub reembed_batch: usize,
    /// (16차 3번) 하루 AI 호출 상한: 한국 시각 하루 안에 팀 논의 · 깨우기로 AI를 부른 횟수의 상한.
    /// 비우면(`None`, 기본) 상한이 없다(스펙 그대로). 켜져 있고 상한에 닿으면 그날 남은 논의 · 깨우기를 쉬고
    /// 사용자에게 한 번 알린다. 손절 · 청산 · 이상 감지 · 한도 판단은 AI를 부르지 않아 그대로 돈다.
    /// 센 횟수는 저장돼 다시 켜도 그날 안에서는 이어진다.
    pub ai_daily_limit: Option<u32>,
}

/// 공식 조립이 아는 종목 → DART 고유번호(시가총액이 큰 국내 종목). 표에 없는 종목은 공시 · 재무를 받지
/// 않는다(자료가 비어 지켜보기, E32). 다른 종목은 [`TeamWiring::corp_codes`]에 더한다.
pub fn known_corp_codes() -> HashMap<Symbol, CorpCode> {
    // 확인한 값만 둔다(틀린 번호는 다른 회사의 공시를 가져온다). 다른 종목은 corpCode.xml에서 찾아 더한다.
    const KNOWN: [(&str, &str); 2] = [
        ("005930", "00126380"), // 삼성전자
        ("000660", "00164779"), // SK하이닉스
    ];
    KNOWN
        .iter()
        .filter_map(|(symbol, corp)| Some((Symbol::new(*symbol), CorpCode::parse(corp).ok()?)))
        .collect()
}

impl TeamWiring {
    /// 공식 주소 · 큰 종목의 고유번호 표([`known_corp_codes`]) · 한국은행 기준금리(722Y001/0101000, 월) · 원/달러 환율(731Y001/0000001, 일) ·
    /// 공시 7일 · [`DEFAULT_CONSENSUS_REQUIRED`] · 기본 일정 간격 · 모델 [`DEFAULT_TEAM_MODEL`].
    pub fn official() -> TeamWiring {
        TeamWiring {
            data: DataConfig::official(),
            corp_codes: known_corp_codes(),
            korea_rate: series("722Y001", "0101000", EcosCycle::Month),
            korea_fx: series("731Y001", "0000001", EcosCycle::Day),
            disclosure_days: 7,
            consensus_required: DEFAULT_CONSENSUS_REQUIRED,
            schedule: ScheduleTimes::default(),
            model: DEFAULT_TEAM_MODEL.to_string(),
            news: NaverNewsConfig::official(),
            data_poll: DEFAULT_DATA_POLL,
            news_daily_limit: DEFAULT_NEWS_DAILY_LIMIT,
            dart_daily_limit: DEFAULT_DART_DAILY_LIMIT,
            reembed_interval: DEFAULT_REEMBED_INTERVAL,
            reembed_batch: DEFAULT_REEMBED_BATCH,
            ai_daily_limit: None,
        }
    }
}

/// 기간이 빈 ECOS 계열.
fn series(stat: &str, item: &str, cycle: EcosCycle) -> EcosSeries {
    EcosSeries {
        stat_code: stat.to_string(),
        item_code: item.to_string(),
        cycle,
        start: String::new(),
        end: String::new(),
    }
}

/// 매매 논의 하나의 결과.
#[derive(Debug, Clone)]
pub struct Discussed {
    /// 논의한 매매(논의중으로 새로 만든 것).
    pub trade: TradeId,
    /// 실행기의 결과(판단 · 점수 · 합의 점수 · 주문 안).
    pub outcome: DiscussionOutcome,
    /// 주문 안을 주문 길([`crate::Trading::place`])에 넣은 결과. 주문 안이 없으면 `None`.
    pub placed: Option<Placed>,
}

/// 하루 복기의 결과.
#[derive(Debug, Clone, Default)]
pub struct Reviewed {
    /// 이번에 남긴 복기 기록(매매 번호 순서).
    pub reviews: Vec<ReviewRecord>,
    /// 이번에 낸 규칙 개선안(승인 요청까지 만든 것).
    pub proposals: Vec<ProposalId>,
}

/// 조립된 투자 팀. [`crate::App::crew`]로 받는다. 트리거가 장 시작 · 마감 뒤에 아래 일을 부르고,
/// 테스트는 같은 일을 직접 부를 수도 있다.
pub struct Crew {
    store: Store,
    clock: Arc<dyn Clock>,
    trading: Arc<Trading>,
    desk: Arc<Desk>,
    team: Team,
    roles: Vec<RoleConfig>,
    tools: Arc<tools::CrewTools>,
    data: Arc<dyn DataAdapter>,
    wiring: TeamWiring,
    demo: Option<Arc<DemoAi>>,
    redactor: Arc<SecretsRedactor>,
    /// (10b 9번) 뉴스 공급자 등록부와 네이버 하루 상한.
    news: Arc<news::NewsDesk>,
    approvals: Approvals,
    journal: Journal,
    /// 하루 판단 · 복기 · 공시 보기를 한 줄로 세운다(트리거와 부르는 쪽이 겹쳐도 한 번씩).
    day_turn: tokio::sync::Mutex<()>,
    review_turn: tokio::sync::Mutex<()>,
    scan_turn: tokio::sync::Mutex<()>,
    /// (W5 3차) 지식 검색 · 사무실(3차 조립이 붙인다).
    links: std::sync::OnceLock<CrewLinks>,
    /// (10c 13번) 지금 논의 중인 수. 뜻 검색 다시 만들기는 이것이 0일 때만 걷는다.
    discussing: Arc<std::sync::atomic::AtomicUsize>,
    /// (16차 3번) 하루 AI 호출 상한의 그날 센 수. 논의 하나를 열 때마다 한 번 센다. 상한 값은 (22차 6번) 켤 때 값
    /// ([`TeamWiring::ai_daily_limit`]), 없으면 설정 화면이 저장한 값이다(둘 다 없으면 세지 않는다).
    ai_cap: quota::DailyQuota,
    /// (16차 3번) 상한에 닿았다고 알린 마지막 날(같은 날 한 번만 알린다).
    ai_cap_told: std::sync::Mutex<Option<chrono::NaiveDate>>,
    /// (51차) 담당별 하루 AI 사용과 사무실 표시판(진행 단계 · 잔량).
    meter: budget::Meter,
    /// (57차) 실행기 포트. 논의마다 그때 고른 담당 모델로 팀을 새로 짠다([`Crew::team_now`]).
    ports: pi_agent::Ports,
}

impl Crew {
    /// (10c 13번) 논의 중인 수(뜻 검색 다시 만들기가 논의 중에는 쉰다).
    pub(crate) fn discussing(&self) -> Arc<std::sync::atomic::AtomicUsize> {
        Arc::clone(&self.discussing)
    }

    /// (10c 13번) 뜻 검색 다시 만들기의 걸음 간격과 한 걸음 조각 수.
    pub(crate) fn reembed_pace(&self) -> (std::time::Duration, usize) {
        (self.wiring.reembed_interval, self.wiring.reembed_batch)
    }

    /// 시연 모드의 가짜 AI. 시연 모드가 아니면 `None`.
    pub fn demo_ai(&self) -> Option<Arc<DemoAi>> {
        self.demo.clone()
    }

    /// 팀장의 하루 운용 판단(V5): 팀장 턴 하나로 오늘 매매할지 쉴지 정하고 [`crate::Trading::decide_day`]에 넘긴다.
    /// [`KIND_DAY_DISCUSSION`]과 방 [`DAY_ROOM`]의 글을 남긴다. 그날 이미 정했으면 다시 부르지 않고 그 판단을 돌려준다.
    ///
    /// 팀장의 답은 `{"plan": "trade"}` 또는 `{"plan": "rest", "reason": 글}`이다. 모양이 틀리거나 팀장이 답하지 못하면
    /// 쉬기(이유는 답하지 못했다는 말)로 본다(보수적으로, E6). 냉각중인 개선안 가운데 다음 거래일 시작이 지난 것은
    /// 이때 반영한다(E28). 그날의 하루 시작([`crate::KIND_DAY_START`])을 먼저 남긴 뒤에 묻는다. 팀이 멈춰 있어 매매를 시작하지 못하면 판단만 남기고 상태는 그대로다.
    pub async fn plan_day(&self) -> Result<DayPlan, TeamError> {
        let _turn = self.day_turn.lock().await;
        // 하루 시작(하루 한도 멈춤 풀기 · 그날 시작 평가액)을 먼저 남긴다: 판단은 그 뒤다.
        self.trading.tick().await;
        self.apply_cooled()?;
        self.decide_today().await
    }

    /// 지켜볼 종목마다 새 공시를 본다(V2): 고유번호가 있는 종목의 공시를 받아, 아직 논의하지 않은 공시가 있으면
    /// [`Crew::discuss`]로 논의한다. 공시가 없는 종목은 논의하지 않는다. 같은 공시(접수번호)는 두 번 논의하지 않는다.
    pub async fn scan(&self) -> Result<Vec<Discussed>, TeamError> {
        let _turn = self.scan_turn.lock().await;
        self.scan_watched().await
    }

    /// 종목 하나를 논의한다(T9 · T10): 논의중 매매를 만들고, 자료(공시 · 재무 · 금리 · 환율)를 받아 저장하고,
    /// 뉴스 공시 시세 담당을 지명해 팀 논의 → 점수 → 합의 점수 → 팀장 판단 → (사기면) 주문 안 → 주문 길까지 간다.
    ///
    /// - 계기 글에는 `topic`과 그 종목의 마지막 시세(`현재가 <값>`)가 들어간다(V32).
    /// - 판단되면 받은 자료를 그 매매에 붙인다(판단 시각보다 늦게 안 자료는 붙지 않는다, V36).
    /// - 하나라도 비거나 받지 못한 자료가 있으면 지켜보기(E32)이고 주문 안이 없다.
    pub async fn discuss(&self, symbol: &str, topic: &str) -> Result<Discussed, TeamError> {
        self.discuss_symbol(symbol, topic).await
    }

    /// 하루 복기(V19 · V20): 아직 복기하지 못한 팔린 매매마다 복기 담당 턴으로 실수와 복기 글을 받아
    /// `pi_journal::Journal::review_trade`로 남긴다(복기됨). 답에 규칙 바꾸기가 있으면 `Journal::propose`로
    /// 개선안을 내고(같은 실수 2건 이상일 때만 생긴다) 승인 요청을 만들어 [`KIND_PROPOSAL`]을 남긴다.
    ///
    /// 복기 담당의 답은 `{"mistakes": [실수 글자], "note": 글, "change": {칸: 십진수 글자} | 없음}`이다.
    /// 실수 글자는 `chased_entry` · `late_stop` · `oversized_position` · `no_rationale` · `panic_exit`이고
    /// 그 밖의 글은 자유 글 실수다. 칸 이름은 [`crate::TEMPLATE_ADJUST_PATH`]와 같다.
    /// 답하지 못한 매매는 그대로 두고 다음 마감에 다시 본다(E17).
    pub async fn review_day(&self) -> Result<Reviewed, TeamError> {
        let _turn = self.review_turn.lock().await;
        // (23번) 정산할 결정 메모가 있으면 복기 앞에 정산한다(R7-4 · N20 · E54).
        self.settle_day_memos().await;
        self.review_pending().await
    }

    /// (W5 3차) 지식 검색 도구([`crate::KNOWLEDGE_TOOL`], T16). 담당이 도구로 부르는 것과 같은 길이다.
    /// 시각은 [`crate::Wiring::clock`]이다. 입력:
    ///
    /// - `{"mode": "keyword", "query": 글, "max"?: 수}`: 키워드 검색(명시 검색, 두 글자 낱말도 찾는다).
    /// - `{"mode": "condition", "symbol"?: 종목, "max_pnl"?: 십진수 글자, "min_pnl"?: 십진수 글자, "max"?: 수,
    ///   "from"?: "YYYY-MM-DD", "to"?: "YYYY-MM-DD", "mistake"?: 실수 글자}`: 팔린 매매(손익 = 판 금액 − 산 금액)에서
    ///   조건으로 찾는다. 기록 키는 매매 번호 글자다. (10c 12번) 날짜는 닫힌 시각의 그 시장 달력 날짜(국내는 한국
    ///   시각)이고 양끝을 넣는다. 실수는 복기 기록의 실수 글자(`late_stop` 등)가 똑같은 매매만이다.
    /// - `{"mode": "semantic", "query": 글}`: 뜻 검색. 임베딩이 없어 결과는 비고 쓸 수 없음(`no_embedder`)이다.
    ///
    /// 모드가 없거나 모르는 모드 · 빈 질의 · 읽을 수 없는 값이면 [`TeamError::Knowledge`]. 바깥 요청은 없다(V23).
    pub async fn search_knowledge(
        &self,
        input: &serde_json::Value,
    ) -> Result<pi_knowledge::SearchOutput, TeamError> {
        self.search_tool(input).await
    }
}
