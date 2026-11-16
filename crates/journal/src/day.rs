//! 하루 정리 내용(V12 · V50).

use pi_store::{Amount, TradeId, TradeState, UnixMillis};

use crate::ReviewRecord;

/// 하루의 범위. 시작은 포함, 끝은 제외다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Day {
    /// 시작(포함).
    pub start: UnixMillis,
    /// 끝(제외). 시작보다 늦어야 한다.
    pub end: UnixMillis,
}

/// 하루 정리에 호출하는 쪽이 넘기는 값(저장소가 모르는 것).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayFacts {
    /// 실현 손익(매매별). 합이 그날 손익이다.
    pub realized: Vec<(TradeId, Amount)>,
    /// 마지막 백업 완료 시각. 아직 없으면 `None`.
    pub last_backup_at: Option<UnixMillis>,
}

/// 그날 한 일 한 줄.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeLine {
    /// 매매 번호.
    pub id: TradeId,
    /// 종목 코드.
    pub symbol: String,
    /// 지금 상태.
    pub state: TradeState,
}

/// 하루 정리(V12 · V50): 한 일 · 손익 · 복기 · 백업 완료 시각이 한 값에 있다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaySummary {
    /// 그날 손댄 매매(만든 · 판단한 · 마지막으로 바뀐 시각 가운데 하나가 하루 범위 안이거나,
    /// 그날 상태가 바뀐 적이 있는 것). 번호 순서다.
    pub done: Vec<TradeLine>,
    /// 손익 합.
    pub pnl: Amount,
    /// 그날 한 복기(복기 시각이 하루 범위 안). 기록한 순서다.
    pub reviewed: Vec<ReviewRecord>,
    /// 아직 복기하지 못한 팔린 매매(E17: 다음 마감으로 넘어간다). 번호 순서다.
    pub unreviewed: Vec<TradeId>,
    /// 마지막 백업 완료 시각.
    pub last_backup_at: Option<UnixMillis>,
}
