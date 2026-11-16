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

mod demo_ai;

use std::collections::HashMap;
use std::sync::Arc;

use pi_agent::DiscussionOutcome;
use pi_data::{CorpCode, DataConfig, EcosCycle, EcosSeries};
use pi_journal::ReviewRecord;
use pi_market::Symbol;
use pi_store::{ProposalId, TradeId};
use pi_trigger::ScheduleTimes;

use crate::{DayPlan, Placed, TeamError};

pub use demo_ai::{DemoAi, DemoTurn};

/// 투자 팀 담당 명단(V1). `GET` → `{"members": [{"id": 담당 id, "name": 담당 이름}, …]}`.
/// 명단 순서(`pi_agent::Role::ALL`)로 6명이다. 세션이 있어야 열린다.
pub const TEAM_MEMBERS_PATH: &str = "/api/team/members";

/// 매매 근거 읽기(C6 · V18). `GET /api/trades/{id}/basis` → `{"id": 매매 번호, "summary": 쉬운 말 요약}`.
/// 요약은 `pi_journal::Journal::read_basis`의 것 그대로다. 없는 매매는 404 `{"error": "not_found"}`.
/// 세션이 있어야 열린다.
pub const TRADE_BASIS_PATH: &str = "/api/trades/{id}/basis";

/// 규칙 개선안 목록. `GET` → `{"proposals": [{"id", "state": "proposed" | "cooling" | "applied" | "rejected",
/// "mistake": 실수(글자), "increases_risk": bool, "evidence": [매매 번호], "number": 승인 번호 | null,
/// "fingerprint": 지문 | null}]}`(번호 순서). 세션이 있어야 열린다.
pub const PROPOSALS_PATH: &str = "/api/proposals";

/// 규칙 개선안 답하기(V21 · E35). `POST` `{"id": 개선안 번호, "number": 승인 번호, "fingerprint": 지문,
/// "answer": "approve" | 그 밖}`. 세션과 같은 출처가 있어야 한다. 응답은 언제나 200
/// `{"result": "applied" | "cooling" | "rejected", "reason": 글자 | null}`.
///
/// - 승인이고 승인 번호 · 지문이 그 개선안의 승인 요청과 맞으면 승인 모듈이 승인을 소비한 뒤에만 반영한다.
///   위험을 키우지 않으면 `"applied"`: 지금 쓰는 템플릿의 새 판이 개선안의 값이 된다(다음 매매부터 적용).
///   위험을 키우면 `"cooling"`: 템플릿은 그대로이고 다음 거래일 시작에 반영한다(E28).
/// - 승인 번호가 없거나 맞지 않으면 `"rejected"`, 이유 [`crate::ANSWER_NOT_FOUND`] 등(E35). 개선안 · 템플릿은 그대로다.
/// - 거절이면 `"rejected"`, 이유 `"user_rejected"`. 개선안은 거절됨이고 템플릿은 그대로다.
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
}

impl TeamWiring {
    /// 공식 주소 · 빈 고유번호 표 · 한국은행 기준금리(722Y001/0101000, 월) · 원/달러 환율(731Y001/0000001, 일) ·
    /// 공시 7일 · [`DEFAULT_CONSENSUS_REQUIRED`] · 기본 일정 간격.
    pub fn official() -> TeamWiring {
        TeamWiring {
            data: DataConfig::official(),
            corp_codes: HashMap::new(),
            korea_rate: series("722Y001", "0101000", EcosCycle::Month),
            korea_fx: series("731Y001", "0000001", EcosCycle::Day),
            disclosure_days: 7,
            consensus_required: DEFAULT_CONSENSUS_REQUIRED,
            schedule: ScheduleTimes::default(),
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
    _private: (),
}

impl Crew {
    /// 시연 모드의 가짜 AI. 시연 모드가 아니면 `None`.
    pub fn demo_ai(&self) -> Option<Arc<DemoAi>> {
        todo!()
    }

    /// 팀장의 하루 운용 판단(V5): 팀장 턴 하나로 오늘 매매할지 쉴지 정하고 [`crate::Trading::decide_day`]에 넘긴다.
    /// [`KIND_DAY_DISCUSSION`]과 방 [`DAY_ROOM`]의 글을 남긴다. 그날 이미 정했으면 다시 부르지 않고 그 판단을 돌려준다.
    ///
    /// 팀장의 답은 `{"plan": "trade"}` 또는 `{"plan": "rest", "reason": 글}`이다. 모양이 틀리거나 팀장이 답하지 못하면
    /// 쉬기(이유는 답하지 못했다는 말)로 본다(보수적으로, E6). 냉각중인 개선안 가운데 다음 거래일 시작이 지난 것은
    /// 이때 반영한다(E28).
    pub async fn plan_day(&self) -> Result<DayPlan, TeamError> {
        todo!()
    }

    /// 지켜볼 종목마다 새 공시를 본다(V2): 고유번호가 있는 종목의 공시를 받아, 아직 논의하지 않은 공시가 있으면
    /// [`Crew::discuss`]로 논의한다. 공시가 없는 종목은 논의하지 않는다. 같은 공시(접수번호)는 두 번 논의하지 않는다.
    pub async fn scan(&self) -> Result<Vec<Discussed>, TeamError> {
        todo!()
    }

    /// 종목 하나를 논의한다(T9 · T10): 논의중 매매를 만들고, 자료(공시 · 재무 · 금리 · 환율)를 받아 저장하고,
    /// 뉴스 공시 시세 담당을 지명해 팀 논의 → 점수 → 합의 점수 → 팀장 판단 → (사기면) 주문 안 → 주문 길까지 간다.
    ///
    /// - 계기 글에는 `topic`과 그 종목의 마지막 시세(`현재가 <값>`)가 들어간다(V32).
    /// - 판단되면 받은 자료를 그 매매에 붙인다(판단 시각보다 늦게 안 자료는 붙지 않는다, V36).
    /// - 하나라도 비거나 받지 못한 자료가 있으면 지켜보기(E32)이고 주문 안이 없다.
    pub async fn discuss(&self, symbol: &str, topic: &str) -> Result<Discussed, TeamError> {
        let _ = (symbol, topic);
        todo!()
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
        todo!()
    }
}
