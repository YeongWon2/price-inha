//! 값과 파일에 적는 것 사이의 변환. 파일에는 이 글자가 남으므로 한 번 정한 글자는 바꾸지 않는다.

use crate::error::StoreError;
use crate::model::{AccountKind, BlockKind, TeamCause, TeamState, TradeState};

fn unknown(what: &str, code: &str) -> StoreError {
    StoreError::Incompatible(format!("모르는 {what} 값: {code}"))
}

/// 기록 번호 칸을 읽는다. 번호는 1부터 커지므로 음수면 이 저장소가 쓴 값이 아니다.
pub(crate) fn seq(row: &rusqlite::Row<'_>, idx: usize) -> Result<u64, StoreError> {
    let raw: i64 = row.get(idx)?;
    u64::try_from(raw).map_err(|_| StoreError::Incompatible(format!("기록 번호가 이상하다: {raw}")))
}

/// `... RETURNING seq`로 끝나는 INSERT를 돌리고 붙은 번호를 준다.
pub(crate) fn insert_returning_seq(
    conn: &rusqlite::Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<u64, StoreError> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params)?;
    match rows.next()? {
        Some(row) => seq(row, 0),
        None => Err(StoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows)),
    }
}

pub(crate) fn team_state_code(state: TeamState) -> &'static str {
    match state {
        TeamState::Operating => "operating",
        TeamState::Resting => "resting",
        TeamState::DailyLimitHalt => "daily_limit_halt",
        TeamState::AnomalyHalt => "anomaly_halt",
        TeamState::FullStop => "full_stop",
    }
}

pub(crate) fn team_state_from(code: &str) -> Result<TeamState, StoreError> {
    TeamState::ALL
        .into_iter()
        .find(|state| team_state_code(*state) == code)
        .ok_or_else(|| unknown("투자 팀 상태", code))
}

pub(crate) fn team_cause_code(cause: TeamCause) -> &'static str {
    match cause {
        TeamCause::TradingStart => "trading_start",
        TeamCause::MarketClose => "market_close",
        TeamCause::NextTradingDayStart => "next_trading_day_start",
        TeamCause::DailyLossLimit => "daily_loss_limit",
        TeamCause::MaxLoss => "max_loss",
        TeamCause::AnomalyDetected => "anomaly_detected",
        TeamCause::AnomalyCleared => "anomaly_cleared",
        TeamCause::OwnerRestart => "owner_restart",
        TeamCause::Restored => "restored",
        TeamCause::OwnerStop => "owner_stop",
    }
}

pub(crate) fn team_cause_from(code: &str) -> Result<TeamCause, StoreError> {
    TeamCause::ALL
        .into_iter()
        .find(|cause| team_cause_code(*cause) == code)
        .ok_or_else(|| unknown("투자 팀 사유", code))
}

pub(crate) fn trade_state_code(state: TradeState) -> &'static str {
    match state {
        TradeState::Discussing => "discussing",
        TradeState::Judged => "judged",
        TradeState::AwaitingApproval => "awaiting_approval",
        TradeState::Blocked => "blocked",
        TradeState::Ordered => "ordered",
        TradeState::Holding => "holding",
        TradeState::Sold => "sold",
        TradeState::Reviewed => "reviewed",
    }
}

pub(crate) fn trade_state_from(code: &str) -> Result<TradeState, StoreError> {
    TradeState::ALL
        .into_iter()
        .find(|state| trade_state_code(*state) == code)
        .ok_or_else(|| unknown("매매 상태", code))
}

pub(crate) fn account_code(account: AccountKind) -> &'static str {
    match account {
        AccountKind::Paper => "paper",
        AccountKind::Live => "live",
    }
}

pub(crate) fn account_from(code: &str) -> Result<AccountKind, StoreError> {
    [AccountKind::Paper, AccountKind::Live]
        .into_iter()
        .find(|account| account_code(*account) == code)
        .ok_or_else(|| unknown("계좌 종류", code))
}

pub(crate) fn block_kind_code(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::PrincipleOrRationale => "principle_or_rationale",
        BlockKind::ApprovalRejected => "approval_rejected",
        BlockKind::UnfilledCancelled => "unfilled_cancelled",
    }
}

pub(crate) fn block_kind_from(code: &str) -> Result<BlockKind, StoreError> {
    BlockKind::ALL
        .into_iter()
        .find(|kind| block_kind_code(*kind) == code)
        .ok_or_else(|| unknown("막힌 이유", code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 모든_값이_글자로_갔다가_그대로_돌아온다() {
        for state in TeamState::ALL {
            assert_eq!(team_state_from(team_state_code(state)).unwrap(), state);
        }
        for cause in TeamCause::ALL {
            assert_eq!(team_cause_from(team_cause_code(cause)).unwrap(), cause);
        }
        for state in TradeState::ALL {
            assert_eq!(trade_state_from(trade_state_code(state)).unwrap(), state);
        }
        for kind in BlockKind::ALL {
            assert_eq!(block_kind_from(block_kind_code(kind)).unwrap(), kind);
        }
        for account in [AccountKind::Paper, AccountKind::Live] {
            assert_eq!(account_from(account_code(account)).unwrap(), account);
        }
    }

    #[test]
    fn 모르는_글자는_오류다() {
        assert!(matches!(
            team_state_from("flying"),
            Err(StoreError::Incompatible(_))
        ));
        assert!(matches!(
            trade_state_from(""),
            Err(StoreError::Incompatible(_))
        ));
    }
}
