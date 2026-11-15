//! 오류.

use pi_store::{StoreError, TradeId, TradeState, UnixMillis};
use thiserror::Error;

/// 이 모듈의 오류. 거부된 호출은 아무것도 바꾸지 않는다(기록 · 상태 · 개선안 모두 그대로).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// 저장소가 거부했거나 읽지 못했다(없는 매매 · 허용되지 않는 전이 · 승인 문제 포함).
    #[error("저장소 오류: {0}")]
    Store(#[from] StoreError),
    /// 팔림이 아닌 매매는 복기할 수 없다(V19).
    #[error("팔린 매매만 복기한다: {id:?}는 {state:?}")]
    NotSold {
        /// 매매 번호.
        id: TradeId,
        /// 지금 상태.
        state: TradeState,
    },
    /// 자유 글 실수가 비었다.
    #[error("실수 글이 비었다")]
    BlankMistake,
    /// 승인 번호 없이 규칙을 바꾸려 했다(E35).
    #[error("승인 번호 없이는 규칙을 바꿀 수 없다")]
    ApprovalRequired,
    /// 절대 상한을 넘거나 값 모양이 틀렸다(M4 · E28).
    #[error("규칙 값이 상한 · 경계를 어겼다: {0}")]
    OverCap(#[from] pi_template::OverCap),
    /// 냉각중인 개선안을 다음 거래일 시작 전에 반영하려 했다(E28).
    #[error("다음 거래일 시작({until:?}) 전이라 반영할 수 없다: {at:?}")]
    StillCooling {
        /// 반영할 수 있는 가장 이른 시각.
        until: UnixMillis,
        /// 반영하려던 시각.
        at: UnixMillis,
    },
    /// 개선안 내용을 읽을 수 없다.
    #[error("개선안 내용을 읽을 수 없다: {0}")]
    BadProposal(String),
    /// 저장된 복기 기록을 읽을 수 없다.
    #[error("복기 기록을 읽을 수 없다: {0}")]
    BadRecord(String),
    /// 손익 합이 정수 범위를 넘었다.
    #[error("손익 합이 범위를 넘었다")]
    Overflow,
    /// 하루의 끝이 시작보다 늦지 않다.
    #[error("하루 범위가 올바르지 않다")]
    BadDay,
}
