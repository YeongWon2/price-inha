//! 히스토리 화면의 읽기 조회 (18차, 결정 17 · additions.md C30 · T38).
//!
//! 매매 목록 · 매매 하나의 흐름 · 승인 요청 기록 · 템플릿 판 이력. 모두 읽기 전용 연결로 돈다.
//! 표 · 쓰기 길 · 판 번호는 그대로다(판 10).

use crate::approval::{ApprovalId, ApprovalStatus};
use crate::model::{AccountKind, TradeId, TradeState, UnixMillis};
use crate::plan::{TradeSide, TradeStateChange};
use crate::proposal::ProposalId;
use crate::template::TemplateId;
use crate::{Store, StoreError, codec};
use rusqlite::{Connection, OptionalExtension, params};

/// 쪽 하나에 담는 줄 수의 상한. `max`가 이보다 크면 이 수로 줄인다.
pub const HISTORY_PAGE_MAX: usize = 100;

/// 매매 목록의 한 줄.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeListItem {
    /// 매매 번호.
    pub id: TradeId,
    /// 종목 코드.
    pub symbol: String,
    /// 계좌 종류.
    pub account: AccountKind,
    /// 지금 상태.
    pub state: TradeState,
    /// 만든 시각.
    pub created_at: UnixMillis,
    /// 마지막으로 바뀐 시각.
    pub updated_at: UnixMillis,
    /// 판단 시각. 판단 전에는 `None`.
    pub decided_at: Option<UnixMillis>,
}

/// 매매 하나의 흐름: 상태 바뀜(시각 순서) · 붙은 계획 · 근거 자료 이름.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeFlow {
    /// 매매 번호.
    pub id: TradeId,
    /// 상태 바뀜 줄. 시각 순서이고, 같은 시각이면 이력 번호 순서다.
    pub states: Vec<TradeStateChange>,
    /// 계획의 방향. 정해지지 않았으면 `None`.
    pub side: Option<TradeSide>,
    /// 계획의 수량. 정해지지 않았으면 `None`.
    pub quantity: Option<u64>,
    /// 붙은 근거 자료의 이름. 처음 붙인 순서다.
    pub fact_labels: Vec<String>,
}

/// 승인 요청 기록의 한 줄.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalLine {
    /// 승인 요청 번호.
    pub id: ApprovalId,
    /// 사람에게 보인 요약.
    pub summary: serde_json::Value,
    /// 지금 결과.
    pub status: ApprovalStatus,
    /// 만든 시각.
    pub created_at: UnixMillis,
    /// 닫힌(기다림에서 바뀐) 시각. 기다림이면 `None`.
    pub resolved_at: Option<UnixMillis>,
    /// 실행에 쓴 시각. 쓰지 않았으면 `None`.
    pub consumed_at: Option<UnixMillis>,
}

/// 템플릿 판 이력의 한 줄.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateHistoryLine {
    /// 판 번호.
    pub revision: u32,
    /// 그 판의 값.
    pub value: serde_json::Value,
    /// 그 판을 저장한 시각.
    pub at: UnixMillis,
    /// 사용자 변경 개선안으로 생긴 판이면 그 개선안 번호. 아니면 `None`.
    pub proposal: Option<ProposalId>,
}

