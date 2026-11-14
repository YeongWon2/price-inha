//! 매매(M2): 만들기 · 읽기 · 판단 · 전이 · 승인 · 손절값.
//!
//! 금액은 SQLite INTEGER(64비트 정수)로만 오간다. 부동소수점을 거치지 않는다.

use rusqlite::{Connection, Row, params};

use crate::codec;
use crate::error::{DenyReason, StoreError, Transition};
use crate::model::{
    Amount, Approval, Judgment, NewTrade, Rationale, RationaleField, RoleScore, SourceRef, Trade,
    TradeId, TradeState, UnixMillis,
};

const COLUMNS: &str = "id, symbol, account, state, created_at, updated_at, hypothesis, \
    trigger_note, entry, stop, target, expected_value, decided_at, consensus_score, \
    approval_number, approval_fingerprint";

/// 전이 호출로 갈 수 있는 길([`TradeState`]의 문서 주석과 같다).
/// 논의중 → 판단됨은 여기에 없다. 그 길은 [`judge`]만 쓴다.
fn allowed(from: TradeState, to: TradeState) -> bool {
    use TradeState::{AwaitingApproval, Blocked, Holding, Judged, Ordered, Reviewed, Sold};
    matches!(
        (from, to),
        (Judged, AwaitingApproval | Blocked | Ordered)
            | (AwaitingApproval, Ordered)
            | (Ordered, Holding)
            | (Holding, Sold)
            | (Sold, Reviewed)
    )
}

fn denied(from: TradeState, to: TradeState, reason: DenyReason) -> StoreError {
    StoreError::TransitionDenied {
        transition: Transition::Trade { from, to },
        reason,
    }
}

fn blank(text: Option<&str>) -> bool {
    text.is_none_or(|t| t.trim().is_empty())
}

/// 근거의 빈 칸들. [`RationaleField`]를 적은 순서다.
fn missing_fields(r: &Rationale) -> Vec<RationaleField> {
    [
        (RationaleField::Hypothesis, blank(r.hypothesis.as_deref())),
        (RationaleField::Trigger, blank(r.trigger.as_deref())),
        (RationaleField::Entry, r.entry.is_none()),
        (RationaleField::Stop, r.stop.is_none()),
        (RationaleField::Target, r.target.is_none()),
        (RationaleField::ExpectedValue, r.expected_value.is_none()),
    ]
    .into_iter()
    .filter_map(|(field, empty)| empty.then_some(field))
    .collect()
}

/// 매매를 논의중으로 만든다.
pub(crate) fn create(conn: &Connection, new: NewTrade) -> Result<Trade, StoreError> {
    conn.execute(
        "INSERT INTO trades (symbol, account, state, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![
            new.symbol,
            codec::account_code(new.account),
            codec::trade_state_code(TradeState::Discussing),
            new.at.0,
        ],
    )?;
    get(conn, TradeId(conn.last_insert_rowid()))
}

/// 매매 한 건을 자료 · 점수와 함께 읽는다.
pub(crate) fn get(conn: &Connection, id: TradeId) -> Result<Trade, StoreError> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM trades WHERE id = ?1"))?;
    let mut rows = stmt.query([id.0])?;
    let Some(row) = rows.next()? else {
        return Err(StoreError::TradeNotFound(id));
    };
    let mut trade = trade_from(row)?;
    trade.sources = sources(conn, id)?;
    trade.scores = scores(conn, id)?;
    Ok(trade)
}

/// 그 상태의 매매들, 번호 순서.
pub(crate) fn in_state(conn: &Connection, state: TradeState) -> Result<Vec<Trade>, StoreError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM trades WHERE state = ?1 ORDER BY id"
    ))?;
    let mut rows = stmt.query([codec::trade_state_code(state)])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut trade = trade_from(row)?;
        trade.sources = sources(conn, trade.id)?;
        trade.scores = scores(conn, trade.id)?;
        out.push(trade);
    }
    Ok(out)
}

/// 논의중 → 판단됨. 근거 · 자료 · 점수를 함께 쓴다.
pub(crate) fn judge(conn: &Connection, id: TradeId, j: Judgment) -> Result<Trade, StoreError> {
    let trade = get(conn, id)?;
    if trade.state != TradeState::Discussing {
        return Err(denied(
            trade.state,
            TradeState::Judged,
            DenyReason::NotAllowed,
        ));
    }
    if let Some(late) = j.sources.iter().find(|s| s.known_at > j.decided_at) {
        return Err(StoreError::SourceAfterDecision {
            source_name: late.name.clone(),
            known_at: late.known_at,
            decided_at: j.decided_at,
        });
    }
    let r = &j.rationale;
    conn.execute(
        "UPDATE trades SET state = ?1, updated_at = ?2, decided_at = ?2, hypothesis = ?3,
             trigger_note = ?4, entry = ?5, stop = ?6, target = ?7, expected_value = ?8,
             consensus_score = ?9
         WHERE id = ?10",
        params![
            codec::trade_state_code(TradeState::Judged),
            j.decided_at.0,
            r.hypothesis,
            r.trigger,
            r.entry.map(|a| a.0),
            r.stop.map(|a| a.0),
            r.target.map(|a| a.0),
            r.expected_value.map(|a| a.0),
            j.consensus_score,
            id.0,
        ],
    )?;
    for (pos, source) in (0_i64..).zip(&j.sources) {
        conn.execute(
            "INSERT INTO trade_sources (trade_id, pos, name, known_at) VALUES (?1, ?2, ?3, ?4)",
            params![id.0, pos, source.name, source.known_at.0],
        )?;
    }
    for (pos, score) in (0_i64..).zip(&j.scores) {
        conn.execute(
            "INSERT INTO trade_scores (trade_id, pos, role, score, reason)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id.0, pos, score.role, score.score, score.reason],
        )?;
    }
    get(conn, id)
}

