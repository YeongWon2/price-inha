//! 매매 계획(방향 · 수량)과 매매 상태 바뀜 이력 (후보 일괄 처리, 스키마 버전 8).
//!
//! - 방향: 판단과 같은 묶음에서 한 번 정하고 바뀌지 않는다. 서버를 다시 띄워도 팔기 매매를 알아본다.
//! - 수량: 승인이 붙기 전(논의중 · 판단됨 · 승인대기)에만 두거나 바꾼다. 실행 때 승인 내용과 대조한다.
//! - 상태 바뀜 이력: 매매를 만들거나 상태가 바뀔 때마다 표의 트리거가 한 줄 남긴다(어떤 길로 바꿔도).

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::codec::trade_state_from;
use crate::model::{Judgment, Trade, TradeId, TradeState, UnixMillis};
use crate::{Store, StoreError, trade};

/// 매매 방향.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TradeSide {
    /// 사기
    Buy,
    /// 팔기
    Sell,
}

impl TradeSide {
    /// 모든 방향.
    pub const ALL: [TradeSide; 2] = [TradeSide::Buy, TradeSide::Sell];
}

/// 매매 상태가 바뀐 한 번.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeStateChange {
    /// 이력 번호. 저장소 전체에서 커지기만 한다.
    pub seq: u64,
    /// 매매 번호.
    pub trade: TradeId,
    /// 바뀌기 전 상태. 매매를 만든 줄이면 `None`이다.
    pub from: Option<TradeState>,
    /// 바뀐 상태.
    pub to: TradeState,
    /// 바뀐 시각(만든 줄은 만든 시각, 바뀐 줄은 그 전이의 `at`).
    pub at: UnixMillis,
}

impl Store {
    /// [`Store::judge_trade`]와 같고, 같은 묶음 안에서 매매 방향을 함께 남긴다.
    /// 판단이 거부되면 방향도 남지 않는다. 방향은 그 뒤 바뀌지 않는다.
    pub fn judge_trade_with_side(
        &self,
        id: TradeId,
        judgment: Judgment,
        side: TradeSide,
    ) -> Result<Trade, StoreError> {
        self.write(|conn| judge_with_side(conn, id, judgment, side))
    }

    /// 매매 방향. 방향 없이 판단했거나 아직 판단하지 않았으면 `None`.
    /// 매매가 없으면 [`StoreError::TradeNotFound`].
    pub fn trade_side(&self, id: TradeId) -> Result<Option<TradeSide>, StoreError> {
        self.read(|conn| {
            trade::get(conn, id)?;
            let code: Option<Option<String>> = conn
                .query_row(
                    "SELECT side FROM trade_plans WHERE trade_id = ?1",
                    [id.0],
                    |r| r.get(0),
                )
                .optional()?;
            code.flatten().map(|c| side_from(&c)).transpose()
        })
    }

    /// 매매 수량(주)을 두거나 바꾼다.
    ///
    /// - 0이거나 `i64` 범위를 넘으면 [`StoreError::InvalidQuantity`].
    /// - 승인이 붙었거나 논의중 · 판단됨 · 승인대기가 아니면 [`StoreError::QuantityLocked`]이고 그대로다.
    /// - 매매가 없으면 [`StoreError::TradeNotFound`].
    pub fn set_trade_quantity(&self, id: TradeId, quantity: u64) -> Result<(), StoreError> {
        let quantity = i64::try_from(quantity)
            .ok()
            .filter(|q| *q > 0)
            .ok_or(StoreError::InvalidQuantity)?;
        self.write(|conn| {
            let now = trade::get(conn, id)?;
            let open = matches!(
                now.state,
                TradeState::Discussing | TradeState::Judged | TradeState::AwaitingApproval
            );
            if !open || now.approval.is_some() {
                return Err(StoreError::QuantityLocked { state: now.state });
            }
            ensure_plan(conn, id)?;
            conn.execute(
                "UPDATE trade_plans SET quantity = ?2 WHERE trade_id = ?1",
                params![id.0, quantity],
            )?;
            Ok(())
        })
    }

