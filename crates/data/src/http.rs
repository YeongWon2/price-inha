//! 세 출처가 함께 쓰는 HTTP 부분: 기본 주소 규칙, 키를 가린 주소, 보내기와 오류 구분.
//!
//! reqwest 오류의 글은 요청 주소(키 포함)를 담을 수 있어 오류에 옮기지 않는다. 종류만 고정된 글로 남긴다.

use std::net::IpAddr;
use std::time::Duration;

use reqwest::{Client, StatusCode};
use url::Url;

use crate::{DataError, SourceName};

/// 출처 · 오류 주소에서 키 자리에 넣는 글자.
pub(crate) const MASK: &str = "***";

/// 응답 본문의 최대 크기. 넘으면 형식 오류다.
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// 보낼 주소와 보여 줄 주소(키를 가린 것).
#[derive(Clone)]
pub(crate) struct Target {
    /// 실제로 보내는 주소. 키가 들어 있다. 오류 · 기록에 싣지 않는다.
    pub(crate) real: Url,
    /// 키를 가린 주소.
    pub(crate) shown: String,
}

/// 한 출처로 보내는 곳: 기본 주소와 시간 한도를 건 클라이언트.
pub(crate) struct Endpoint {
    /// 출처.
    pub(crate) origin: SourceName,
    /// 기본 주소(규칙을 지난 것).
    pub(crate) base: Url,
    /// HTTP 클라이언트.
    pub(crate) http: Client,
}

impl Endpoint {
    /// 기본 주소 규칙을 보고 클라이언트를 만든다.
    pub(crate) fn new(
        origin: SourceName,
        base: &str,
        timeout: Duration,
    ) -> Result<Self, DataError> {
        Ok(Self {
            origin,
            base: parse_base(base, origin)?,
            http: build_client(origin, timeout)?,
        })
    }

    /// GET을 보내고 2xx 본문을 글로 돌려준다([`get_text`]).
    pub(crate) async fn get(&self, target: &Target) -> Result<String, DataError> {
        get_text(&self.http, self.origin, target).await
    }
}

/// 기본 주소를 읽는다. `https`이거나 루프백(`127.0.0.0/8` · `::1` · `localhost`)의 `http`만 받는다.
pub(crate) fn parse_base(base: &str, origin: SourceName) -> Result<Url, DataError> {
    let url = Url::parse(base.trim_end_matches('/'))
        .map_err(|_| DataError::InsecureBaseUrl { origin })?;
    if is_allowed_scheme(&url) {
        Ok(url)
    } else {
        Err(DataError::InsecureBaseUrl { origin })
    }
}

fn is_allowed_scheme(url: &Url) -> bool {
    match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => url.host_str().is_some_and(is_loopback_host),
        _ => false,
    }
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

/// 기본 주소 뒤에 경로 조각을 붙인다(조각마다 퍼센트 인코딩).
pub(crate) fn with_segments(base: &Url, segments: &[&str]) -> Url {
    let mut url = base.clone();
    if let Ok(mut path) = url.path_segments_mut() {
        path.pop_if_empty();
        path.extend(segments);
    }
    url
}

/// 쿼리에 키가 실리는 주소(DART · FRED). `key_name` 자리는 보여 줄 주소에서 [`MASK`]다.
pub(crate) fn query_target(
    base: &Url,
    segments: &[&str],
    key_name: &str,
    key: &str,
    params: &[(&str, &str)],
) -> Target {
    let build = |value: &str| {
        let mut url = with_segments(base, segments);
        {
            let mut query = url.query_pairs_mut();
            query.append_pair(key_name, value);
            for (name, param) in params {
                query.append_pair(name, param);
            }
        }
        url
    };
    Target {
        real: build(key),
        shown: build(MASK).to_string(),
    }
}

/// 시간 한도를 건 HTTP 클라이언트. 리다이렉트는 따라가지 않는다(키가 다른 곳으로 가지 않게).
pub(crate) fn build_client(origin: SourceName, timeout: Duration) -> Result<Client, DataError> {
    Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| DataError::Unreachable {
            origin,
            url: String::new(),
            detail: "HTTP 클라이언트를 만들 수 없습니다".to_owned(),
        })
}