/// 매매 상태를 바꾼다. 판단됨으로는 가지 못한다.
pub(crate) fn transition(
    conn: &Connection,
    id: TradeId,
    to: TradeState,
    at: UnixMillis,
) -> Result<Trade, StoreError> {
    let trade = get(conn, id)?;
    if !allowed(trade.state, to) {
        return Err(denied(trade.state, to, DenyReason::NotAllowed));
    }
    if to == TradeState::Ordered {
        let missing = missing_fields(&trade.rationale);
        if !missing.is_empty() {
            return Err(denied(
                trade.state,
                to,
                DenyReason::RationaleIncomplete { missing },
            ));
        }
    }
    conn.execute(
        "UPDATE trades SET state = ?1, updated_at = ?2 WHERE id = ?3",
        params![codec::trade_state_code(to), at.0, id.0],
    )?;
    get(conn, id)
}

/// 승인 번호와 내용 지문을 붙인다(있으면 바꿔 넣는다).
pub(crate) fn set_approval(
    conn: &Connection,
    id: TradeId,
    approval: Approval,
    at: UnixMillis,
) -> Result<Trade, StoreError> {
    let changed = conn.execute(
        "UPDATE trades SET approval_number = ?1, approval_fingerprint = ?2, updated_at = ?3
         WHERE id = ?4",
        params![approval.number, approval.content_fingerprint, at.0, id.0],
    )?;
    if changed == 0 {
        return Err(StoreError::TradeNotFound(id));
    }
    get(conn, id)
}

/// 보유중인 매매의 손절값을 더 높게 바꾼다.
pub(crate) fn update_stop(
    conn: &Connection,
    id: TradeId,
    new_stop: Amount,
    at: UnixMillis,
) -> Result<Trade, StoreError> {
    let trade = get(conn, id)?;
    if trade.state != TradeState::Holding {
        return Err(StoreError::StopNotAdjustable { state: trade.state });
    }
    // 보유중이면 손절값이 있다(근거가 비면 주문됨을 지나지 못한다).
    // 그래도 없는 파일을 만나면, 견줄 값이 없으니 새 값을 받아들인다.
    if let Some(current) = trade.rationale.stop
        && new_stop <= current
    {
        return Err(StoreError::StopNotTightened {
            current,
            requested: new_stop,
        });
    }
    conn.execute(
        "UPDATE trades SET stop = ?1, updated_at = ?2 WHERE id = ?3",
        params![new_stop.0, at.0, id.0],
    )?;
    get(conn, id)
}

fn trade_from(row: &Row<'_>) -> Result<Trade, StoreError> {
    let approval = match (
        row.get::<_, Option<i64>>(14)?,
        row.get::<_, Option<String>>(15)?,
    ) {
        (Some(number), Some(content_fingerprint)) => Some(Approval {
            number,
            content_fingerprint,
        }),
        _ => None,
    };
    Ok(Trade {
        id: TradeId(row.get(0)?),
        symbol: row.get(1)?,
        account: codec::account_from(&row.get::<_, String>(2)?)?,
        state: codec::trade_state_from(&row.get::<_, String>(3)?)?,
        created_at: UnixMillis(row.get(4)?),
        updated_at: UnixMillis(row.get(5)?),
        rationale: Rationale {
            hypothesis: row.get(6)?,
            trigger: row.get(7)?,
            entry: row.get::<_, Option<i64>>(8)?.map(Amount),
            stop: row.get::<_, Option<i64>>(9)?.map(Amount),
            target: row.get::<_, Option<i64>>(10)?.map(Amount),
            expected_value: row.get::<_, Option<i64>>(11)?.map(Amount),
        },
        decided_at: row.get::<_, Option<i64>>(12)?.map(UnixMillis),
        sources: Vec::new(),
        scores: Vec::new(),
        consensus_score: row.get(13)?,
        approval,
        blocked: None,
    })
}

fn sources(conn: &Connection, id: TradeId) -> Result<Vec<SourceRef>, StoreError> {
    let mut stmt =
        conn.prepare("SELECT name, known_at FROM trade_sources WHERE trade_id = ?1 ORDER BY pos")?;
    let rows = stmt.query_map([id.0], |row| {
        Ok(SourceRef {
            name: row.get(0)?,
            known_at: UnixMillis(row.get(1)?),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn scores(conn: &Connection, id: TradeId) -> Result<Vec<RoleScore>, StoreError> {
    let mut stmt = conn
        .prepare("SELECT role, score, reason FROM trade_scores WHERE trade_id = ?1 ORDER BY pos")?;
    let rows = stmt.query_map([id.0], |row| {
        Ok(RoleScore {
            role: row.get(0)?,
            score: row.get(1)?,
            reason: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}
