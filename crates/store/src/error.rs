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

/// 전이가 거부된 이유. 종류가 늘어날 수 있다.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
    /// 막힘은 이유를 받는 [`crate::Store::block_trade`]로만 간다.
    /// 이유를 안 받는 [`crate::Store::transition_trade`]로 막힘을 주면 이것으로 거부된다.
    BlockReasonRequired,
    /// 막힌 이유 종류와 출발 상태의 짝이 맞지 않는다
    /// (판단됨 ↔ 원칙 · 근거 부족, 승인대기 ↔ 승인 거절, 주문됨 ↔ 미체결 취소).
    BlockReasonMismatch,
    /// 전체 정지로 가는 사유는 최대 손실([`crate::TeamCause::MaxLoss`])뿐이다.
    MaxLossOnly,
}

/// 저장소의 모든 오류.
///
/// 상태 전이 거부는 [`StoreError::TransitionDenied`]로만 나온다.
/// 거부된 호출은 아무것도 바꾸지 않는다(상태 · 기록 · 근거 모두 그대로).
/// 종류가 늘어날 수 있다.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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
    /// 주문됨 이후(주문됨 · 보유중 · 팔림 · 복기됨)의 매매에 승인을 붙이거나 바꾸려 했다.
    #[error("주문된 뒤에는 승인을 붙이거나 바꿀 수 없다: {state:?}")]
    ApprovalLocked {
        /// 그 매매의 지금 상태.
        state: TradeState,
    },
    /// 다른 매매에 이미 붙은 승인 번호를 붙이려 했다(승인 번호는 다시 쓰지 않는다).
    #[error("승인 번호 {number}는 이미 다른 매매에 붙어 있다")]
    ApprovalNumberReused {
        /// 다시 쓰려던 승인 번호.
        number: i64,
    },
    /// 이벤트 종류 이름이 규칙(`^[a-z][a-z0-9_.-]*$`, [`crate::EVENT_KIND_MAX_CHARS`]자 이하)에
    /// 어긋난다. 이벤트는 쌓이지 않고 번호도 쓰지 않는다. (3차)
    #[error("이벤트 종류 이름이 규칙에 어긋난다: {kind:?}")]
    InvalidEventKind {
        /// 거부된 이름.
        kind: String,
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
