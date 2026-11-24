//! 히스토리 화면의 읽기 조회 (18차, 결정 17 · additions.md C30 · T38).
//!
//! 매매 목록 · 매매 하나의 흐름 · 승인 요청 기록 · 템플릿 판 이력. 모두 읽기 전용 연결로 돈다.
//! 표 · 쓰기 길 · 판 번호는 그대로다(판 10).

use crate::approval::{ApprovalId, ApprovalStatus};
use crate::model::{AccountKind, TradeId, TradeState, UnixMillis};
use crate::plan::{TradeSide, TradeStateChange};
use crate::proposal::ProposalId;
use crate::template::TemplateId;
use crate::{Store, StoreError};

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
        let _ = (self, before, max, state);
        Ok(Vec::new())
    }

    /// 매매 하나의 흐름. 없는 번호면 [`StoreError::TradeNotFound`].
    pub fn trade_flow(&self, id: TradeId) -> Result<TradeFlow, StoreError> {
        let _ = (self, id);
        Err(StoreError::Incompatible("trade_flow 미구현".to_owned()))
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
        let _ = (self, before, max, status);
        Ok(Vec::new())
    }

    /// 템플릿 하나의 판 이력. 판 번호 순서다. 없는 템플릿이면 [`StoreError::TemplateNotFound`].
    pub fn template_history(&self, id: TemplateId) -> Result<Vec<TemplateHistoryLine>, StoreError> {
        let _ = (self, id);
        Err(StoreError::Incompatible(
            "template_history 미구현".to_owned(),
        ))
    }
}
