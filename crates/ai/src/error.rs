//! 이 모듈의 오류 타입.

use std::time::Duration;

/// 이 모듈이 내는 오류. 문장과 `Debug`에 자격 글자가 들어가지 않는다(E38).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AiError {
    /// 자격이 등록되지 않았다. 바깥으로 요청을 보내지 않았다(V31).
    #[error("AI 키가 설정되지 않았습니다")]
    NotConfigured,
    /// 인증 실패(401 · 403). 다시 시도하지 않는다.
    #[error("AI 공급자 인증에 실패했습니다 (상태 {status})")]
    Auth {
        /// HTTP 상태 코드.
        status: u16,
    },
    /// 한도 초과(429). 다시 시도한 뒤에도 실패했다.
    #[error("AI 공급자 한도를 넘었습니다")]
    RateLimited {
        /// 응답의 `retry-after`(초)가 있으면 그 값.
        retry_after: Option<Duration>,
    },
    /// 공급자 장애(5xx · 529). 다시 시도한 뒤에도 실패했다.
    #[error("AI 공급자에 장애가 있습니다 (상태 {status})")]
    ProviderUnavailable {
        /// HTTP 상태 코드.
        status: u16,
    },
    /// 잘못된 요청(400). 다시 시도하지 않는다.
    #[error("AI 공급자가 요청을 받아들이지 않았습니다: {message}")]
    InvalidRequest {
        /// 공급자가 준 설명. 자격 글자는 가려져 있다.
        message: String,
    },
    /// 설정한 시간 안에 응답이 오지 않았다.
    #[error("AI 공급자 응답이 시간 안에 오지 않았습니다")]
    Timeout,
    /// 공급자에 연결하지 못했다.
    #[error("AI 공급자에 연결하지 못했습니다")]
    Connection,
    /// 성공 응답인데 읽을 수 없는 모양이다.
    #[error("AI 공급자 응답을 읽지 못했습니다")]
    InvalidResponse,
}
