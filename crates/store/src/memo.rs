//! 결정 메모(어제 메모) (19차, 결정 17 · additions.md M11 · T23).
//!
//! 판단됨이 된 매매마다 하나(판 11). 정산 대기에서 정산됨 · 정산 못 함 가운데 하나로 한 번만
//! 바뀐다. 교훈은 정산됨일 때만 있다. 지우지 못한다.
//! 수익률 · 초과수익은 부동소수점을 쓰지 않고 만분율(1bp = 0.01%) 정수로 둔다.

use crate::model::{TradeId, UnixMillis};
use crate::plan::TradeSide;
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

impl Store {
    /// 판단된 매매의 결정 메모를 정산 대기로 만든다.
    ///
    /// - 없는 매매: [`StoreError::TradeNotFound`]
    /// - 판단 전(논의중) 매매: [`StoreError::TradeNotJudged`]
    /// - 같은 매매로 이미 있음: [`StoreError::DecisionMemoExists`]
    ///
    /// 거부되면 아무것도 남기지 않는다.
    pub fn create_decision_memo(&self, new: NewDecisionMemo) -> Result<DecisionMemo, StoreError> {
        let _ = (self, new);
        Err(StoreError::Incompatible(
            "create_decision_memo 미구현".to_owned(),
        ))
    }

    /// 결정 메모 하나. 없는 번호면 [`StoreError::DecisionMemoNotFound`].
    pub fn get_decision_memo(&self, id: DecisionMemoId) -> Result<DecisionMemo, StoreError> {
        let _ = (self, id);
        Err(StoreError::Incompatible(
            "get_decision_memo 미구현".to_owned(),
        ))
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
        let _ = (self, id, outcome, at);
        Err(StoreError::Incompatible(
            "settle_decision_memo 미구현".to_owned(),
        ))
    }

    /// 결정 날짜가 `on`(`YYYY-MM-DD`) 이하인 정산 대기 메모. 날짜 순서이고, 같은 날짜면 번호 순서다.
    pub fn decision_memos_pending(&self, on: &str) -> Result<Vec<DecisionMemo>, StoreError> {
        let _ = (self, on);
        Ok(Vec::new())
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
        let _ = (self, before, max);
        Ok(Vec::new())
    }
}
