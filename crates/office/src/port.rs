//! 바깥에서 끼우는 자리: 시계와 이벤트 내보내기.

use async_trait::async_trait;
use pi_store::UnixMillis;

use crate::error::SinkError;

/// 시계. 응답 객체의 `updatedAt`에 쓴다. 테스트가 시각을 정한다.
pub trait Clock: Send + Sync {
    /// 지금 시각.
    fn now(&self) -> UnixMillis;
}

/// 번호 있는(저장되는) 이벤트를 내보내는 포트.
///
/// 조립에서 gate의 `Publisher::publish`를 끼운다. 저장소가 붙인 번호를 돌려준다.
/// 화면이 하나도 열려 있지 않아도 이벤트는 여기로 나가 쌓인다(E37).
#[async_trait]
pub trait EventSink: Send + Sync {
    /// 이벤트 하나를 저장하고 내보낸다. 붙은 번호를 돌려준다.
    async fn publish(&self, kind: &str, payload: serde_json::Value) -> Result<u64, SinkError>;
}
