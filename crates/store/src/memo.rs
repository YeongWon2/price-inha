//! 결정 메모(어제 메모) (19차, 결정 17 · additions.md M11 · T23).
//!
//! 판단됨이 된 매매마다 하나(판 11). 정산 대기에서 정산됨 · 정산 못 함 가운데 하나로 한 번만
//! 바뀐다. 교훈은 정산됨일 때만 있다. 지우지 못한다.
//! 수익률 · 초과수익은 부동소수점을 쓰지 않고 만분율(1bp = 0.01%) 정수로 둔다.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::codec::trade_state_from;
use crate::history::page_limit;
use crate::model::{TradeId, UnixMillis};
use crate::plan::{TradeSide, side_code, side_from};
use crate::{Store, StoreError};

/// 결정 메모 번호. 저장소가 붙인다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecisionMemoId(pub i64);

/// 결정 메모 상태. 정산 대기에서 한 번만 바뀐다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecisionMemoState {
    /// 정산 대기
    Pending,
    /// 정산됨
    Settled,
    /// 정산 못 함
    Unsettleable,
}

impl DecisionMemoState {
    /// 모든 상태.
    pub const ALL: [DecisionMemoState; 3] = [
        DecisionMemoState::Pending,
        DecisionMemoState::Settled,
        DecisionMemoState::Unsettleable,
    ];
}

/// 새 결정 메모. 종목은 매매에서 가져온다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDecisionMemo {
    /// 판단된 매매.
    pub trade: TradeId,
    /// 결정 날짜(`YYYY-MM-DD`, 시장 시간대).
    pub date: String,
    /// 팀장 판단(사기 · 팔기).
    pub side: TradeSide,
    /// 만든 시각.
    pub at: UnixMillis,
}

/// 정산 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoSettlement {
    /// 정산됨.
    Settled {
        /// 수익률(만분율): 결정 날 종가 대비 정산 날 종가.
        return_bp: i64,
        /// 초과수익(만분율): 수익률 − 기준 지수 수익률. 기준 지수 시세가 없으면 `None`.
        excess_bp: Option<i64>,
        /// 정산까지 거래일 수.
        holding_days: u32,
        /// 교훈(2~4문장). 빈 글자는 거부한다.
        lesson: String,
    },
    /// 정산 못 함. 수익률 · 초과수익 · 보유일 · 교훈은 남지 않는다.
    Unsettleable,
}

/// 저장된 결정 메모.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionMemo {
    /// 번호.
    pub id: DecisionMemoId,
    /// 매매 번호.
    pub trade: TradeId,
    /// 결정 날짜(`YYYY-MM-DD`).
    pub date: String,
    /// 종목 코드(매매의 종목).
    pub symbol: String,
    /// 팀장 판단.
    pub side: TradeSide,
    /// 상태.
    pub state: DecisionMemoState,
    /// 수익률(만분율). 정산됨일 때만 있다.
    pub return_bp: Option<i64>,
    /// 초과수익(만분율). 정산됨이고 기준 지수 시세가 있을 때만 있다.
    pub excess_bp: Option<i64>,
    /// 보유일(거래일 수). 정산됨일 때만 있다.
    pub holding_days: Option<u32>,
    /// 교훈. 정산됨일 때만 있다.
    pub lesson: Option<String>,
    /// 만든 시각.
    pub created_at: UnixMillis,
    /// 정산 대기에서 바뀐 시각. 정산 대기면 `None`.
    pub settled_at: Option<UnixMillis>,
}

/// 결정 메모를 읽는 칸들(줄 모양은 [`memo_from`]).
const MEMO_COLUMNS: &str = "id, trade_id, date, symbol, side, state, return_bp, excess_bp, \
     holding_days, lesson, created_at, settled_at";

