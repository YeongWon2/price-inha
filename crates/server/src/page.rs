//! 화면 파일. 화면 빌드 결과(`web/dist`, Vite — server 11차)를 build.rs가 실행 파일에 넣은 표에서 내준다
//! (바깥에서 아무것도 불러오지 않는다). 설정(`GET /`) · 키(`GET` [`crate::KEYS_PAGE_PATH`]) ·
//! 관리 화면 넷(`GET` [`ADMIN_PAGE_PATHS`] — 26차 4번)은 여기서,
//! 사무실 · 폰 · 묶음(`/office/` · `/phone/` · `/assets/`)은 `reach::assets`가 이 표로 내준다.

use axum::Router;
use axum::extract::Path;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, REFERRER_POLICY,
    X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

include!(concat!(env!("OUT_DIR"), "/web_dist.rs"));

/// 설정 화면(dist 안의 이름).
const INDEX_FILE: &str = "index.html";

/// 다른 기기의 키 입력 화면(dist 안의 이름).
const KEYS_FILE: &str = "keys.html";

/// 관리 화면(26차 4번): 계좌 화면 주소(`GET`). 관문은 설정 · 사무실 화면과 같다(그 컴퓨터는 세션 없이,
/// 다른 기기는 원격 세션이 있어야 열린다). dist의 `account/index.html`을 내준다.
pub const ACCOUNT_PAGE_PATH: &str = "/account/";
/// 관리 화면(26차 4번): 템플릿 화면 주소(`GET`). dist의 `templates/index.html`.
pub const TEMPLATES_PAGE_PATH: &str = "/templates/";
/// 관리 화면(26차 4번): 규칙 화면 주소(`GET`). dist의 `rules/index.html`.
pub const RULES_PAGE_PATH: &str = "/rules/";
/// 관리 화면(26차 4번): 히스토리 화면 주소(`GET`). dist의 `history/index.html`.
pub const HISTORY_PAGE_PATH: &str = "/history/";
/// 관리 화면 넷의 주소(계좌 · 템플릿 · 규칙 · 히스토리 차례).
pub const ADMIN_PAGE_PATHS: [&str; 4] = [
    ACCOUNT_PAGE_PATH,
    TEMPLATES_PAGE_PATH,
    RULES_PAGE_PATH,
    HISTORY_PAGE_PATH,
];

/// 캐시: 설정 · 키 화면은 남기지 않는다(지금처럼).
pub(crate) const NO_STORE: &str = "no-store";
/// 캐시: 사무실 · 폰 쪽 파일(HTML · `sw.js` · manifest 포함)은 늘 다시 확인한다(지금처럼).
pub(crate) const NO_CACHE: &str = "no-cache";
/// 캐시: 내용 해시가 이름에 붙은 묶음(`/assets/`)은 오래 둔다(내용이 바뀌면 이름이 바뀐다).
pub(crate) const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// 관리 화면 넷의 주소와 dist 안의 HTML(26차 4번, [`ADMIN_PAGE_PATHS`] 차례).
const ADMIN_PAGES: [(&str, &str); 4] = [
    (ACCOUNT_PAGE_PATH, "account/index.html"),
    (TEMPLATES_PAGE_PATH, "templates/index.html"),
    (RULES_PAGE_PATH, "rules/index.html"),
    (HISTORY_PAGE_PATH, "history/index.html"),
];

/// 공개 경로: `GET /`과 관리 화면 넷(`GET` [`ADMIN_PAGE_PATHS`], 그 폴더 아래 dist 파일 포함 — 사무실 · 폰과 같다).
/// 공개 경로라 관문 규칙이 설정 · 사무실 화면과 같다: 그 컴퓨터는 세션 없이, 다른 기기는 원격 세션이 있어야 열린다.
pub(crate) fn routes() -> Router {
    ADMIN_PAGES
        .into_iter()
        .fold(Router::new().route("/", get(index)), admin_routes)
}

