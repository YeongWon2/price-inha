//! 담당 의견과 팀장 요약 (19차, 결정 17 · additions.md M9 · D13 · T23).
//!
//! 담당 점수([`crate::RoleScore`]) 저장은 그대로 두고 옆 표에 둔다(판 11). 판단됨이 될 때 같은
//! 쓰기 안에서 함께 남기고, 그 뒤 고치지 못한다(기록 — 지우기 · 고치기 · 다시 넣기를 표에서 막는다).
//! 칸이 생기기 전 매매는 "기록 없음"(빈 값)이다.
//! 이유 글의 비밀키 가리기는 부르는 쪽이 한 뒤 넘긴다.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::model::{Judgment, Trade, TradeId};
use crate::plan::{self, TradeSide};
use crate::{Store, StoreError, trade};

/// 담당 의견의 방향.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stance {
    /// 강세
    Bullish,
    /// 약세
    Bearish,
    /// 중립
    Neutral,
}

impl Stance {
    /// 모든 방향.
    pub const ALL: [Stance; 3] = [Stance::Bullish, Stance::Bearish, Stance::Neutral];
}

/// 담당 의견의 확신도(3등급).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Conviction {
    /// 낮음
    Low,
    /// 보통
    Medium,
    /// 높음
    High,
}

impl Conviction {
    /// 모든 확신도.
    pub const ALL: [Conviction; 3] = [Conviction::Low, Conviction::Medium, Conviction::High];
}

/// 담당 점수 한 줄에 붙는 의견.
///
/// - 기권이면 방향 · 확신도는 `None`이어야 한다. 기권이 아니면 둘 다 있어야 한다.
/// - 보는 기간은 거래일 수로 1 이상이다. 모르면 `None`.
/// - `role`은 같은 판단의 담당 점수 가운데 하나여야 하고, 한 담당에 의견은 하나다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleOpinion {
    /// 담당 이름(담당 점수의 `role`과 같다).
    pub role: String,
    /// 방향. 기권이면 `None`.
    pub stance: Option<Stance>,
    /// 확신도. 기권이면 `None`.
    pub conviction: Option<Conviction>,
    /// 한 줄 이유 글.
    pub reason: String,
    /// 보는 기간(거래일 수, 1 이상). 모르면 `None`.
    pub horizon_days: Option<u32>,
    /// 의견을 못 냈다(0점과 따로 남긴다).
    pub abstained: bool,
}

/// 매매마다 한 줄인 팀장 요약.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeSummary {
    /// 팀장 요약(2~4문장).
    pub summary: String,
    /// 뒤집힐 조건("이러면 뒤집힌다" 한 줄).
    pub reversal: String,
}

/// 매매 하나에 남은 담당 의견과 팀장 요약.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TradeOpinions {
    /// 담당 의견. 준 순서대로다. 기록이 없으면 비어 있다.
    pub opinions: Vec<RoleOpinion>,
    /// 팀장 요약. 기록이 없으면 `None`.
    pub summary: Option<TradeSummary>,
}

impl Store {
    /// [`Store::judge_trade_with_side`]와 같고, 같은 쓰기 안에서 담당 의견과 팀장 요약을 함께 남긴다.
    ///
    /// - 판단이 거부되면([`StoreError::SourceAfterDecision`] · [`StoreError::TransitionDenied`] 등)
    ///   방향 · 의견 · 요약도 남지 않는다.
    /// - 의견이 [`RoleOpinion`]의 규칙에 맞지 않으면 [`StoreError::InvalidOpinion`]이고 판단도
    ///   남지 않는다(매매는 논의중 그대로).
    /// - 한 번 남은 의견 · 요약은 고치지 못한다(같은 매매로 다시 부르면 판단이 거부된다).
    pub fn judge_trade_with_opinions(
        &self,
        id: TradeId,
        judgment: Judgment,
        side: TradeSide,
        opinions: &[RoleOpinion],
        summary: &TradeSummary,
    ) -> Result<Trade, StoreError> {
        check_opinions(&judgment, opinions)?;
        self.write(|conn| {
            let judged = plan::judge_with_side(conn, id, judgment, side)?;
            for opinion in opinions {
                insert_opinion(conn, id, opinion)?;
            }
            conn.execute(
                "INSERT INTO trade_summaries (trade_id, summary, reversal) VALUES (?1, ?2, ?3)",
                params![id.0, summary.summary, summary.reversal],
            )?;
            Ok(judged)
        })
    }