    /// 매매 수량. 두지 않았으면 `None`. 매매가 없으면 [`StoreError::TradeNotFound`].
    pub fn trade_quantity(&self, id: TradeId) -> Result<Option<u64>, StoreError> {
        self.read(|conn| {
            trade::get(conn, id)?;
            let found: Option<Option<i64>> = conn
                .query_row(
                    "SELECT quantity FROM trade_plans WHERE trade_id = ?1",
                    [id.0],
                    |r| r.get(0),
                )
                .optional()?;
            found
                .flatten()
                .map(|q| {
                    u64::try_from(q)
                        .map_err(|_| StoreError::Incompatible(format!("이상한 매매 수량: {q}")))
                })
                .transpose()
        })
    }

    /// 한 매매의 상태 바뀜 이력. 번호 순서다. 매매가 없으면 [`StoreError::TradeNotFound`].
    /// 스키마 버전 8 전에 있던 매매는 그 뒤의 바뀜만 있다.
    pub fn trade_state_changes(&self, id: TradeId) -> Result<Vec<TradeStateChange>, StoreError> {
        self.read(|conn| {
            trade::get(conn, id)?;
            changes(
                conn,
                "SELECT seq, trade_id, from_state, to_state, at FROM trade_state_changes
                 WHERE trade_id = ?1 ORDER BY seq",
                params![id.0],
            )
        })
    }

    /// 시각이 `[from, until)`인 모든 매매의 상태 바뀜. 시각 순서, 같은 시각이면 번호 순서다.
    /// `from >= until`이면 빈 목록이다.
    pub fn trade_state_changes_between(
        &self,
        from: UnixMillis,
        until: UnixMillis,
    ) -> Result<Vec<TradeStateChange>, StoreError> {
        self.read(|conn| {
            changes(
                conn,
                "SELECT seq, trade_id, from_state, to_state, at FROM trade_state_changes
                 WHERE at >= ?1 AND at < ?2 ORDER BY at, seq",
                params![from.0, until.0],
            )
        })
    }
}

// ───────────── 안에서만 쓰는 것 ─────────────

/// 판단하고 같은 묶음 안에서 방향을 남긴다([`Store::judge_trade_with_side`]의 길).
/// 담당 의견과 함께 판단하는 길(19차)도 같은 묶음 안에서 이것을 쓴다.
pub(crate) fn judge_with_side(
    conn: &Connection,
    id: TradeId,
    judgment: Judgment,
    side: TradeSide,
) -> Result<Trade, StoreError> {
    let judged = trade::judge(conn, id, judgment)?;
    ensure_plan(conn, id)?;
    conn.execute(
        "UPDATE trade_plans SET side = ?2 WHERE trade_id = ?1",
        params![id.0, side_code(side)],
    )?;
    Ok(judged)
}

pub(crate) fn side_code(side: TradeSide) -> &'static str {
    match side {
        TradeSide::Buy => "buy",
        TradeSide::Sell => "sell",
    }
}

pub(crate) fn side_from(code: &str) -> Result<TradeSide, StoreError> {
    TradeSide::ALL
        .into_iter()
        .find(|side| side_code(*side) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 매매 방향 값: {code}")))
}

/// 매매의 계획 줄이 없으면 빈 줄을 만든다.
fn ensure_plan(conn: &Connection, id: TradeId) -> Result<(), StoreError> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM trade_plans WHERE trade_id = ?1",
            [id.0],
            |_| Ok(()),
        )
        .optional()?;
    if exists.is_none() {
        conn.execute("INSERT INTO trade_plans (trade_id) VALUES (?1)", [id.0])?;
    }
    Ok(())
}

fn changes(
    conn: &Connection,
    sql: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<TradeStateChange>, StoreError> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(args)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(change_from(row)?);
    }
    Ok(out)
}

fn change_from(row: &Row<'_>) -> Result<TradeStateChange, StoreError> {
    let from: Option<String> = row.get(2)?;
    Ok(TradeStateChange {
        seq: crate::codec::seq(row, 0)?,
        trade: TradeId(row.get(1)?),
        from: from.as_deref().map(trade_state_from).transpose()?,
        to: trade_state_from(&row.get::<_, String>(3)?)?,
        at: UnixMillis(row.get(4)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::trade_state_code;

    #[test]
    fn 방향_값은_왕복한다() {
        for side in TradeSide::ALL {
            assert_eq!(side_from(side_code(side)).unwrap(), side);
        }
        assert!(side_from("hold").is_err());
        assert_eq!(trade_state_code(TradeState::Judged), "judged");
    }
}
