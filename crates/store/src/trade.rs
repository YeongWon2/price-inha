//! 매매(M2): 만들기 · 읽기 · 판단 · 전이 · 승인 · 손절값.
//!
//! 금액은 SQLite INTEGER(64비트 정수)로만 오간다. 부동소수점을 거치지 않는다.

use rusqlite::{Connection, Row, params};

use crate::codec;
use crate::error::{DenyReason, StoreError, Transition};
use crate::model::{
    Amount, Approval, BlockKind, BlockReason, Judgment, NewTrade, Rationale, RationaleField,
    RoleScore, SourceRef, Trade, TradeId, TradeState, UnixMillis,
};

const COLUMNS: &str = "id, symbol, account, state, created_at, updated_at, hypothesis, \
    trigger_note, entry, stop, target, expected_value, decided_at, consensus_score, \
    approval_number, approval_fingerprint, block_kind, block_detail";

/// 전이 호출로 갈 수 있는 길([`TradeState`]의 문서 주석과 같다).
/// 논의중 → 판단됨은 여기에 없다. 그 길은 [`judge`]만 쓴다.
/// 막힘으로 가는 길도 여기에 없다. 그 길은 [`block`]만 쓴다.
fn allowed(from: TradeState, to: TradeState) -> bool {
    use TradeState::{AwaitingApproval, Holding, Judged, Ordered, Reviewed, Sold};
    matches!(
        (from, to),
        (Judged, AwaitingApproval | Ordered)
            | (AwaitingApproval, Ordered)
            | (Ordered, Holding)
            | (Holding, Sold)
            | (Sold, Reviewed)
    )
}

/// 그 상태에서 막힘으로 갈 때 짝이 맞는 이유 종류. 막힘으로 가는 길이 없는 상태는 `None`.
fn block_kind_for(from: TradeState) -> Option<BlockKind> {
    match from {
        TradeState::Judged => Some(BlockKind::PrincipleOrRationale),
        TradeState::AwaitingApproval => Some(BlockKind::ApprovalRejected),
        TradeState::Ordered => Some(BlockKind::UnfilledCancelled),
        TradeState::Discussing
        | TradeState::Blocked
        | TradeState::Holding
        | TradeState::Sold
        | TradeState::Reviewed => None,
    }
}

