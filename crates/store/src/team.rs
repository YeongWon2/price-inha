//! 투자 팀 상태(M1). 지금 상태는 마지막 기록의 "무엇으로"다(따로 두지 않아 어긋날 수 없다).

use rusqlite::{Connection, Row, params};

use crate::codec::{self, insert_returning_seq};
use crate::error::{DenyReason, StoreError, Transition};
use crate::model::{TeamCause, TeamChange, TeamState, TeamStateRecord, UnixMillis};
use crate::template;

/// 허용 표의 한 줄에 붙은 조건.
#[derive(Clone, Copy)]
enum Condition {
    /// 조건 없음.
    Always,
    /// 사유가 "다음 거래일 시작"일 때만.
    NextTradingDayStart,
    /// "내 승인" 표시가 있을 때만.
    OwnerApproved,
    /// 사유가 "최대 손실"일 때만.
    MaxLoss,
}

/// 허용 표([`TeamState`]의 문서 주석과 같다). 표에 없으면 `None`.
fn rule(from: TeamState, to: TeamState) -> Option<Condition> {
    use TeamState::{AnomalyHalt, DailyLimitHalt, FullStop, Operating, Resting};
    match (from, to) {
        (Resting, Operating | AnomalyHalt) => Some(Condition::Always),
        (Operating, Resting | DailyLimitHalt | AnomalyHalt) => Some(Condition::Always),
        (Operating | DailyLimitHalt | AnomalyHalt, FullStop) => Some(Condition::MaxLoss),
        (DailyLimitHalt, Resting) => Some(Condition::NextTradingDayStart),
        (AnomalyHalt, Resting) => Some(Condition::Always),
        (FullStop, Resting) => Some(Condition::OwnerApproved),
        _ => None,
    }
}

/// 이 변경이 되는지 본다. 안 되면 이유를 준다.
fn check(from: TeamState, change: &TeamChange) -> Result<(), DenyReason> {
    match rule(from, change.to) {
        None => Err(DenyReason::NotAllowed),
        Some(Condition::Always) => Ok(()),
        Some(Condition::NextTradingDayStart) => {
            if change.cause == TeamCause::NextTradingDayStart {
                Ok(())
            } else {
                Err(DenyReason::NextTradingDayOnly)
            }
        }
        Some(Condition::MaxLoss) => {
            if change.cause == TeamCause::MaxLoss {
                Ok(())
            } else {
                Err(DenyReason::MaxLossOnly)
            }
        }
        Some(Condition::OwnerApproved) => {
            if change.owner_approved {
                Ok(())
            } else {
                Err(DenyReason::OwnerApprovalRequired)
            }
        }
    }
}

/// 지금 상태. 기록이 없으면 쉬는 중이다.
pub(crate) fn current(conn: &Connection) -> Result<TeamState, StoreError> {
    let mut stmt = conn.prepare("SELECT to_state FROM team_history ORDER BY seq DESC LIMIT 1")?;
    let mut rows = stmt.query([])?;
    match rows.next()? {
        Some(row) => codec::team_state_from(&row.get::<_, String>(0)?),
        None => Ok(TeamState::Resting),
    }
}

/// 상태를 바꾸고 기록 한 줄을 남긴다. 거부되면 아무것도 쓰지 않는다.
pub(crate) fn change(conn: &Connection, change: TeamChange) -> Result<TeamStateRecord, StoreError> {
    let from = current(conn)?;
    check(from, &change).map_err(|reason| StoreError::TransitionDenied {
        transition: Transition::Team {
            from,
            to: change.to,
        },
        reason,
    })?;
    let seq = insert_returning_seq(
        conn,
        "INSERT INTO team_history (seq, at, from_state, to_state, cause, detail, owner_approved)
         SELECT COALESCE(MAX(seq), 0) + 1, ?1, ?2, ?3, ?4, ?5, ?6 FROM team_history
         RETURNING seq",
        params![
            change.at.0,
            codec::team_state_code(from),
            codec::team_state_code(change.to),
            codec::team_cause_code(change.cause),
            change.detail,
            change.owner_approved,
        ],
    )?;
    // 전체 정지가 되면 실전중 템플릿은 같은 묶음 안에서 멈춤이 된다(M3).
    if change.to == TeamState::FullStop {
        template::stop_all_live(conn, change.at)?;
    }
    Ok(TeamStateRecord {
        seq,
        at: change.at,
        from,
        to: change.to,
        cause: change.cause,
        detail: change.detail,
        owner_approved: change.owner_approved,
    })
}

/// 기록 전체, 번호 순서.
pub(crate) fn history(conn: &Connection) -> Result<Vec<TeamStateRecord>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT seq, at, from_state, to_state, cause, detail, owner_approved
         FROM team_history ORDER BY seq",
    )?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(record_from(row)?);
    }
    Ok(out)
}

fn record_from(row: &Row<'_>) -> Result<TeamStateRecord, StoreError> {
    Ok(TeamStateRecord {
        seq: codec::seq(row, 0)?,
        at: UnixMillis(row.get(1)?),
        from: codec::team_state_from(&row.get::<_, String>(2)?)?,
        to: codec::team_state_from(&row.get::<_, String>(3)?)?,
        cause: codec::team_cause_from(&row.get::<_, String>(4)?)?,
        detail: row.get(5)?,
        owner_approved: row.get(6)?,
    })
}
