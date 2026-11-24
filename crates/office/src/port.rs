//! 바깥에서 끼우는 자리: 시계 · 이벤트 내보내기 · 가리기 · AI 상한 판정.

use async_trait::async_trait;
use pi_store::UnixMillis;

use crate::error::SinkError;
use crate::place::Agent;

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

/// 글 속 비밀 값 가리기(E38). 공지 글은 이것을 거친 뒤 저장하고 이벤트로 낸다.
/// 조립에서 비밀 보관의 가리기를 끼운다.
pub trait Redactor: Send + Sync {
    /// 가린 글.
    fn redact(&self, text: &str) -> String;
}

/// 하루 AI 호출 상한 판정(N18 · E45). 팀장 예비분을 남기는 셈은 이 포트가 한다.
/// 끼우지 않으면 늘 허용한다.
pub trait AiBudget: Send + Sync {
    /// 그 담당이 AI를 한 번 부를 수 있으면 한 번을 잡고 `true`, 상한이면 `false`.
    fn try_take(&self, agent: Agent) -> bool;
}
