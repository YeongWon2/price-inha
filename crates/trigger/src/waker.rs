//! 깨울 대상 포트(조립에서 agent를 끼운다).

use async_trait::async_trait;

use crate::{Wake, WakeError};

/// 깨울 대상. 깨운 실행이 끝날 때까지 기다렸다가 결과를 돌려준다.
///
/// `Err`면 실패로 센다(물러서기 · 10번이면 끄기).
#[async_trait]
pub trait Waker: Send + Sync {
    /// 대상을 깨운다.
    async fn wake(&self, wake: Wake) -> Result<(), WakeError>;
}
