//! 백업 설정 화면(저장소 이름 · 수신자 공개키, F9 사전 조건)과 되살리기.
//!
//! - 설정은 보관 폴더의 [`BACKUP_SETTINGS_FILE`]에 남는다(비밀이 아니다: 저장소 이름과 age **공개**키뿐,
//!   토큰은 비밀 보관소의 `KeySlot::BackupGithubToken`). 다시 켜면 되읽어 `reach.backup`의 저장소 ·
//!   공개키보다 앞선다(GitHub 주소는 `reach.backup`의 것, 없으면 [`GITHUB_API_BASE`]).
//! - 되살리기([`restore_backup`])는 빈 폴더에 하고, 검증 사본은 그 옆의 작업 폴더에만 두었다가 지운다
//!   (시스템 임시 폴더에 평문을 두지 않는다, `pi_backup::restore_in`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use pi_backup::{Bundle, Manifest, RestoreReport};
use pi_secrets::SecretString;
use pi_store::UnixMillis;
use serde_json::{Value, json};

use super::BackupWiring;
use super::linked::Linked;
use super::office_http::reply;
use crate::AppError;

/// 백업 설정. `GET` → `{"repo", "recipient", "configured"}`(정하지 않았으면 `repo` · `recipient`는 `null`),
/// `POST` 몸통 `{"repo": "소유자/이름", "recipient": "age1…"}` → 200 같은 모양. 틀린 값은 400
/// `{"error": "invalid_input"}`이고 그대로 둔다. 세션 · Origin 확인을 받는다.
pub const SETUP_BACKUP_PATH: &str = "/api/setup/backup";

/// 보관 폴더 안 백업 설정 파일 이름.
pub const BACKUP_SETTINGS_FILE: &str = "backup-settings.json";

/// `reach.backup`이 없을 때 쓰는 GitHub API 주소.
pub const GITHUB_API_BASE: &str = "https://api.github.com";

/// 저장소 이름 `소유자/이름`(영문 · 숫자 · `.` `_` `-`)인가.
fn valid_repo(repo: &str) -> bool {
    let good = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    };
    matches!(repo.split_once('/'), Some((owner, name)) if good(owner) && good(name))
}

/// age 수신자 공개키로 쓸 수 있는가(빈 묶음을 봉해 본다).
fn valid_recipient(recipient: &str) -> bool {
    let empty = Bundle {
        manifest: Manifest {
            version: 1,
            created_at: UnixMillis(0),
            files: Vec::new(),
        },
        files: Vec::new(),
    };
    recipient.starts_with("age1") && pi_backup::seal(&empty, recipient).is_ok()
}

/// 보관 폴더의 설정 파일에서 저장소 · 공개키를 읽는다. 없거나 틀리면 `None`.
pub(crate) fn saved(home: &Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(home.join(BACKUP_SETTINGS_FILE)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let repo = value["repo"].as_str()?.to_string();
    let recipient = value["recipient"].as_str()?.to_string();
    (valid_repo(&repo) && valid_recipient(&recipient)).then_some((repo, recipient))
}

/// 띄울 때의 백업 조립 값: 남긴 설정이 `reach.backup`의 저장소 · 공개키보다 앞선다.
pub(crate) fn effective(home: &Path, wiring: Option<BackupWiring>) -> Option<BackupWiring> {
    match (wiring, saved(home)) {
        (Some(wiring), Some((repo, recipient))) => Some(BackupWiring {
            repo,
            recipient,
            ..wiring
        }),
        (Some(wiring), None) => Some(wiring),
        (None, Some((repo, recipient))) => Some(BackupWiring {
            api_base: GITHUB_API_BASE.to_string(),
            repo,
            recipient,
        }),
        (None, None) => None,
    }
}

/// 설정을 파일에 남긴다.
pub(crate) fn save(home: &Path, repo: &str, recipient: &str) -> Result<(), AppError> {
    let text = json!({"repo": repo, "recipient": recipient}).to_string();
    std::fs::write(home.join(BACKUP_SETTINGS_FILE), text)
        .map_err(|error| AppError::Backup(error.to_string()))
}

fn view(wiring: Option<&BackupWiring>) -> Value {
    json!({
        "repo": wiring.map(|w| w.repo.clone()),
        "recipient": wiring.map(|w| w.recipient.clone()),
        "configured": wiring.is_some(),
    })
}

async fn show(State(linked): State<Arc<Linked>>) -> Response {
    reply(StatusCode::OK, view(linked.backup_wiring().as_ref()))
}

async fn change(State(linked): State<Arc<Linked>>, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let text = |name: &str| body[name].as_str().map(str::trim).unwrap_or_default();
    let (repo, recipient) = (text("repo"), text("recipient"));
    if !valid_repo(repo) || !valid_recipient(recipient) {
        return reply(StatusCode::BAD_REQUEST, json!({"error": "invalid_input"}));
    }
    match linked.set_backup(repo, recipient) {
        Ok(wiring) => reply(StatusCode::OK, view(Some(&wiring))),
        Err(error) => {
            tracing::warn!(%error, "백업 설정을 남기지 못했습니다");
            reply(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "internal"}),
            )
        }
    }
}

/// `GateBuilder::protected_routes`에 끼운다.
pub(crate) fn routes(linked: Arc<Linked>) -> Router {
    Router::new()
        .route(SETUP_BACKUP_PATH, get(show).post(change))
        .with_state(linked)
}

/// `target` 옆의 작업 폴더(`<이름>.restore-work`).
fn work_dir(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "home".to_string());
    target.with_file_name(format!("{name}.restore-work"))
}

/// 암호문을 없거나 빈 폴더 `target`에 되살린다(F9 1a). 팀은 전체 정지가 되고 기다리던 승인은
/// 만료된다(`at` 기준). 검증 사본은 `target` 옆의 작업 폴더에만 두고, 끝나면 그 폴더째 지운다.
/// `identity`는 인자로만 쓰고 파일 · 기록 · 오류 어디에도 남기지 않는다.
pub fn restore_backup(
    encrypted: &[u8],
    identity: &SecretString,
    target: &Path,
    at: UnixMillis,
) -> Result<RestoreReport, AppError> {
    let work = work_dir(target);
    let existed = work.exists();
    let restored = pi_backup::restore_in(encrypted, identity, target, &work, at);
    if !existed
        && work.exists()
        && let Err(error) = std::fs::remove_dir_all(&work)
    {
        tracing::warn!(%error, "되살리기 작업 폴더를 지우지 못했습니다");
    }
    restored.map_err(|error| AppError::Backup(error.to_string()))
}
