//! 저장소 오류. 오류 타입은 [`StoreError`] 하나다.

use crate::model::{Amount, RationaleField, TeamState, TradeId, TradeState, UnixMillis};

/// 거부된 전이가 무엇이었는지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// 투자 팀 상태 전이.
    Team {
        /// 떠나려던 상태.
        from: TeamState,
        /// 가려던 상태.
        to: TeamState,
    },
    /// 매매 상태 전이.
    Trade {
        /// 떠나려던 상태.
        from: TradeState,
        /// 가려던 상태.
        to: TradeState,
    },
}

/// 전이가 거부된 이유.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    /// 허용 표에 없는 전이다(같은 상태로 가는 것도 여기에 든다).
    NotAllowed,
    /// 전체 정지에서 나가려는데 "내 승인" 표시가 없다.
    OwnerApprovalRequired,
    /// 하루 한도 멈춤은 "다음 거래일 시작" 사유로만 풀린다.
    NextTradingDayOnly,
    /// 근거가 비어 주문됨으로 갈 수 없다. `missing`에는 빈 칸이 모두 들어간다.
    RationaleIncomplete {
        /// 빈 칸들. [`RationaleField`]를 적은 순서(가설 · 계기 · 들어간 값 · 손절값 · 목표값 · 기대값)다.
        missing: Vec<RationaleField>,
    },
}

/// 저장소의 모든 오류.
///
/// 상태 전이 거부는 [`StoreError::TransitionDenied`]로만 나온다.
/// 거부된 호출은 아무것도 바꾸지 않는다(상태 · 기록 · 근거 모두 그대로).
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// 상태 전이가 거부됐다.
    #[error("허용되지 않은 상태 전이: {transition:?} ({reason:?})")]
    TransitionDenied {
        /// 거부된 전이.
        transition: Transition,
        /// 거부된 이유.
        reason: DenyReason,
    },
    /// 판단에 쓴 자료의 "알게 된 시각"이 판단 시각보다 늦다. 판단은 저장되지 않는다.
    #[error(
        "자료 '{source_name}'의 알게 된 시각({known_at:?})이 판단 시각({decided_at:?})보다 늦다"
    )]
    SourceAfterDecision {
        /// 늦은 자료의 이름(여럿이면 준 순서에서 처음 것).
        source_name: String,
        /// 그 자료의 알게 된 시각.
        known_at: UnixMillis,
        /// 판단 시각.
        decided_at: UnixMillis,
    },
    /// 손절값을 좁히는 쪽(더 높게)이 아닌 값으로 바꾸려 했다.
    #[error("손절값은 좁히는 쪽으로만 바꿀 수 있다: 지금 {current:?}, 요청 {requested:?}")]
    StopNotTightened {
        /// 지금 손절값.
        current: Amount,
        /// 바꾸려던 값.
        requested: Amount,
    },
    /// 보유중이 아닌 매매의 손절값을 바꾸려 했다.
    #[error("보유중이 아닌 매매의 손절값은 바꿀 수 없다: {state:?}")]
    StopNotAdjustable {
        /// 그 매매의 지금 상태.
        state: TradeState,
    },
    /// 그 번호의 매매가 없다.
    #[error("매매를 찾을 수 없다: {0:?}")]
    TradeNotFound(TradeId),
    /// 파일이 이 저장소의 것이 아니거나, 이 프로그램이 아는 것보다 새 스키마다.
    #[error("저장소 파일을 쓸 수 없다: {0}")]
    Incompatible(String),
    /// SQLite 오류.
    #[error("SQLite 오류: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// JSON 오류.
    #[error("JSON 오류: {0}")]
    Json(#[from] serde_json::Error),
}