    /// 매매 하나의 담당 의견과 팀장 요약. 기록이 없으면 빈 값([`TradeOpinions::default`]).
    /// 없는 번호면 [`StoreError::TradeNotFound`].
    pub fn trade_opinions(&self, id: TradeId) -> Result<TradeOpinions, StoreError> {
        self.read(|conn| {
            trade::get(conn, id)?;
            let mut stmt = conn.prepare(
                "SELECT role, stance, conviction, reason, horizon_days, abstained \
                 FROM role_opinions WHERE trade_id = ?1 ORDER BY id",
            )?;
            let mut rows = stmt.query([id.0])?;
            let mut opinions = Vec::new();
            while let Some(row) = rows.next()? {
                opinions.push(opinion_from(row)?);
            }
            let summary = conn
                .query_row(
                    "SELECT summary, reversal FROM trade_summaries WHERE trade_id = ?1",
                    [id.0],
                    |r| {
                        Ok(TradeSummary {
                            summary: r.get(0)?,
                            reversal: r.get(1)?,
                        })
                    },
                )
                .optional()?;
            Ok(TradeOpinions { opinions, summary })
        })
    }
}

// ───────────── 안에서만 쓰는 것 ─────────────

/// 의견이 [`RoleOpinion`]의 규칙에 맞는지 본다. 판단의 담당 점수에 있는 담당만, 한 담당에 하나.
fn check_opinions(judgment: &Judgment, opinions: &[RoleOpinion]) -> Result<(), StoreError> {
    let scored: HashSet<&str> = judgment.scores.iter().map(|s| s.role.as_str()).collect();
    let mut seen = HashSet::new();
    for o in opinions {
        let role = &o.role;
        let why = if !scored.contains(role.as_str()) {
            Some("담당 점수에 없는 담당이다")
        } else if !seen.insert(role.as_str()) {
            Some("같은 담당의 의견이 둘이다")
        } else if o.abstained && (o.stance.is_some() || o.conviction.is_some()) {
            Some("기권인데 방향이나 확신도가 있다")
        } else if !o.abstained && (o.stance.is_none() || o.conviction.is_none()) {
            Some("기권이 아닌데 방향이나 확신도가 없다")
        } else if o.horizon_days == Some(0) {
            Some("보는 기간은 1 이상이어야 한다")
        } else {
            None
        };
        if let Some(why) = why {
            return Err(StoreError::InvalidOpinion(format!("{role}: {why}")));
        }
    }
    Ok(())
}

fn insert_opinion(conn: &Connection, id: TradeId, o: &RoleOpinion) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO role_opinions \
         (trade_id, role, stance, conviction, reason, horizon_days, abstained) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id.0,
            o.role,
            o.stance.map(stance_code),
            o.conviction.map(conviction_code),
            o.reason,
            o.horizon_days.map(i64::from),
            o.abstained
        ],
    )?;
    Ok(())
}

fn opinion_from(row: &Row<'_>) -> Result<RoleOpinion, StoreError> {
    let stance: Option<String> = row.get(1)?;
    let conviction: Option<String> = row.get(2)?;
    let horizon: Option<i64> = row.get(4)?;
    let horizon_days = horizon
        .map(|h| {
            u32::try_from(h)
                .map_err(|_| StoreError::Incompatible(format!("이상한 보는 기간 값: {h}")))
        })
        .transpose()?;
    Ok(RoleOpinion {
        role: row.get(0)?,
        stance: stance.as_deref().map(stance_from).transpose()?,
        conviction: conviction.as_deref().map(conviction_from).transpose()?,
        reason: row.get(3)?,
        horizon_days,
        abstained: row.get(5)?,
    })
}

fn stance_code(stance: Stance) -> &'static str {
    match stance {
        Stance::Bullish => "bullish",
        Stance::Bearish => "bearish",
        Stance::Neutral => "neutral",
    }
}

fn stance_from(code: &str) -> Result<Stance, StoreError> {
    Stance::ALL
        .into_iter()
        .find(|s| stance_code(*s) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 의견 방향 값: {code}")))
}

fn conviction_code(conviction: Conviction) -> &'static str {
    match conviction {
        Conviction::Low => "low",
        Conviction::Medium => "medium",
        Conviction::High => "high",
    }
}

fn conviction_from(code: &str) -> Result<Conviction, StoreError> {
    Conviction::ALL
        .into_iter()
        .find(|c| conviction_code(*c) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 확신도 값: {code}")))
}
