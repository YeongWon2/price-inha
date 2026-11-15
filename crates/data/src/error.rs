//! 오류 구분(E14). 어떤 오류도 자료 키를 담지 않는다. 주소를 담는 오류는 키를 가린 주소만 담는다.

use crate::model::SourceName;

/// 자료 어댑터 오류.
///
/// 자료가 비어 있는 것은 오류가 아니다. 그때는 [`crate::Fetched::Missing`]이다(E32).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DataError {
    /// 그 출처의 키가 없다(D9). 이때는 바깥으로 요청을 보내지 않는다.
    #[error("{origin} 자료 키가 없습니다")]
    NoKey {
        /// 출처.
        origin: SourceName,
    },
    /// 출처가 키를 받아들이지 않았다(등록되지 않은 키 · 쓸 수 없는 키).
    #[error("{origin}가 자료 키를 받아들이지 않았습니다 ({code})")]
    KeyRejected {
        /// 출처.
        origin: SourceName,
        /// 출처가 준 코드(예: DART `010`, ECOS `INFO-100`).
        code: String,
    },
    /// 요청 한도를 넘었다(DART `020`, HTTP 429).
    #[error("{origin} 요청 한도를 넘었습니다")]
    RateLimited {
        /// 출처.
        origin: SourceName,
    },
    /// 정해진 시간 안에 응답이 오지 않았다.
    #[error("{origin} 응답 시간이 지났습니다: {url}")]
    Timeout {
        /// 출처.
        origin: SourceName,
        /// 요청 주소. 키는 가려져 있다.
        url: String,
    },
    /// 출처에 닿지 않았다(연결 실패 · 5xx · 점검).
    #[error("{origin}에 닿지 않았습니다: {url} ({detail})")]
    Unreachable {
        /// 출처.
        origin: SourceName,
        /// 요청 주소. 키는 가려져 있다.
        url: String,
        /// 실패 종류. 키를 담지 않는다.
        detail: String,
    },
    /// 출처가 요청을 거절했다(위에 없는 출처 코드나 4xx).
    #[error("{origin}가 요청을 거절했습니다 ({code})")]
    Rejected {
        /// 출처.
        origin: SourceName,
        /// 출처가 준 코드나 HTTP 상태.
        code: String,
    },
    /// 응답 모양이 명세와 다르다.
    #[error("{origin} 응답을 읽을 수 없습니다: {detail}")]
    Format {
        /// 출처.
        origin: SourceName,
        /// 무엇이 달랐는가. 응답 본문 전체나 키를 담지 않는다.
        detail: String,
    },
    /// 요청이 틀렸다(고유번호 모양, 기간 순서 등). 바깥으로 요청을 보내지 않는다.
    #[error("자료 요청이 올바르지 않습니다: {detail}")]
    InvalidRequest {
        /// 무엇이 틀렸는가.
        detail: String,
    },
    /// 기본 주소가 `https`가 아니다(루프백 주소만 예외).
    #[error("{origin} 기본 주소는 https여야 합니다")]
    InsecureBaseUrl {
        /// 출처.
        origin: SourceName,
    },
    /// 출처에 받을 양의 상한(ECOS 10000행, DART 20쪽)보다 많은 자료가 있어 다 받지 못했다.
    /// 잘린 결과를 완전한 것처럼 돌려주지 않는다(2차 1번).
    #[error("{origin} 자료가 받을 수 있는 양보다 많아 잘렸습니다")]
    Truncated {
        /// 출처.
        origin: SourceName,
    },
    /// 키 보관소를 읽지 못했다(입출력 · 손상). 키가 없는 것([`DataError::NoKey`])과 다르다(2차 2번).
    /// 이때는 바깥으로 요청을 보내지 않는다.
    #[error("{origin} 자료 키를 보관소에서 읽을 수 없습니다")]
    KeyUnreadable {
        /// 출처.
        origin: SourceName,
    },
}
