//! 규칙 개선안(M4 · V20 · V21 · E28 · E35).

use pi_store::Proposal;
use pi_template::{Change, RiskParams, adjust};
use serde::{Deserialize, Serialize};

use crate::{Error, Mistake};

/// 개선안 내용: 실수 종류와 바꾸기 전 · 후 값(RiskParams 차이). 저장소 개선안의 `content`에 그대로 들어간다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Improvement {
    /// 어떤 실수 때문인가.
    pub mistake: Mistake,
    /// 바꾸기 전 값.
    pub before: RiskParams,
    /// 바꾼 뒤 값.
    pub after: RiskParams,
}

impl Improvement {
    /// 저장소 개선안의 `content`를 읽는다. 모양이 다르면 [`Error::BadProposal`].
    pub fn from_proposal(proposal: &Proposal) -> Result<Improvement, Error> {
        serde_json::from_value(proposal.content.clone())
            .map_err(|err| Error::BadProposal(err.to_string()))
    }
}

/// 개선안이 위험을 키우는가(M4): 템플릿 조정과 같은 판정이다 — [`pi_template::increases_risk`]를 그대로 부른다
/// (5차). 무엇을 넓힘으로 보는지는 그 함수의 문서에 있다. 좁히거나 같거나 위험과 무관한 칸만 바뀌면 거짓이다.
pub fn increases_risk(before: &RiskParams, after: &RiskParams) -> bool {
    pi_template::increases_risk(before, after)
}

/// 반영할 값이 절대 상한 · 경계 안인지 다시 본다(M4 · E28). 상한이 있는 여섯 칸을 그대로 되넘겨 확인한다.
pub(crate) fn check_caps(params: &RiskParams) -> Result<(), Error> {
    let same = [
        Change::RiskPerTradePct(params.risk_per_trade_pct),
        Change::DailyLossLimitPct(params.daily_loss_limit_pct),
        Change::MaxDrawdownPct(params.max_drawdown_pct),
        Change::SymbolWeightPct(params.symbol_weight_pct),
        Change::ConvictionWeightPct(params.conviction_weight_pct),
        Change::SectorWeightPct(params.sector_weight_pct),
    ];
    adjust(params, &same)?;
    Ok(())
}

/// 승인 · 거절 · 냉각 해제의 결과: 개선안의 지금 모습과 그 뒤 적용되는 규칙.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleOutcome {
    /// 개선안(저장소에서 읽은 지금 모습).
    pub proposal: Proposal,
    /// 이 뒤 적용되는 규칙. 반영됨이면 개선안의 `after`, 아니면(냉각중 · 거절됨) 받은 `current` 그대로다.
    pub rules: RiskParams,
}
