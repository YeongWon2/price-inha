//! 담당 의견과 팀장 요약 (19차, 결정 17 · additions.md M9 · D13 · T23).
//!
//! 담당 점수([`crate::RoleScore`]) 저장은 그대로 두고 옆 표에 둔다(판 11). 판단됨이 될 때 같은
//! 쓰기 안에서 함께 남기고, 그 뒤 고치지 못한다(기록 — 지우기 · 고치기 · 다시 넣기를 표에서 막는다).
//! 칸이 생기기 전 매매는 "기록 없음"(빈 값)이다.
//! 이유 글의 비밀키 가리기는 부르는 쪽이 한 뒤 넘긴다.

use crate::model::{Judgment, Trade, TradeId};
use crate::plan::TradeSide;
use crate::{Store, StoreError};

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
        let _ = (self, id, judgment, side, opinions, summary);
        Err(StoreError::Incompatible(
            "judge_trade_with_opinions 미구현".to_owned(),
        ))
    }

    /// 매매 하나의 담당 의견과 팀장 요약. 기록이 없으면 빈 값([`TradeOpinions::default`]).
    /// 없는 번호면 [`StoreError::TradeNotFound`].
    pub fn trade_opinions(&self, id: TradeId) -> Result<TradeOpinions, StoreError> {
        let _ = (self, id);
        Err(StoreError::Incompatible("trade_opinions 미구현".to_owned()))
    }
}
