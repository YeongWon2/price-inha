//! 오류.

use thiserror::Error;

/// 승인 확인에 실패했거나 거절된 까닭(E7 · E8 · P19).
///
/// 어느 것이든 결과는 **거절**이다. 팀이 대신 판단하지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectReason {
    /// 사용자가 거절을 눌렀다(V43: 바로 된다).
    UserRejected,
    /// 그런 승인 번호가 없다(E8).
    NotFound,
    /// 이미 처리된 번호다. 첫 결과는 그대로다(E8 · V45).
    AlreadyResolved,
    /// 만료됐다: 신호 유효 시간이 지났다(E8).
    Expired,
    /// 지문이 다르다: 화면에 보인 내용이나 실행할 내용이 승인 요청의 내용과 다르다(E7 · V44).
    FingerprintMismatch,
    /// 승인됨이 아니다(기다림 · 거절 · 답 없음 · 만료). 승인 전에는 진행되지 않는다(V42).
    NotApproved,
    /// 이미 실행에 썼다(한 번만 소비).
    AlreadyConsumed,
    /// 전체 정지 중이라 실전 승격을 할 수 없다.
    FullStop,
}

/// 이 모듈의 오류.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// 저장소 오류.
    #[error("저장소 오류: {0}")]
    Store(#[from] pi_store::StoreError),
    /// 승인이 필요한 주문은 지정가로만 낸다(결정 2026-11-16 7번).
    #[error("승인이 필요한 주문은 지정가로만 낸다")]
    LimitOrderRequired,
    /// 대상이 앞뒤가 맞지 않는다(예: 사는 주문인데 신호 종류가 손절).
    #[error("승인 대상이 올바르지 않다: {0}")]
    InvalidSubject(String),
    /// 요청을 만드는 시각에 이미 신호 유효 시간이 지났다.
    #[error("신호 유효 시간이 이미 지났다")]
    SignalAlreadyExpired,
    /// 전체 정지 중에는 실전 승격 승인 요청을 만들지 않는다(전체 정지는 재개를 사람만 한다).
    #[error("전체 정지 중에는 실전 승격을 요청할 수 없다")]
    FullStop,
    /// 승인을 실행에 쓸 수 없다. 실행하지 않는다.
    #[error("승인을 쓸 수 없다: {0:?}")]
    Denied(RejectReason),
}