/// GET을 보내고 2xx 본문을 글로 돌려준다.
///
/// - 시간 한도 → [`DataError::Timeout`], 연결 실패 · 5xx → [`DataError::Unreachable`]
/// - 429 → [`DataError::RateLimited`], 그 밖의 비정상 상태 → [`DataError::Rejected`](HTTP 상태 숫자)
pub(crate) async fn get_text(
    http: &Client,
    origin: SourceName,
    target: &Target,
) -> Result<String, DataError> {
    let mut response = http
        .get(target.real.clone())
        .send()
        .await
        .map_err(|err| transport_error(origin, target, &err))?;
    let status = response.status();
    if !status.is_success() {
        return Err(status_error(origin, target, status));
    }
    let mut bytes: Vec<u8> = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| transport_error(origin, target, &err))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_BODY_BYTES {
            return Err(format_error(origin, "응답이 너무 큽니다"));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| format_error(origin, "응답이 UTF-8 글이 아닙니다"))
}

fn transport_error(origin: SourceName, target: &Target, err: &reqwest::Error) -> DataError {
    if err.is_timeout() {
        return DataError::Timeout {
            origin,
            url: target.shown.clone(),
        };
    }
    let detail = if err.is_connect() {
        "연결 실패"
    } else if err.is_body() || err.is_decode() {
        "응답을 받는 중 끊김"
    } else {
        "요청 실패"
    };
    unreachable_error(origin, target, detail)
}

fn status_error(origin: SourceName, target: &Target, status: StatusCode) -> DataError {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return DataError::RateLimited { origin };
    }
    if status.is_server_error() {
        return unreachable_error(origin, target, &format!("HTTP {}", status.as_u16()));
    }
    DataError::Rejected {
        origin,
        code: status.as_u16().to_string(),
    }
}

/// 닿지 않음. 주소는 키를 가린 것만 싣는다.
pub(crate) fn unreachable_error(origin: SourceName, target: &Target, detail: &str) -> DataError {
    DataError::Unreachable {
        origin,
        url: target.shown.clone(),
        detail: detail.to_owned(),
    }
}

/// 형식 오류. 본문은 싣지 않는다.
pub(crate) fn format_error(origin: SourceName, detail: &str) -> DataError {
    DataError::Format {
        origin,
        detail: detail.to_owned(),
    }
}

/// 요청 오류. 받은 글자는 싣지 않는다.
pub(crate) fn invalid(detail: &str) -> DataError {
    DataError::InvalidRequest {
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 루프백만_http를_받는다() {
        for ok in [
            "https://example.test",
            "http://127.0.0.1:8080",
            "http://127.1.2.3",
            "http://localhost:1",
            "http://LOCALHOST",
            "http://[::1]:1",
        ] {
            assert!(parse_base(ok, SourceName::Dart).is_ok(), "{ok}");
        }
        for bad in [
            "http://192.0.2.1",
            "http://example.test",
            "ftp://127.0.0.1",
            "http://localhost.example.test",
            "",
            "아무글",
        ] {
            assert_eq!(
                parse_base(bad, SourceName::Ecos).unwrap_err(),
                DataError::InsecureBaseUrl {
                    origin: SourceName::Ecos
                },
                "{bad}"
            );
        }
    }

    #[test]
    fn 보여_줄_주소에는_키가_없다() {
        let base = parse_base("http://127.0.0.1:9/", SourceName::Fred).unwrap();
        let target = query_target(
            &base,
            &["fred", "series", "observations"],
            "api_key",
            "secret-value-1234",
            &[("series_id", "DGS10")],
        );
        assert!(target.real.as_str().contains("secret-value-1234"));
        assert!(!target.shown.contains("secret-value-1234"));
        assert_eq!(
            target.shown,
            "http://127.0.0.1:9/fred/series/observations?api_key=***&series_id=DGS10"
        );
    }
}
