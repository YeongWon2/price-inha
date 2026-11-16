//! 화면 한 장. 프로그램에 넣어 둔 `web/index.html`을 내준다(바깥에서 아무것도 불러오지 않는다).

use axum::Router;
use axum::http::HeaderMap;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, REFERRER_POLICY,
    X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

/// 화면 파일 그대로.
const INDEX_HTML: &str = include_str!("../web/index.html");

/// 다른 기기의 키 입력 화면 파일 그대로.
const KEYS_HTML: &str = include_str!("../web/keys.html");

/// 공개 경로: `GET /`.
pub(crate) fn routes() -> Router {
    Router::new().route("/", get(index))
}

/// 화면이 할 수 있는 일을 좁힌다: 파일 안의 스크립트 · 모양만 쓰고, 연결은 같은 출처로만 한다.
///
/// `connect-src`의 `'self'`가 웹소켓을 포함하지 않는 브라우저가 있어 `ws://<Host>`를 따로 적는다.
/// `Host`는 접속 관문이 이미 허용 목록과 포트를 확인한 값이다.
fn content_security_policy(host: Option<&str>) -> String {
    let socket = host
        .filter(|host| {
            !host.is_empty()
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
        })
        .map(|host| format!(" ws://{host}"))
        .unwrap_or_default();
    format!(
        "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
         connect-src 'self'{socket}; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
    )
}

/// 세션 경로: 다른 기기의 키 입력 화면이 다는 자리.
fn keys_paths() -> [&'static str; 1] {
    [crate::KEYS_PAGE_PATH]
}

/// 세션 경로(`GateBuilder::protected_routes`): 다른 기기의 키 입력 화면(`GET` [`crate::KEYS_PAGE_PATH`]).
/// 원격 기기는 TOTP 로그인으로 세션을 받은 뒤에만 연다. 화면은 [`crate::REMOTE_KEYS_PATH`]로 보낸다.
pub(crate) fn keys_routes() -> Router {
    keys_paths()
        .into_iter()
        .fold(Router::new(), |router, path| router.route(path, get(keys)))
}

async fn keys(headers: HeaderMap) -> Response {
    html(&headers, KEYS_HTML)
}

async fn index(headers: HeaderMap) -> Response {
    html(&headers, INDEX_HTML)
}

/// 화면 파일 하나를 정책 머리와 함께 내준다.
fn html(headers: &HeaderMap, body: &'static str) -> Response {
    let host = headers.get(HOST).and_then(|value| value.to_str().ok());
    (
        [
            (CONTENT_TYPE, "text/html; charset=utf-8".to_owned()),
            (CONTENT_SECURITY_POLICY, content_security_policy(host)),
            (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (X_FRAME_OPTIONS, "DENY".to_owned()),
            (REFERRER_POLICY, "no-referrer".to_owned()),
            (CACHE_CONTROL, "no-store".to_owned()),
        ],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 화면_정책은_같은_출처의_연결만_연다() {
        let policy = content_security_policy(Some("127.0.0.1:7878"));
        assert!(policy.contains("default-src 'none'"));
        assert!(policy.contains("connect-src 'self' ws://127.0.0.1:7878;"));
        let odd = content_security_policy(Some("evil.example; script-src *"));
        assert!(odd.contains("connect-src 'self';"), "{odd}");
        assert!(!odd.contains("evil"));
    }

    #[test]
    fn 키_입력_화면은_원격_등록_경로로만_보내고_키_값을_다시_채우지_않는다() {
        assert!(
            KEYS_HTML.contains(crate::REMOTE_KEYS_PATH),
            "원격 등록 경로로 보낸다"
        );
        assert!(
            !KEYS_HTML.contains("/api/setup/keys"),
            "그 컴퓨터 전용 경로는 쓰지 않는다"
        );
        let fields = KEYS_HTML.matches("type=\"password\"").count();
        assert!(fields >= 3, "키 칸은 가린 칸이다: {fields}");
        assert_eq!(
            fields,
            KEYS_HTML.matches("autocomplete=\"new-password\"").count(),
            "브라우저가 저장을 권하지 않는다"
        );
        assert!(!KEYS_HTML.contains("localStorage") && !KEYS_HTML.contains("sessionStorage"));
        for banned in [
            "http://",
            "https://",
            "innerHTML",
            "outerHTML",
            "document.write",
            "eval(",
        ] {
            assert!(!KEYS_HTML.contains(banned), "{banned}");
        }
        // 등록한 뒤 칸을 비우기만 한다(값을 받아 채우는 곳이 없다).
        for line in KEYS_HTML.lines().filter(|l| l.contains(".value =")) {
            assert!(line.contains(".value = \"\""), "칸을 비우기만 한다: {line}");
        }
    }

    #[tokio::test]
    async fn 키_입력_화면은_세션_경로로_내준다() {
        let response = keys(HeaderMap::new()).await;
        let kind = response.headers().get(CONTENT_TYPE).cloned();
        assert_eq!(
            kind.as_ref().and_then(|v| v.to_str().ok()),
            Some("text/html; charset=utf-8")
        );
        let paths: Vec<&str> = keys_paths().to_vec();
        assert_eq!(paths, vec![crate::KEYS_PAGE_PATH]);
    }

    #[test]
    fn 화면_파일은_바깥_주소를_부르지_않고_innerhtml을_쓰지_않는다() {
        for banned in [
            "http://",
            "https://",
            "innerHTML",
            "outerHTML",
            "document.write",
            "eval(",
        ] {
            assert!(!INDEX_HTML.contains(banned), "{banned}");
        }
    }
}