impl Store {
    /// 판단된 매매의 결정 메모를 정산 대기로 만든다.
    ///
    /// - 없는 매매: [`StoreError::TradeNotFound`]
    /// - 판단 전(논의중) 매매: [`StoreError::TradeNotJudged`]
    /// - 같은 매매로 이미 있음: [`StoreError::DecisionMemoExists`]
    /// - 날짜가 `YYYY-MM-DD` 모양이 아님: [`StoreError::InvalidDecisionMemo`]
    ///
    /// 거부되면 아무것도 남기지 않는다.
    pub fn create_decision_memo(&self, new: NewDecisionMemo) -> Result<DecisionMemo, StoreError> {
        if !is_date(&new.date) {
            return Err(StoreError::InvalidDecisionMemo(format!(
                "날짜는 YYYY-MM-DD여야 한다: {:?}",
                new.date
            )));
        }
        self.write(|conn| {
            let (state, decided_at, symbol): (String, Option<i64>, String) = conn
                .query_row(
                    "SELECT state, decided_at, symbol FROM trades WHERE id = ?1",
                    [new.trade.0],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?
                .ok_or(StoreError::TradeNotFound(new.trade))?;
            if decided_at.is_none() {
                return Err(StoreError::TradeNotJudged {
                    id: new.trade,
                    state: trade_state_from(&state)?,
                });
            }
            let exists = conn
                .query_row(
                    "SELECT 1 FROM decision_memos WHERE trade_id = ?1",
                    [new.trade.0],
                    |_| Ok(()),
                )
                .optional()?;
            if exists.is_some() {
                return Err(StoreError::DecisionMemoExists(new.trade));
            }
            conn.execute(
                "INSERT INTO decision_memos (trade_id, date, symbol, side, state, created_at) \
                 VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
                params![new.trade.0, new.date, symbol, side_code(new.side), new.at.0],
            )?;
            load(conn, DecisionMemoId(conn.last_insert_rowid()))
        })
    }

    /// 결정 메모 하나. 없는 번호면 [`StoreError::DecisionMemoNotFound`].
    pub fn get_decision_memo(&self, id: DecisionMemoId) -> Result<DecisionMemo, StoreError> {
        self.read(|conn| load(conn, id))
    }

    /// 정산 대기인 결정 메모를 정산한다(한 번만).
    ///
    /// - 없는 번호: [`StoreError::DecisionMemoNotFound`]
    /// - 이미 정산 대기가 아님: [`StoreError::DecisionMemoLocked`]
    /// - 정산됨인데 교훈이 빈 글자: [`StoreError::InvalidDecisionMemo`]
    ///
    /// 거부되면 아무것도 바꾸지 않는다.
    pub fn settle_decision_memo(
        &self,
        id: DecisionMemoId,
        outcome: MemoSettlement,
        at: UnixMillis,
    ) -> Result<DecisionMemo, StoreError> {
        self.write(|conn| {
            if load(conn, id)?.state != DecisionMemoState::Pending {
                return Err(StoreError::DecisionMemoLocked(id));
            }
            match outcome {
                MemoSettlement::Settled {
                    return_bp,
                    excess_bp,
                    holding_days,
                    lesson,
                } => {
                    if lesson.trim().is_empty() {
                        return Err(StoreError::InvalidDecisionMemo(
                            "정산됨에는 교훈이 있어야 한다".to_owned(),
                        ));
                    }
                    conn.execute(
                        "UPDATE decision_memos SET state = 'settled', return_bp = ?2, \
                         excess_bp = ?3, holding_days = ?4, lesson = ?5, settled_at = ?6 \
                         WHERE id = ?1",
                        params![
                            id.0,
                            return_bp,
                            excess_bp,
                            i64::from(holding_days),
                            lesson,
                            at.0
                        ],
                    )?;
                }
                MemoSettlement::Unsettleable => {
                    conn.execute(
                        "UPDATE decision_memos SET state = 'unsettleable', settled_at = ?2 \
                         WHERE id = ?1",
                        params![id.0, at.0],
                    )?;
                }
            }
            load(conn, id)
        })
    }

    /// 결정 날짜가 `on`(`YYYY-MM-DD`) 이하인 정산 대기 메모. 날짜 순서이고, 같은 날짜면 번호 순서다.
    pub fn decision_memos_pending(&self, on: &str) -> Result<Vec<DecisionMemo>, StoreError> {
        self.read(|conn| {
            memos(
                conn,
                &format!(
                    "SELECT {MEMO_COLUMNS} FROM decision_memos \
                     WHERE state = 'pending' AND date <= ?1 ORDER BY date, id"
                ),
                [on],
            )
        })
    }

    /// 결정 메모 목록. 번호가 큰 것(최근)부터 쪽 단위로 돌려준다.
    ///
    /// `before`가 있으면 그 번호보다 작은 메모만. `max`는 [`crate::HISTORY_PAGE_MAX`]로 줄인다.
    /// 0이면 빈 목록이다.
    pub fn decision_memos_page(
        &self,
        before: Option<DecisionMemoId>,
        max: usize,
    ) -> Result<Vec<DecisionMemo>, StoreError> {
        self.read(|conn| {
            memos(
                conn,
                &format!(
                    "SELECT {MEMO_COLUMNS} FROM decision_memos \
                     WHERE ?1 IS NULL OR id < ?1 ORDER BY id DESC LIMIT ?2"
                ),
                params![before.map(|b| b.0), page_limit(max)],
            )
        })
    }
}

// ───────────── 안에서만 쓰는 것 ─────────────

/// `YYYY-MM-DD` 모양(숫자 넷 · 줄표 · 숫자 둘 · 줄표 · 숫자 둘)인지.
fn is_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        })
}

fn load(conn: &Connection, id: DecisionMemoId) -> Result<DecisionMemo, StoreError> {
    memos(
        conn,
        &format!("SELECT {MEMO_COLUMNS} FROM decision_memos WHERE id = ?1"),
        [id.0],
    )?
    .pop()
    .ok_or(StoreError::DecisionMemoNotFound(id))
}

fn memos(
    conn: &Connection,
    sql: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<DecisionMemo>, StoreError> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(args)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(memo_from(row)?);
    }
    Ok(out)
}

fn memo_from(row: &Row<'_>) -> Result<DecisionMemo, StoreError> {
    let side: String = row.get(4)?;
    let state: String = row.get(5)?;
    let holding: Option<i64> = row.get(8)?;
    let settled_at: Option<i64> = row.get(11)?;
    Ok(DecisionMemo {
        id: DecisionMemoId(row.get(0)?),
        trade: TradeId(row.get(1)?),
        date: row.get(2)?,
        symbol: row.get(3)?,
        side: side_from(&side)?,
        state: state_from(&state)?,
        return_bp: row.get(6)?,
        excess_bp: row.get(7)?,
        holding_days: holding
            .map(|h| {
                u32::try_from(h)
                    .map_err(|_| StoreError::Incompatible(format!("이상한 보유일 값: {h}")))
            })
            .transpose()?,
        lesson: row.get(9)?,
        created_at: UnixMillis(row.get(10)?),
        settled_at: settled_at.map(UnixMillis),
    })
}

fn state_code(state: DecisionMemoState) -> &'static str {
    match state {
        DecisionMemoState::Pending => "pending",
        DecisionMemoState::Settled => "settled",
        DecisionMemoState::Unsettleable => "unsettleable",
    }
}

fn state_from(code: &str) -> Result<DecisionMemoState, StoreError> {
    DecisionMemoState::ALL
        .into_iter()
        .find(|s| state_code(*s) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 결정 메모 상태 값: {code}")))
}