/// 주문된 적이 있는 매매인가. 이런 매매에는 승인을 붙이거나 바꿀 수 없다.
/// 미체결 취소로 막힌 매매도 주문됨을 지나왔으므로 여기에 든다.
fn approval_locked(trade: &Trade) -> bool {
    match trade.state {
        TradeState::Ordered | TradeState::Holding | TradeState::Sold | TradeState::Reviewed => true,
        TradeState::Blocked => trade
            .blocked
            .as_ref()
            .is_some_and(|b| b.kind == BlockKind::UnfilledCancelled),
        TradeState::Discussing | TradeState::Judged | TradeState::AwaitingApproval => false,
    }
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
    if to == TradeState::Blocked {
        // 막힘은 이유를 받는 [`block`]으로만 간다. 길이 있는 상태에서는 그 점을 알려 준다.
        let reason = if block_kind_for(trade.state).is_some() {
            DenyReason::BlockReasonRequired
        } else {
            DenyReason::NotAllowed
        };
        return Err(denied(trade.state, to, reason));
    }
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

/// 매매를 막힘으로 보내고 이유를 남긴다. 이유 종류와 출발 상태의 짝이 맞아야 한다.
///
/// 미체결 취소에서 "하나도 체결되지 않았는가"는 여기서 알 수 없다. 호출하는 쪽이 확인한다.
pub(crate) fn block(
    conn: &Connection,
    id: TradeId,
    reason: BlockReason,
    at: UnixMillis,
) -> Result<Trade, StoreError> {
    let trade = get(conn, id)?;
    let Some(expected) = block_kind_for(trade.state) else {
        return Err(denied(
            trade.state,
            TradeState::Blocked,
            DenyReason::NotAllowed,
        ));
    };
    if reason.kind != expected {
        return Err(denied(
            trade.state,
            TradeState::Blocked,
            DenyReason::BlockReasonMismatch,
        ));
    }
    conn.execute(
        "UPDATE trades SET state = ?1, updated_at = ?2, block_kind = ?3, block_detail = ?4
         WHERE id = ?5",
        params![
            codec::trade_state_code(TradeState::Blocked),
            at.0,
            codec::block_kind_code(reason.kind),
            reason.detail,
            id.0,
        ],
    )?;
    get(conn, id)
}

/// 승인 번호와 내용 지문을 붙인다(주문 전이면 바꿔 넣을 수 있다).
///
/// 한 번 매매에 붙은 번호는 `approval_numbers`에 그 매매의 것으로 남는다. 그 매매가 주문되거나
/// 막히거나 다른 번호로 바꿔 넣은 뒤에도 다른 매매는 그 번호를 쓸 수 없다.
pub(crate) fn set_approval(
    conn: &Connection,
    id: TradeId,
    approval: Approval,
    at: UnixMillis,
) -> Result<Trade, StoreError> {
    let trade = get(conn, id)?;
    if approval_locked(&trade) {
        return Err(StoreError::ApprovalLocked { state: trade.state });
    }
    let owner = {
        let mut stmt = conn.prepare("SELECT trade_id FROM approval_numbers WHERE number = ?1")?;
        let mut rows = stmt.query([approval.number])?;
        match rows.next()? {
            Some(row) => Some(row.get::<_, i64>(0)?),
            None => None,
        }
    };
    match owner {
        Some(owner) if owner != id.0 => {
            return Err(StoreError::ApprovalNumberReused {
                number: approval.number,
            });
        }
        Some(_) => {}
        None => {
            conn.execute(
                "INSERT INTO approval_numbers (number, trade_id) VALUES (?1, ?2)",
                params![approval.number, id.0],
            )?;
        }
    }
    conn.execute(
        "UPDATE trades SET approval_number = ?1, approval_fingerprint = ?2, updated_at = ?3
         WHERE id = ?4",
        params![approval.number, approval.content_fingerprint, at.0, id.0],
    )?;
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
    // 버전 1에서 막힌 매매는 이유 칸이 비어 있다. 그때는 이유 없이 읽힌다.
    let blocked = match row.get::<_, Option<String>>(16)? {
        Some(code) => Some(BlockReason {
            kind: codec::block_kind_from(&code)?,
            detail: row.get::<_, Option<String>>(17)?.unwrap_or_default(),
        }),
        None => None,
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
        blocked,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AccountKind;

    fn store() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::schema::open(&dir.path().join("pi.sqlite")).unwrap();
        (dir, conn)
    }

    fn judged(conn: &Connection) -> TradeId {
        let id = create(
            conn,
            NewTrade {
                symbol: "005930".to_string(),
                account: AccountKind::Paper,
                at: UnixMillis(0),
            },
        )
        .unwrap()
        .id;
        let judgment = Judgment {
            decided_at: UnixMillis(10),
            rationale: Rationale {
                hypothesis: Some("가설".to_string()),
                trigger: Some("계기".to_string()),
                entry: Some(Amount(100)),
                stop: Some(Amount(90)),
                target: Some(Amount(120)),
                expected_value: Some(Amount(5)),
            },
            sources: Vec::new(),
            scores: Vec::new(),
            consensus_score: 60,
        };
        judge(conn, id, judgment).unwrap();
        id
    }

    fn approval(number: i64) -> Approval {
        Approval {
            number,
            content_fingerprint: format!("sha256:{number}"),
        }
    }

    #[test]
    fn 바꿔_넣어_떼어_낸_승인_번호도_다른_매매는_쓸_수_없고_같은_매매는_다시_쓸_수_있다() {
        let (_dir, conn) = store();
        let a = judged(&conn);
        let b = judged(&conn);
        set_approval(&conn, a, approval(1), UnixMillis(20)).unwrap();
        set_approval(&conn, a, approval(2), UnixMillis(21)).unwrap();
        assert!(matches!(
            set_approval(&conn, b, approval(1), UnixMillis(22)),
            Err(StoreError::ApprovalNumberReused { number: 1 })
        ));
        let again = set_approval(&conn, a, approval(1), UnixMillis(23)).unwrap();
        assert_eq!(again.approval, Some(approval(1)));
    }

    #[test]
    fn 미체결_취소로_막힌_매매에는_승인을_붙일_수_없고_주문_전에_막힌_매매에는_된다() {
        let (_dir, conn) = store();
        let ordered = judged(&conn);
        transition(&conn, ordered, TradeState::Ordered, UnixMillis(20)).unwrap();
        let reason = BlockReason {
            kind: BlockKind::UnfilledCancelled,
            detail: "체결 없음".to_string(),
        };
        block(&conn, ordered, reason, UnixMillis(30)).unwrap();
        assert!(matches!(
            set_approval(&conn, ordered, approval(1), UnixMillis(40)),
            Err(StoreError::ApprovalLocked {
                state: TradeState::Blocked
            })
        ));

        let early = judged(&conn);
        let reason = BlockReason {
            kind: BlockKind::PrincipleOrRationale,
            detail: "근거 부족".to_string(),
        };
        block(&conn, early, reason, UnixMillis(30)).unwrap();
        set_approval(&conn, early, approval(2), UnixMillis(40)).unwrap();
    }
}
