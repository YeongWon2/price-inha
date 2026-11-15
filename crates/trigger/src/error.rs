//! 오류.

use pi_market::MarketError;
use thiserror::Error;

use crate::JobId;

/// 이 모듈의 오류.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// 설정에 같은 작업 id가 두 번 있다.
    #[error("같은 작업 id가 두 번 있다: {0}")]
    DuplicateJob(JobId),
    /// 작업이 가리키는 시장의 어댑터가 없다.
    #[error("작업 {0}의 시장 어댑터가 없다")]
    MissingMarket(JobId),
    /// 시세 조건 값이 쓸 수 없다: 넘음 · 내려감의 기준값이 0 이하, 변동률의 기준가나 변동률이 0 이하.
    #[error("시세 조건 {0}의 값이 잘못됐다")]
    InvalidCondition(JobId),
    /// 설정 값이 쓸 수 없다(다시 걸기 간격이 음수 등).
    #[error("설정 값이 잘못됐다: {0}")]
    InvalidConfig(&'static str),
    /// 영수증 · 작업 상태 저장소(포트)가 실패했다. 이때는 깨우지 않는다.
    #[error("저장소 실패: {0}")]
    Store(#[from] StoreError),
    /// 시장 어댑터(달력 · 시세)가 실패했다.
    #[error("시장 어댑터 실패: {0}")]
    Market(#[from] MarketError),
}

/// 저장소 포트([`crate::TriggerStore`])의 오류.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct StoreError(pub String);

/// 깨울 대상 포트([`crate::Waker`])의 오류. 깨운 실행이 실패했다는 뜻이다.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct WakeError(pub String);
