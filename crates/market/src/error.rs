//! 오류 구분. 어떤 오류도 자격(client_id · client_secret)이나 토큰을 담지 않는다.

use std::time::Duration;

/// 시장 어댑터 오류.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MarketError {
    /// 증권사 키가 없다(V31). 이때는 바깥으로 요청을 보내지 않는다.
    #[error("증권사 키가 없습니다")]
    NoCredentials,
    /// 인증 실패. 401(`invalid-token` · 다시 발급한 뒤에도 `expired-token` 등)이나
    /// 토큰 발급의 `invalid_client`.
    #[error("증권사 인증에 실패했습니다 ({code})")]
    AuthFailed {
        /// 증권사가 준 오류 코드.
        code: String,
    },
    /// 허용 IP가 아니다. 403 `ip-not-allowed` 또는 토큰 발급의 `access_denied`.
    #[error("허용되지 않은 IP입니다")]
    IpNotAllowed,
    /// 한도 초과. 정해진 횟수만큼 다시 시도한 뒤에도 429다.
    #[error("요청 한도를 넘었습니다")]
    RateLimited {
        /// 마지막 429의 `Retry-After`.
        retry_after: Option<Duration>,
    },
    /// 점검 · 장애. 5xx 전부(`maintenance` · `internal-error` 포함).
    #[error("증권사 서버가 점검 중이거나 장애입니다 (HTTP {status})")]
    Unavailable {
        /// HTTP 상태.
        status: u16,
        /// 증권사가 준 오류 코드(본문을 읽을 수 없으면 없다). 점검이면 `maintenance`.
        code: Option<String>,
    },
    /// 증권사가 요청을 거절했다. 위에 없는 4xx(예: 404 `stock-not-found`).
    #[error("증권사가 요청을 거절했습니다 (HTTP {status}, {code})")]
    Rejected {
        /// HTTP 상태.
        status: u16,
        /// 증권사가 준 오류 코드.
        code: String,
    },
    /// 응답 모양이 명세와 다르다.
    #[error("증권사 응답을 읽을 수 없습니다: {detail}")]
    UnexpectedResponse {
        /// 무엇이 달랐는가. 응답 본문 전체나 자격을 담지 않는다.
        detail: String,
    },
    /// 연결 실패(연결 거부 · 시간 초과 포함).
    #[error("증권사에 연결하지 못했습니다: {detail}")]
    Connection {
        /// 실패 종류. 자격을 담지 않는다.
        detail: String,
    },
    /// 종목 글자가 틀렸다([`crate::Symbol::parse`]의 규칙). 받은 글자를 담지 않는다.
    /// 어댑터는 이때 바깥으로 요청을 보내지 않는다.
    #[error("종목 글자가 올바르지 않습니다")]
    InvalidSymbol,
    /// 기본 주소가 `https`가 아니다(루프백 주소만 예외). 바깥으로 요청을 보내지 않는다.
    #[error("증권사 기본 주소는 https여야 합니다")]
    InsecureBaseUrl,
    /// 그 시장의 시세 공급자를 고르지 못했다(등록부에 없거나, 경로 · 기본이 등록되지 않은
    /// 공급자를 가리킨다). 다른 공급자로 넘어가지 않고 요청도 보내지 않는다.
    #[error("{market:?} 시장의 시세 공급자가 없습니다")]
    NoProvider {
        /// 공급자를 찾은 시장.
        market: crate::Market,
    },
}