/// 관리 화면 하나: 주소(`/account/`)는 그 HTML을, 그 아래(`/account/<파일>`)는 dist의 같은 경로 파일을 내준다.
fn admin_routes(router: Router, (path, rel): (&'static str, &'static str)) -> Router {
    let dir = path.trim_matches('/');
    router
        .route(
            path,
            get(move |headers: HeaderMap| async move { serve(&headers, rel, NO_STORE) }),
        )
        .route(
            &format!("{path}{{*rest}}"),
            get(
                move |headers: HeaderMap, Path(rest): Path<String>| async move {
                    serve(&headers, &format!("{dir}/{rest}"), NO_CACHE)
                },
            ),
        )
}

/// dist 안의 파일(`/` 구분 상대 경로).
pub(crate) fn dist_file(rel: &str) -> Option<&'static [u8]> {
    DIST_FILES
        .iter()
        .find(|(path, _)| *path == rel)
        .map(|(_, body)| *body)
}

/// 파일 끝으로 정한 `Content-Type`.
fn content_type(rel: &str) -> &'static str {
    match rel.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "webmanifest" => "application/manifest+json",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "glb" => "model/gltf-binary",
        "md" | "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// 화면이 할 수 있는 일을 좁힌다: 같은 출처의 묶음(스크립트 · 모양 · 그림 · manifest · 서비스 워커)만 쓰고,
/// 연결은 같은 출처로만 한다. 빌드한 화면에는 인라인 스크립트 · 모양이 없어 `'unsafe-inline'`을 두지 않는다.
///
/// `connect-src`의 `'self'`가 웹소켓을 포함하지 않는 브라우저가 있어 `ws://<Host>`를 따로 적고,
/// https 뒤(역방향 프록시 · Tailscale serve)에서 화면이 `wss://`로 붙어도 막히지 않게 같은 Host의
/// `wss://<Host>`도 적는다. 바깥 주소는 더하지 않는다.
/// `Host`는 접속 관문이 이미 허용 목록과 포트를 확인한 값이다.
fn content_security_policy(host: Option<&str>) -> String {
    let socket = host
        .filter(|host| {
            !host.is_empty()
                && host
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
        })
        .map(|host| format!(" ws://{host} wss://{host}"))
        .unwrap_or_default();
    format!(
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; \
         manifest-src 'self'; worker-src 'self'; connect-src 'self'{socket}; base-uri 'none'; \
         form-action 'none'; frame-ancestors 'none'"
    )
}

