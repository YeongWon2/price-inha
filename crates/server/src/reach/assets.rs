//! 웹 자산 경로(공개): 사무실 화면은 `/office/`, 폰 화면은 `/phone/` 아래.

use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

/// 파일 끝으로 정한 `Content-Type`.
fn office_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "css" => "text/css; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "md" | "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn file(content_type: &'static str, body: &'static [u8]) -> Response {
    (
        [
            (CONTENT_TYPE, content_type),
            (X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (X_FRAME_OPTIONS, "DENY"),
            (CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response()
}

fn office_file(path: &str) -> Response {
    match pi_office::web_assets().iter().find(|a| a.path == path) {
        Some(asset) => file(office_type(asset.path), asset.body),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn phone_file(path: &str) -> Response {
    match pi_notify::phone_asset(path) {
        Some(asset) => file(asset.content_type, asset.body),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn office_index() -> Response {
    office_file(pi_office::INDEX_PATH)
}

async fn office_asset(Path(path): Path<String>) -> Response {
    office_file(&path)
}

async fn phone_index() -> Response {
    phone_file("index.html")
}

async fn phone_asset(Path(path): Path<String>) -> Response {
    phone_file(&path)
}

/// 공개 경로(`GateBuilder::public_routes`)에 끼운다.
pub(crate) fn routes() -> Router {
    Router::new()
        .route("/office/", get(office_index))
        .route("/office/{*path}", get(office_asset))
        .route("/phone/", get(phone_index))
        .route("/phone/{*path}", get(phone_asset))
}
