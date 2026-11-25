//! 오류 타입. 어느 오류 글에도 봇 토큰 · 토큰이 든 주소 · 페어링 코드를 넣지 않는다(V87 · V88).

use chrono::{DateTime, Utc};

/// 채널 · 판정 오류.
///
/// `Display` · `Debug` 어디에도 봇 토큰이나 토큰이 든 주소를 통째로 넣지 않는다
/// (텔레그램은 토큰이 주소 경로에 든다 — 주소를 찍어야 하면 토큰 자리를 가린 글만).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// 토큰이 맞지 않는다(텔레그램 HTTP 401).
    #[error("봇 토큰이 맞지 않는다")]
    Unauthorized,
    /// 요청이 너무 많다(텔레그램 HTTP 429). `retry_after_secs`는 응답의 `parameters.retry_after`다.
    /// 이 모듈은 스스로 다시 하지 않는다 — 부르는 쪽이 정한 간격으로 다시 한다.
    #[error("요청이 너무 많다 — {retry_after_secs}초 뒤에 다시 한다")]
    RateLimited {
        /// 기다릴 초.
        retry_after_secs: u64,
    },
    /// 그 밖의 API 오류(`ok: false` 또는 2xx가 아닌 상태).
    #[error("메신저 API 오류 (HTTP {status}): {description}")]
    Api {
        /// HTTP 상태 코드.
        status: u16,
        /// 응답의 `description`(없으면 빈 글).
        description: String,
    },
    /// 응답을 읽을 수 없다(JSON이 아니거나 모양이 다르다).
    #[error("메신저 응답을 읽을 수 없다: {0}")]
    Malformed(String),
    /// 연결 · 읽기 실패. 글에 주소(토큰)를 넣지 않는다.
    #[error("메신저에 닿지 못했다: {0}")]
    Transport(String),
    /// 가리기에 실패했다. 이 글은 보내지 않는다(V89, 결정 24).
    #[error("가리기에 실패해 보내지 않는다")]
    Redaction,
    /// 페어링 장부를 읽거나 쓰지 못했다.
    #[error("페어링 장부를 읽거나 쓰지 못했다: {0}")]
    Book(String),
    /// 설정 값이 맞지 않는다(예: 주소를 만들 수 없다).
    #[error("채널 설정이 맞지 않는다: {0}")]
    Config(String),
}

/// 페어링 승인([`crate::Gatekeeper::approve`])의 거절.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApproveError {
    /// 기다리는 요청에 없는 코드다(만료된 코드 포함). 실패 횟수에 든다.
    #[error("맞는 페어링 코드가 아니다")]
    WrongCode,
    /// 틀린 코드가 [`crate::MAX_FAILED_APPROVALS`]번 쌓여 `until`까지 승인을 받지 않는다.
    #[error("틀린 코드가 많아 잠시 승인을 받지 않는다")]
    Locked {
        /// 잠금이 풀리는 시각.
        until: DateTime<Utc>,
    },
    /// 이미 허용된 사람이 있다(결정 24 — 많아야 1명). 먼저 지금 사람을 뺀다([`crate::Gatekeeper::revoke`]).
    #[error("이미 허용된 사람이 있다 — 먼저 지금 사람을 뺀다")]
    OwnerExists,
    /// 페어링 장부를 읽거나 쓰지 못했다.
    #[error("페어링 장부를 읽거나 쓰지 못했다: {0}")]
    Book(String),
}

/// 페어링 장부 포트의 오류.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("페어링 장부 오류: {0}")]
pub struct BookError(pub String);

/// 가리개 포트의 실패. 이 글은 보내지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, thiserror::Error)]
#[error("가리기 실패")]
pub struct RedactError;
