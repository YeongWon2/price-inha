//! 웹 자산 경로(공개): 사무실 화면은 `/office/`, 폰 화면은 `/phone/`, 그 화면들이 부르는 묶음은 `/assets/` 아래.
//! 모두 화면 빌드 결과(`web/dist`, server 11차)에서 같은 경로의 파일을 내준다(`crate::page::serve`).

use axum::Router;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::get;

use crate::page::{IMMUTABLE, NO_CACHE, serve};

async fn office_index(headers: HeaderMap) -> Response {
    serve(&headers, "office/index.html", NO_CACHE)
}

async fn office_asset(headers: HeaderMap, Path(path): Path<String>) -> Response {
    serve(&headers, &format!("office/{path}"), NO_CACHE)
}

async fn phone_index(headers: HeaderMap) -> Response {
    serve(&headers, "phone/index.html", NO_CACHE)
}

async fn phone_asset(headers: HeaderMap, Path(path): Path<String>) -> Response {
    serve(&headers, &format!("phone/{path}"), NO_CACHE)
}

/// 내용 해시가 이름에 붙은 묶음(Vite가 만든 `assets/<이름>-<해시>.js|css`).
async fn bundle(headers: HeaderMap, Path(path): Path<String>) -> Response {
    serve(&headers, &format!("assets/{path}"), IMMUTABLE)
}

/// 공개 경로(`GateBuilder::public_routes`)에 끼운다.
pub(crate) fn routes() -> Router {
    Router::new()
        .route("/office/", get(office_index))
        .route("/office/{*path}", get(office_asset))
        .route("/phone/", get(phone_index))
        .route("/phone/{*path}", get(phone_asset))
        .route("/assets/{*path}", get(bundle))
}