/// dist 안의 파일 하나를 정책 머리와 함께 내준다. 없으면 404.
pub(crate) fn serve(headers: &HeaderMap, rel: &str, cache: &'static str) -> Response {
    let Some(body) = dist_file(rel) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let host = headers.get(HOST).and_then(|value| value.to_str().ok());
    (
        [
            (CONTENT_TYPE, content_type(rel).to_owned()),
            (CONTENT_SECURITY_POLICY, content_security_policy(host)),
            (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (X_FRAME_OPTIONS, "DENY".to_owned()),
            (REFERRER_POLICY, "no-referrer".to_owned()),
            (CACHE_CONTROL, cache.to_owned()),
        ],
        body,
    )
        .into_response()
}

/// 키 입력 화면이 다는 자리.
fn keys_paths() -> [&'static str; 1] {
    [crate::KEYS_PAGE_PATH]
}

/// 공개 경로(`GateBuilder::public_routes`, 기능 H 2번): 키 입력 화면(`GET` [`crate::KEYS_PAGE_PATH`]).
/// 관문 규칙이 설정 화면과 같다: 그 컴퓨터(루프백 · 전달 머리글 없음)는 세션 없이 열리고 화면이 세션을 얻는다.
/// 다른 기기는 관문이 원격 세션을 먼저 보므로 접속 키로 들어온 뒤에만 연다. 화면은 HTML뿐이고 값을 다루는 길
/// ([`crate::REMOTE_KEYS_PATH`] · 등록 여부 읽기)은 세션 경로 그대로다.
pub(crate) fn keys_routes() -> Router {
    keys_paths()
        .into_iter()
        .fold(Router::new(), |router, path| router.route(path, get(keys)))
}

async fn keys(headers: HeaderMap) -> Response {
    serve(&headers, KEYS_FILE, NO_STORE)
}

async fn index(headers: HeaderMap) -> Response {
    serve(&headers, INDEX_FILE, NO_STORE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(rel: &str) -> &'static str {
        std::str::from_utf8(dist_file(rel).unwrap_or_else(|| panic!("dist/{rel}"))).expect("UTF-8")
    }

    /// 쪽 HTML이 부르는 같은 출처 묶음(`/assets/…js`)과 그 묶음이 다시 부르는 묶음의 글.
    fn bundles(html: &str) -> Vec<(&'static str, &'static str)> {
        let mut queue: Vec<String> = html
            .split("src=\"/assets/")
            .skip(1)
            .chain(
                html.split("modulepreload\" crossorigin href=\"/assets/")
                    .skip(1),
            )
            .filter_map(|rest| rest.split('"').next())
            .map(str::to_owned)
            .collect();
        let mut out = Vec::new();
        while let Some(name) = queue.pop() {
            let rel = format!("assets/{name}");
            if out.iter().any(|(seen, _)| *seen == rel.as_str()) {
                continue;
            }
            let (path, _) = DIST_FILES
                .iter()
                .find(|(path, _)| *path == rel)
                .unwrap_or_else(|| panic!("{rel}"));
            let body = text(path);
            for piece in body.split("from \"./").skip(1) {
                queue.extend(piece.split('"').next().map(str::to_owned));
            }
            out.push((*path, body));
        }
        out
    }

    #[test]
    fn 화면_정책은_같은_출처의_연결만_연다() {
        let policy = content_security_policy(Some("127.0.0.1:7878"));
        assert!(policy.contains("default-src 'none'"));
        assert!(policy.contains("connect-src 'self' ws://127.0.0.1:7878 wss://127.0.0.1:7878;"));
        assert!(policy.contains("script-src 'self';"), "{policy}");
        assert!(!policy.contains("unsafe-inline"), "{policy}");
        let odd = content_security_policy(Some("evil.example; script-src *"));
        assert!(odd.contains("connect-src 'self';"), "{odd}");
        assert!(!odd.contains("evil"));
    }

    #[test]
    fn 빌드한_화면에는_인라인_스크립트가_없다() {
        for page in [
            INDEX_FILE,
            KEYS_FILE,
            "office/index.html",
            "phone/index.html",
        ] {
            let html = text(page);
            for piece in html.split("<script").skip(1) {
                let head = piece.split('>').next().unwrap_or_default();
                assert!(
                    head.contains("src=\"/assets/"),
                    "{page}: 인라인 스크립트 <script{head}>"
                );
            }
            assert!(!html.contains("<style"), "{page}: 인라인 모양");
        }
    }

    #[test]
    fn 키_입력_화면은_원격_등록_경로로만_보내고_키_값을_다시_채우지_않는다() {
        let html = text(KEYS_FILE);
        let fields = html.matches("type=\"password\"").count();
        assert!(fields >= 3, "키 칸은 가린 칸이다: {fields}");
        assert_eq!(
            fields,
            html.matches("autocomplete=\"new-password\"").count(),
            "브라우저가 저장을 권하지 않는다"
        );
        let scripts = bundles(html);
        let own = scripts
            .iter()
            .find(|(path, _)| path.starts_with("assets/keys-"))
            .map(|(_, body)| *body)
            .expect("키 화면 묶음");
        assert!(own.contains("API.remoteKeys"), "원격 등록 경로로 보낸다");
        assert!(
            !own.contains("API.setupKeys"),
            "그 컴퓨터 전용 경로는 쓰지 않는다"
        );
        let all: String = scripts.iter().map(|(_, body)| *body).collect();
        assert!(
            all.contains(crate::REMOTE_KEYS_PATH),
            "경로 표에 원격 등록 경로가 있다"
        );
        for banned in [
            "localStorage",
            "sessionStorage",
            "http://",
            "https://",
            "innerHTML",
            "outerHTML",
            "document.write",
            "eval(",
        ] {
            assert!(!all.contains(banned), "{banned}");
        }
        // 등록한 뒤 칸을 비우기만 한다(값을 받아 채우는 곳이 없다).
        for line in all.lines().filter(|l| l.contains(".value =")) {
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
        let html = text(INDEX_FILE);
        let mut all = html.to_owned();
        for (_, body) in bundles(html) {
            all.push_str(body);
        }
        for banned in [
            "http://",
            "https://",
            "innerHTML",
            "outerHTML",
            "document.write",
            "eval(",
        ] {
            assert!(!all.contains(banned), "{banned}");
        }
    }
}
