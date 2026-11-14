//! 요청 하나를 보내고 실패를 구분한다. 어떤 오류에도 응답 본문 · 자격 · 토큰을 담지 않는다.

use std::time::Duration;

use reqwest::header::RETRY_AFTER;
use reqwest::{Client, RequestBuilder, Url};
use serde::Deserialize;

use crate::{Credentials, MarketError};

use super::TossConfig;

/// 토큰 발급 경로.
const TOKEN_PATH: &str = "/oauth2/token";
/// 오류 코드가 없거나 읽을 수 없을 때 쓰는 글자.
const UNKNOWN_CODE: &str = "unknown";
/// 오류 코드로 받아들이는 가장 긴 길이.
const MAX_CODE_LEN: usize = 64;

/// 설정대로 HTTP 클라이언트를 만든다. 다른 주소로 넘기는 응답(3xx)은 따라가지 않는다
/// (토큰을 실은 요청이 엉뚱한 곳으로 가지 않게).
pub(super) fn build_client(config: &TossConfig) -> Result<Client, MarketError> {
    Client::builder()
        .timeout(config.timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| MarketError::Connection {
            detail: "HTTP 클라이언트를 만들지 못했습니다".to_string(),
        })
}

/// 기본 주소에 경로와 쿼리를 붙인다. 쿼리 값은 여기서 인코딩한다.
pub(super) fn endpoint(
    config: &TossConfig,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Url, MarketError> {
    let base = config.base_url.trim_end_matches('/');
    let mut url = Url::parse(&format!("{base}{path}")).map_err(|_| MarketError::Connection {
        detail: "기본 주소가 올바르지 않습니다".to_string(),
    })?;
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    Ok(url)
}

/// 발급받은 토큰.
pub(super) struct IssuedToken {
    /// 토큰 글자.
    pub(super) value: String,
    /// 유효 시간(초).
    pub(super) expires_in: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

/// `POST /oauth2/token`(form). 자격은 본문에만 담는다.
pub(super) async fn issue_token(
    http: &Client,
    config: &TossConfig,
    credentials: &Credentials,
) -> Result<IssuedToken, MarketError> {
    let url = endpoint(config, TOKEN_PATH, &[])?;
    let form = [
        ("grant_type", "client_credentials"),
        ("client_id", credentials.client_id()),
        ("client_secret", credentials.client_secret()),
    ];
    let body = send(http.post(url).form(&form)).await?;
    let parsed: TokenResponse = serde_json::from_str(&body).map_err(|_| unexpected("토큰"))?;
    if parsed.access_token.is_empty() {
        return Err(unexpected("토큰"));
    }
    Ok(IssuedToken {
        value: parsed.access_token,
        expires_in: parsed.expires_in,
    })
}

/// 응답 모양이 명세와 다르다는 오류. 본문이나 파서의 말(받은 값이 섞일 수 있다)은 담지 않는다.
pub(super) fn unexpected(what: &str) -> MarketError {
    MarketError::UnexpectedResponse {
        detail: format!("{what} 응답 모양이 명세와 다릅니다"),
    }
}

/// 요청을 보내고 2xx면 본문을, 아니면 구분한 오류를 돌려준다.
pub(super) async fn send(request: RequestBuilder) -> Result<String, MarketError> {
    let response = request.send().await.map_err(connection_error)?;
    let status = response.status();
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
        .map(Duration::from_secs);
    let body = response.text().await.map_err(connection_error)?;
    if status.is_success() {
        Ok(body)
    } else {
        Err(classify(status.as_u16(), retry_after, &body))
    }
}

/// 연결 실패의 종류만 남긴다(reqwest 오류 글자는 주소를 담을 수 있어 쓰지 않는다).
fn connection_error(error: reqwest::Error) -> MarketError {
    let detail = if error.is_timeout() {
        "시간 제한을 넘었습니다"
    } else if error.is_connect() {
        "연결하지 못했습니다"
    } else {
        "요청을 주고받는 중에 끊겼습니다"
    };
    MarketError::Connection {
        detail: detail.to_string(),
    }
}

/// 2xx가 아닌 응답을 오류로 구분한다.
fn classify(status: u16, retry_after: Option<Duration>, body: &str) -> MarketError {
    let code = error_code(body);
    match status {
        429 => MarketError::RateLimited { retry_after },
        401 => MarketError::AuthFailed {
            code: code.unwrap_or_else(|| UNKNOWN_CODE.to_string()),
        },
        403 if matches!(code.as_deref(), Some("ip-not-allowed" | "access_denied")) => {
            MarketError::IpNotAllowed
        }
        500..=599 => MarketError::Unavailable { status, code },
        400..=499 => MarketError::Rejected {
            status,
            code: code.unwrap_or_else(|| UNKNOWN_CODE.to_string()),
        },
        _ => MarketError::UnexpectedResponse {
            detail: format!("예상하지 못한 HTTP 상태 {status}"),
        },
    }
}

/// 오류 본문에서 코드만 꺼낸다. 공통 모양 `{"error":{"code"}}`와 토큰 발급의 OAuth 모양
/// `{"error":"..."}`를 둘 다 읽는다. 코드처럼 생기지 않은 글자는 버린다(본문에 섞여 온
/// 다른 값이 오류로 새어 나가지 않게).
fn error_code(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let error = value.get("error")?;
    let code = match error {
        serde_json::Value::String(code) => code.as_str(),
        other => other.get("code")?.as_str()?,
    };
    let looks_like_code = !code.is_empty()
        && code.len() <= MAX_CODE_LEN
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    looks_like_code.then(|| code.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 오류_코드는_두_모양을_모두_읽는다() {
        assert_eq!(
            error_code(r#"{"error":{"requestId":"r","code":"token-revoked","message":"m"}}"#),
            Some("token-revoked".to_string())
        );
        assert_eq!(
            error_code(r#"{"error":"invalid_client","error_description":"d"}"#),
            Some("invalid_client".to_string())
        );
        assert_eq!(error_code("<html>Service Unavailable</html>"), None);
        assert_eq!(error_code(r#"{"error":{"message":"m"}}"#), None);
    }

    #[test]
    fn 코드처럼_생기지_않은_글자는_오류에_담지_않는다() {
        assert_eq!(error_code(r#"{"error":"client_secret=abc def"}"#), None);
        assert_eq!(error_code(r#"{"error":{"code":""}}"#), None);
        let long = "a".repeat(MAX_CODE_LEN + 1);
        assert_eq!(error_code(&format!(r#"{{"error":"{long}"}}"#)), None);
    }

    #[test]
    fn 상태와_코드로_오류를_구분한다() {
        let api = |code: &str| format!(r#"{{"error":{{"code":"{code}"}}}}"#);
        assert_eq!(
            classify(403, None, &api("ip-not-allowed")),
            MarketError::IpNotAllowed
        );
        // 허용 IP가 아닌 403은 그냥 거절이다.
        assert_eq!(
            classify(403, None, &api("forbidden")),
            MarketError::Rejected {
                status: 403,
                code: "forbidden".to_string()
            }
        );
        assert_eq!(
            classify(401, None, "not json"),
            MarketError::AuthFailed {
                code: UNKNOWN_CODE.to_string()
            }
        );
        assert_eq!(
            classify(
                429,
                Some(Duration::from_secs(2)),
                &api("edge-rate-limit-exceeded")
            ),
            MarketError::RateLimited {
                retry_after: Some(Duration::from_secs(2))
            }
        );
        assert!(matches!(
            classify(302, None, ""),
            MarketError::UnexpectedResponse { .. }
        ));
    }

    #[test]
    fn 주소는_기본_주소_끝의_빗금을_겹치지_않고_쿼리를_인코딩한다() {
        let config = TossConfig {
            base_url: "http://127.0.0.1:1/".to_string(),
            ..TossConfig::default()
        };
        let url = endpoint(&config, "/api/v1/prices", &[("symbols", "A,B&c=d")]).unwrap();
        assert_eq!(url.path(), "/api/v1/prices");
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(pairs, vec![("symbols".to_string(), "A,B&c=d".to_string())]);

        let broken = TossConfig {
            base_url: "주소 아님".to_string(),
            ..TossConfig::default()
        };
        assert!(matches!(
            endpoint(&broken, "/oauth2/token", &[]),
            Err(MarketError::Connection { .. })
        ));
    }
}
