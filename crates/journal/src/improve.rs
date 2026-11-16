//! 규칙 개선안(M4 · V20 · V21 · E28 · E35).

use pi_store::Proposal;
use pi_template::{Change, Cooldown, LossStreakStop, PctRange, RiskParams, adjust};
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

/// 개선안이 위험을 키우는가(M4): 한도나 손절을 넓히는 변경이면 참.
///
/// 키우는 것으로 본다: 1회 위험 · 일일 손실 · 최대 낙폭 · 종목 · 확신 종목 · 업종 비중 · 자동 주문 한도의 상향,
/// 확신 비중이 비었다가(종목 비중이 한도) 종목 비중보다 넓은 값이 생기거나 값이 없어지는 것,
/// 고정 손절이 사라지거나 그 범위의 아래 · 위 끝이 넓어지는 것, 연속 손실 정지가 해제되거나
/// 정지까지의 손실 횟수가 늘거나 쉬는 기간이 줄어드는 것, ATR 손절이 사라지거나 배수가 커지는 것,
/// 최소 현금의 감소, 최대 동시 보유 수 · 일일 최대 매매 횟수의 상향.
/// 좁히거나 같거나 다른 칸만 바뀌면 거짓이다.
pub fn increases_risk(before: &RiskParams, after: &RiskParams) -> bool {
    let conviction = |p: &RiskParams| p.conviction_weight_pct.unwrap_or(p.symbol_weight_pct);
    let atr = |p: &RiskParams| p.stop.atr.map(|atr| atr.multiple);
    after.risk_per_trade_pct > before.risk_per_trade_pct
        || after.daily_loss_limit_pct > before.daily_loss_limit_pct
        || after.max_drawdown_pct > before.max_drawdown_pct
        || after.symbol_weight_pct > before.symbol_weight_pct
        || conviction(after) > conviction(before)
        || after.sector_weight_pct > before.sector_weight_pct
        || after.auto_order_limit_pct > before.auto_order_limit_pct
        || stop_widened(before.stop.fixed_loss_pct, after.stop.fixed_loss_pct)
        || streak_loosened(before.loss_streak, after.loss_streak)
        || matches!((atr(before), atr(after)), (Some(_), None))
        || matches!((atr(before), atr(after)), (Some(b), Some(a)) if a > b)
        || after.min_cash_pct < before.min_cash_pct
        || after.max_positions.high > before.max_positions.high
        || after.max_trades_per_day > before.max_trades_per_day
}

/// 연속 손실 정지가 느슨해졌는가: 해제됐거나, 정지까지의 손실 횟수가 늘었거나, 쉬는 기간이 줄었다.
fn streak_loosened(before: Option<LossStreakStop>, after: Option<LossStreakStop>) -> bool {
    match (before, after) {
        (Some(_), None) => true,
        (Some(b), Some(a)) => a.losses > b.losses || rest_days(a.cooldown) < rest_days(b.cooldown),
        _ => false,
    }
}

/// 쉬는 기간의 길이 순서: 그날만(0) < 거래일 n일(n).
fn rest_days(cooldown: Cooldown) -> u32 {
    match cooldown {
        Cooldown::RestOfDay => 0,
        Cooldown::TradingDays(days) => days,
    }
}

/// 고정 손절이 넓어졌는가: 사라졌거나, 아래 끝이 내려갔거나, 위 끝이 올라갔다.
fn stop_widened(before: Option<PctRange>, after: Option<PctRange>) -> bool {
    match (before, after) {
        (Some(_), None) => true,
        (Some(b), Some(a)) => a.low < b.low || a.high > b.high,
        _ => false,
    }
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