impl Store {
    /// 매매 목록. 번호가 큰 것(최근)부터 쪽 단위로 돌려준다.
    ///
    /// - `before`가 있으면 그 번호보다 작은 매매만. 없으면 맨 처음 쪽.
    /// - `max`는 [`HISTORY_PAGE_MAX`]로 줄인다. 0이면 빈 목록이다.
    /// - `state`가 있으면 그 상태인 매매만. 없으면 전부.
    /// - 빈 저장소에서는 빈 목록이다.
    pub fn trade_page(
        &self,
        before: Option<TradeId>,
        max: usize,
        state: Option<TradeState>,
    ) -> Result<Vec<TradeListItem>, StoreError> {
        let limit = page_limit(max);
        if limit == 0 {
            return Ok(Vec::new());
        }
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, symbol, account, state, created_at, updated_at, decided_at
                 FROM trades
                 WHERE (?1 IS NULL OR id < ?1) AND (?2 IS NULL OR state = ?2)
                 ORDER BY id DESC LIMIT ?3",
            )?;
            let mut rows = stmt.query(params![
                before.map(|id| id.0),
                state.map(codec::trade_state_code),
                limit
            ])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(TradeListItem {
                    id: TradeId(row.get(0)?),
                    symbol: row.get(1)?,
                    account: codec::account_from(&row.get::<_, String>(2)?)?,
                    state: codec::trade_state_from(&row.get::<_, String>(3)?)?,
                    created_at: UnixMillis(row.get(4)?),
                    updated_at: UnixMillis(row.get(5)?),
                    decided_at: row.get::<_, Option<i64>>(6)?.map(UnixMillis),
                });
            }
            Ok(out)
        })
    }

    /// 매매 하나의 흐름. 없는 번호면 [`StoreError::TradeNotFound`].
    pub fn trade_flow(&self, id: TradeId) -> Result<TradeFlow, StoreError> {
        self.read(|conn| {
            let exists = conn
                .query_row("SELECT 1 FROM trades WHERE id = ?1", [id.0], |_| Ok(()))
                .optional()?;
            if exists.is_none() {
                return Err(StoreError::TradeNotFound(id));
            }
            let states = state_changes(conn, id)?;
            let plan = conn
                .query_row(
                    "SELECT side, quantity FROM trade_plans WHERE trade_id = ?1",
                    [id.0],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<i64>>(1)?,
                        ))
                    },
                )
                .optional()?;
            let (side, quantity) = match plan {
                Some((side, quantity)) => (
                    side.as_deref().map(side_from).transpose()?,
                    quantity
                        .map(|q| {
                            u64::try_from(q).map_err(|_| {
                                StoreError::Incompatible(format!("이상한 매매 수량: {q}"))
                            })
                        })
                        .transpose()?,
                ),
                None => (None, None),
            };
            let mut stmt = conn.prepare(
                "SELECT f.label FROM trade_facts AS tf JOIN facts AS f ON f.id = tf.fact_id
                 WHERE tf.trade_id = ?1 ORDER BY tf.rowid",
            )?;
            let fact_labels = stmt
                .query_map([id.0], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(TradeFlow {
                id,
                states,
                side,
                quantity,
                fact_labels,
            })
        })
    }

    /// 승인 요청 기록. 번호가 큰 것(최근)부터 쪽 단위로 돌려준다.
    ///
    /// `before` · `max` · `status`의 뜻은 [`Store::trade_page`]와 같다.
    pub fn approval_page(
        &self,
        before: Option<ApprovalId>,
        max: usize,
        status: Option<ApprovalStatus>,
    ) -> Result<Vec<ApprovalLine>, StoreError> {
        let limit = page_limit(max);
        if limit == 0 {
            return Ok(Vec::new());
        }
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, summary, status, created_at, resolved_at, consumed_at
                 FROM approvals
                 WHERE (?1 IS NULL OR id < ?1) AND (?2 IS NULL OR status = ?2)
                 ORDER BY id DESC LIMIT ?3",
            )?;
            let mut rows = stmt.query(params![
                before.map(|id| id.0),
                status.map(crate::approval::status_code),
                limit
            ])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                out.push(ApprovalLine {
                    id: ApprovalId(row.get(0)?),
                    summary: serde_json::from_str(&row.get::<_, String>(1)?)?,
                    status: crate::approval::status_from(&row.get::<_, String>(2)?)?,
                    created_at: UnixMillis(row.get(3)?),
                    resolved_at: row.get::<_, Option<i64>>(4)?.map(UnixMillis),
                    consumed_at: row.get::<_, Option<i64>>(5)?.map(UnixMillis),
                });
            }
            Ok(out)
        })
    }

    /// 템플릿 하나의 판 이력. 판 번호 순서다. 없는 템플릿이면 [`StoreError::TemplateNotFound`].
    pub fn template_history(&self, id: TemplateId) -> Result<Vec<TemplateHistoryLine>, StoreError> {
        self.read(|conn| {
            let exists = conn
                .query_row("SELECT 1 FROM templates WHERE id = ?1", [id.0], |_| Ok(()))
                .optional()?;
            if exists.is_none() {
                return Err(StoreError::TemplateNotFound(id));
            }
            let mut stmt = conn.prepare(
                "SELECT r.revision, r.value, r.at, u.proposal_id
                 FROM template_revisions AS r
                 LEFT JOIN user_revisions AS u
                   ON u.template_id = r.template_id AND u.revision = r.revision
                 WHERE r.template_id = ?1 ORDER BY r.revision",
            )?;
            let mut rows = stmt.query([id.0])?;
            let mut out = Vec::new();
            while let Some(row) = rows.next()? {
                let revision: i64 = row.get(0)?;
                out.push(TemplateHistoryLine {
                    revision: u32::try_from(revision).map_err(|_| {
                        StoreError::Incompatible(format!("이상한 템플릿 판 번호: {revision}"))
                    })?,
                    value: serde_json::from_str(&row.get::<_, String>(1)?)?,
                    at: UnixMillis(row.get(2)?),
                    proposal: row.get::<_, Option<i64>>(3)?.map(ProposalId),
                });
            }
            Ok(out)
        })
    }
}

/// 쪽 크기: `max`를 상한으로 줄인다.
pub(crate) fn page_limit(max: usize) -> i64 {
    // 상한이 100이라 i64에 언제나 들어간다.
    i64::try_from(max.min(HISTORY_PAGE_MAX)).unwrap_or(0)
}

/// 한 매매의 상태 바뀜 줄. 시각 순서, 같은 시각이면 이력 번호 순서다.
fn state_changes(conn: &Connection, id: TradeId) -> Result<Vec<TradeStateChange>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT seq, from_state, to_state, at FROM trade_state_changes
         WHERE trade_id = ?1 ORDER BY at, seq",
    )?;
    let mut rows = stmt.query([id.0])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let from: Option<String> = row.get(1)?;
        out.push(TradeStateChange {
            seq: codec::seq(row, 0)?,
            trade: id,
            from: from.as_deref().map(codec::trade_state_from).transpose()?,
            to: codec::trade_state_from(&row.get::<_, String>(2)?)?,
            at: UnixMillis(row.get(3)?),
        });
    }
    Ok(out)
}

fn side_from(code: &str) -> Result<TradeSide, StoreError> {
    match code {
        "buy" => Ok(TradeSide::Buy),
        "sell" => Ok(TradeSide::Sell),
        other => Err(StoreError::Incompatible(format!(
            "모르는 매매 방향 값: {other}"
        ))),
    }
}
