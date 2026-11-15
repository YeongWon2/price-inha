//! 실전 승격 기준(`principles.md` P14, `dev.md` M3 · E26).
//!
//! 기본값(추정, `docs/dev/decisions.md` 2026-11-16 30번): 매매 30건 · 거래일 20일 ·
//! 비용 뺀 손익 > 0 · 최대 낙폭 ≤ 템플릿 한도의 절반. 표본 밖 성과는 P14 의 기준이며
//! 기본은 "비용 뺀 표본 밖 손익 > 0"(보수적으로 고름, 미확정).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 승격 기준 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criterion {
    /// 표본 수(매매 건수).
    Trades,
    /// 기간(거래일 수).
    TradingDays,
    /// 비용 뺀 손익.
    NetPnl,
    /// 최대 낙폭.
    MaxDrawdown,
    /// 표본 밖 성과.
    OutOfSample,
}

/// 템플릿마다 정한 승격 기준.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionCriteria {
    /// 최소 매매 건수(이상이면 통과).
    pub min_trades: u32,
    /// 최소 거래일 수(이상이면 통과).
    pub min_trading_days: u32,
    /// 비용 뺀 손익이 이 값보다 **커야** 통과(같으면 미달).
    pub net_pnl_above: Decimal,
    /// 최대 낙폭(퍼센트)이 이 값 **이하**면 통과.
    pub max_drawdown_pct: Decimal,
    /// 비용 뺀 표본 밖 손익이 이 값보다 커야 통과. `None` 이면 보지 않는다.
    pub out_of_sample_pnl_above: Option<Decimal>,
}

/// 모의 결과 요약. 계산은 이 모듈 밖에서 하고 값만 받는다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromotionStats {
    /// 매매 건수.
    pub trades: u32,
    /// 거래일 수.
    pub trading_days: u32,
    /// 비용(수수료 · 세금 · 미끄러짐) 뺀 손익.
    pub net_pnl: Decimal,
    /// 최대 낙폭(퍼센트, 양수).
    pub max_drawdown_pct: Decimal,
    /// 비용 뺀 표본 밖 손익. 아직 없으면 `None`(미달로 본다).
    pub out_of_sample_pnl: Option<Decimal>,
}

/// 판정 결과(E26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 모든 기준 통과.
    Passed,
    /// 모자란 기준들. [`Criterion`] 순서대로, 비어 있지 않다.
    Short(Vec<Criterion>),
}

impl PromotionCriteria {
    /// 모의 결과를 이 기준과 비교한다.
    pub fn evaluate(&self, stats: &PromotionStats) -> Verdict {
        let _ = stats;
        todo!()
    }
}
