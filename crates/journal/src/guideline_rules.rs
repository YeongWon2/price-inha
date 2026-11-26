//! (백로그 T · 결정 30) 1층으로 가는 지침 후보를 지금의 규칙 개선안 길로.

use pi_store::{DecisionMemo, Proposal, TradeId, UnixMillis};
use pi_template::{Change, RiskParams};

use crate::guideline_learn::Candidate;
use crate::{Error, Journal};

/// 값 없이 늘림 · 줄임만 적힌 글에서 움직이는 몫(지금 값의 백분율).
pub const GUIDELINE_RULE_STEP_PCT: u32 = 10;

/// 1층 후보 글에서 RiskParams 칸 하나의 바뀜을 읽는다.
pub fn rule_change_of(_text: &str, _current: &RiskParams) -> Option<Change> {
    None
}

/// 후보의 근거 매매.
pub fn candidate_trades(_candidate: &Candidate, _memos: &[DecisionMemo]) -> Vec<TradeId> {
    Vec::new()
}

impl Journal {
    /// 1층 후보에서 개선안을 낸다.
    pub fn propose_from_candidate(
        &self,
        _candidate: &Candidate,
        _memos: &[DecisionMemo],
        _current: &RiskParams,
        _at: UnixMillis,
    ) -> Result<Option<Proposal>, Error> {
        Ok(None)
    }
}
