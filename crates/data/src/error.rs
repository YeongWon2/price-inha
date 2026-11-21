//! 오류 구분(E14). 어떤 오류도 자료 키를 담지 않는다. 주소를 담는 오류는 키를 가린 주소만 담는다.

use crate::model::SourceName;
use crate::news::NewsProviderId;

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

impl DataError {
    /// 같은 요청을 나중에 다시 하면 될 수도 있는 실패인가 (조립 · trigger가 다시 시도를 가른다).
    ///
    /// `true`: 시간 초과 · 요청 한도 · 닿지 않음(연결 실패 · 5xx).
    /// `false`: 다시 해도 같은 실패(키 없음 · 키 거절 · 거절 · 형식 · 요청 오류 · 잘림 · 키 읽기 오류 ·
    /// 기본 주소 오류)와 주소 오류(404 · 3xx로 닿지 않음).
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Timeout { .. } | Self::RateLimited { .. } => true,
            Self::Unreachable { detail, .. } => !is_address_error(detail),
            _ => false,
        }
    }
}

/// 닿지 않음의 `detail`이 주소를 고쳐야 풀리는 HTTP 상태(404 · 3xx)인가.
fn is_address_error(detail: &str) -> bool {
    detail
        .strip_prefix("HTTP ")
        .and_then(|code| code.parse::<u16>().ok())
        .is_some_and(|code| code == 404 || (300..400).contains(&code))
}

/// 뉴스 공급자 오류(4차). 모든 변형이 공급자를 싣는다.
///
/// 어떤 변형도 클라이언트 ID · 시크릿 값을 담지 않는다. 결과가 비면 오류가 아니라
/// [`crate::NewsFetched::Missing`]이다(E32).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NewsError {
    /// 검색을 맡을 공급자가 없다(등록부가 비었거나 기본이 등록되지 않았다). 요청하지 않았다.
    #[error("{provider} 뉴스 공급자가 등록되어 있지 않습니다")]
    NoProvider {
        /// 찾은 공급자(기본으로 정한 것, 없으면 첫 공급자 이름).
        provider: NewsProviderId,
    },
    /// 공급자 키가 없거나 칸이 모자란다(D9). 요청하지 않았다.
    #[error("{provider} 뉴스 키가 없습니다")]
    NoKey {
        /// 공급자.
        provider: NewsProviderId,
    },
    /// 키를 보관소에서 읽지 못했다(입출력 · 손상). 키 없음으로 묻지 않는다.
    #[error("{provider} 뉴스 키를 보관소에서 읽을 수 없습니다")]
    KeyUnreadable {
        /// 공급자.
        provider: NewsProviderId,
    },
    /// 공급자가 키를 받아들이지 않았다(401 · 403).
    #[error("{provider}가 뉴스 키를 받아들이지 않았습니다 ({code})")]
    KeyRejected {
        /// 공급자.
        provider: NewsProviderId,
        /// 공급자 오류 코드나 HTTP 상태. 키를 담지 않는다.
        code: String,
    },
    /// 요청 한도를 넘었다(429).
    #[error("{provider} 뉴스 요청 한도를 넘었습니다")]
    RateLimited {
        /// 공급자.
        provider: NewsProviderId,
    },
    /// 정해진 시간 안에 응답이 오지 않았다.
    #[error("{provider} 뉴스 응답 시간이 지났습니다")]
    Timeout {
        /// 공급자.
        provider: NewsProviderId,
    },
    /// 공급자에 닿지 않았다(연결 실패 · 5xx).
    #[error("{provider}에 닿지 않았습니다 ({detail})")]
    Unreachable {
        /// 공급자.
        provider: NewsProviderId,
        /// 실패 종류. 키를 담지 않는다.
        detail: String,
    },
    /// 주소가 틀렸다(404 · 3xx). 다시 해도 안 된다. 리다이렉트는 따라가지 않는다.
    #[error("{provider} 뉴스 주소가 올바르지 않습니다")]
    BadAddress {
        /// 공급자.
        provider: NewsProviderId,
    },
    /// 공급자가 요청을 거절했다(400 `SE01`~`SE04` · `SE06` 등).
    #[error("{provider}가 뉴스 요청을 거절했습니다 ({code})")]
    Rejected {
        /// 공급자.
        provider: NewsProviderId,
        /// 공급자 오류 코드(예: `SE01`)나 HTTP 상태.
        code: String,
    },
    /// 응답 모양이 명세와 다르다(칸 없음 · 시각 · 링크가 깨짐 · 본문 상한 넘음).
    #[error("{provider} 뉴스 응답을 읽을 수 없습니다: {detail}")]
    Format {
        /// 공급자.
        provider: NewsProviderId,
        /// 무엇이 다른가. 본문 · 키를 담지 않는다.
        detail: String,
    },
    /// 요청이 올바르지 않다(검색어 · 개수). 요청하지 않았다.
    #[error("{provider} 뉴스 요청이 올바르지 않습니다: {detail}")]
    InvalidRequest {
        /// 공급자.
        provider: NewsProviderId,
        /// 무엇이 틀렸나.
        detail: String,
    },
    /// 기본 주소가 `https`가 아니다(루프백 주소는 예외).
    #[error("{provider} 뉴스 기본 주소는 https여야 합니다")]
    InsecureBaseUrl {
        /// 공급자.
        provider: NewsProviderId,
    },
}

impl NewsError {
    /// 같은 요청을 나중에 다시 해 볼 만한가([`DataError::is_retryable`]과 같은 갈래):
    /// 시간 초과 · 한도 · 닿지 않음만 `true`.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Timeout { .. } | Self::RateLimited { .. } | Self::Unreachable { .. }
        )
    }
}
