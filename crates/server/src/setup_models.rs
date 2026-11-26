//! (57차, R18-4 · C32 · V78) 설정 화면에서 담당별 AI 모델 고르기([`SETUP_MODELS_PATH`]).
//!
//! 원본 동작 근거: 원본에 없음(우리 설계서 R18-4). 저장 방식은 하루 AI 상한(`desk/limit.rs`) · 알림음(`setup_sound`)과
//! 같다 — 보관 폴더의 설정 파일, 저장소 판을 올리지 않는다.
//!
//! # 읽기 (`GET` [`SETUP_MODELS_PATH`], 세션, 원격 세션도 읽는다, 200)
//!
//! ```json
//! {"roles": [{"agent": "scout", "name": "뉴스 공시 시세 담당", "model": "claude-sonnet-5-5", "default": "claude-sonnet-5-5"}, …],
//!  "choices": [{"id": "claude-sonnet-5-5", "label": "Claude Sonnet 5.5", "note": "기본 · 빠름과 깊이의 균형"}],
//!  "editable": true}
//! ```
//!
//! - `roles`는 `pi_agent::Role::ALL` 차례의 여섯 줄이다. `model`은 지금 쓰는 모델(저장한 값, 없으면 `default`),
//!   `default`는 조립의 모델(`TeamWiring::model` — 팀을 잇지 않은 서버면 [`crate::team::DEFAULT_TEAM_MODEL`]).
//! - `choices`는 [`MODEL_CHOICES`] 그대로다. 목록 밖의 이름은 화면의 "직접 입력"으로 고른다.
//! - `editable`은 이 컴퓨터에서 온 요청이면 참(`pi_gate::local_only`와 같은 판단).
//!
//! # 바꾸기 (`POST` [`SETUP_MODELS_PATH`], 세션 · 같은 출처 · **이 컴퓨터에서만** — 원격 세션은 403)
//!
//! 몸통 `{"roles": [{"agent", "model"}, …]}` — 여섯 담당을 한 번씩 모두(통째로 바꾼다). 줄의 다른 칸(`name` ·
//! `default`)과 맨 위의 다른 칸은 보지 않는다. 답은 200 읽기와 같은 모양(저장한 뒤의 값)이다.
//!
//! - 모델 이름 규칙([`valid_model`]): 1~[`MODEL_NAME_MAX`]자, 영문 소문자 · 숫자 · `-` · `.`만, 첫 글자는 영문 소문자나
//!   숫자(`-`로 시작하면 CLI 방식에서 `--model` 뒤의 인자로 읽힐 수 있어 받지 않는다).
//! - 모르는 담당 · 빠진 담당 · 겹친 담당 · 규칙에 맞지 않는 이름 · 글자가 아닌 값 · JSON 아님이면 400
//!   `{"error": "invalid_input"}`이고 아무것도 저장하지 않는다.
//! - 기본값과 같은 담당은 저장하지 않는다(파일에는 기본과 다른 담당만 남는다).
//!
//! # 언제 쓰이나
//!
//! 다시 켜지 않아도 **다음 AI 턴부터** 쓰인다: 담당 턴 하나(하루 판단 · 깨우기 · 복기 · 결정 메모 · 사무실 채팅)는 그
//! 턴을 시작할 때, 논의는 논의를 시작할 때 지금 값을 읽는다(`Crew::role` · `Crew::team_now`). 논의가 도는 중에 바꾸면
//! 그 논의는 시작할 때의 모델로 끝나고 다음 논의부터 새 모델이다.
//!
//! # 저장 · 깨진 파일
//!
//! 보관 폴더의 [`TEAM_MODELS_FILE`] `{"models": {담당 id: 모델}}`에 원자적으로(임시 파일 → 이름 바꾸기) 쓴다. 파일이
//! 깨졌으면(JSON 아님 · 모양이 다름 · 모르는 담당 · 규칙에 맞지 않는 이름) 기본값으로 돌고 경고를 남긴다 — 팀은
//! 멈추지 않는다. 응답 · 파일에 키 값은 없다.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use pi_agent::Role;
use pi_gate::{FORWARDING_HEADERS, Peer};
use serde_json::{Value, json};

use crate::SetupRefusal;
use crate::desk::Desk;
use crate::setup_http::{body_of, registered, reply};
use crate::team::{DEFAULT_TEAM_MODEL, role_name};

/// 담당 모델을 읽고(`GET`, 세션) 통째로 바꾸는(`POST`, 이 컴퓨터에서만) 경로.
pub const SETUP_MODELS_PATH: &str = "/api/setup/models";

/// 고른 담당 모델을 두는 파일 이름(보관 폴더 안). 비밀이 아니다. 모양 `{"models": {담당 id: 모델}}`.
pub const TEAM_MODELS_FILE: &str = "team-models.json";

/// 모델 이름의 최대 글자 수.
pub const MODEL_NAME_MAX: usize = 64;

/// 고를 수 있는 모델(`id` · `label` · `note`). 이 저장소가 지금 쓰는 모델 이름(조립의 기본값)만 둔다 — 근거 없는
/// 이름을 지어내지 않는다. 그 밖의 이름은 화면의 "직접 입력"으로 고르고 글자 규칙([`valid_model`])만 본다.
pub const MODEL_CHOICES: [(&str, &str, &str); 1] = [(
    DEFAULT_TEAM_MODEL,
    "Claude Sonnet 5.5",
    "기본 · 빠름과 깊이의 균형",
)];

/// `price-inha setup ai` 구역 끝의 안내 한 줄(마법사에서 고르게 하지는 않는다).
pub const SETUP_MODELS_HINT: &str = "담당 모델은 설정 화면에서 바꿀 수 있습니다";

/// 요청 · 응답 · 설정 파일의 칸 이름.
const ROLES: &str = "roles";
const MODELS: &str = "models";

/// 모델 이름 규칙: 1~[`MODEL_NAME_MAX`]자, 영문 소문자 · 숫자 · `-` · `.`, 첫 글자는 영문 소문자나 숫자.
pub(crate) fn valid_model(name: &str) -> bool {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.';
    name.len() <= MODEL_NAME_MAX
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name.chars().all(allowed)
}

/// 고른 담당 모델: 보관 폴더 · 조립의 기본값 · 저장한 값(기본과 다른 담당만).
pub(crate) struct TeamModels {
    home: PathBuf,
    /// 팀 조립의 모델(켤 때 정한 기본값). 팀을 잇지 않았으면 `None`([`DEFAULT_TEAM_MODEL`]).
    launch: Mutex<Option<String>>,
    saved: Mutex<BTreeMap<String, String>>,
}

impl TeamModels {
    /// 보관 폴더에 저장한 값을 읽는다. 없으면 비어 있고, 깨졌으면 비우고 경고를 남긴다.
    pub(crate) fn load(home: &Path) -> TeamModels {
        let saved = match std::fs::read(home.join(TEAM_MODELS_FILE)) {
            Ok(bytes) => parse_file(&bytes).unwrap_or_else(|| {
                tracing::warn!("저장한 담당 모델 설정을 읽을 수 없어 기본 모델로 봅니다");
                BTreeMap::new()
            }),
            Err(_) => BTreeMap::new(),
        };
        TeamModels {
            home: home.to_path_buf(),
            launch: Mutex::new(None),
            saved: Mutex::new(saved),
        }
    }

    /// 팀 조립의 모델(기본값)을 적는다.
    pub(crate) fn set_default(&self, model: &str) {
        *self.launch.lock().unwrap_or_else(PoisonError::into_inner) = Some(model.to_string());
    }

    fn default_model(&self) -> String {
        self.launch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| DEFAULT_TEAM_MODEL.to_string())
    }

    /// 그 담당(id)에게 저장한 모델. 없으면 `None`(기본값을 쓴다).
    pub(crate) fn saved_for(&self, agent: &str) -> Option<String> {
        self.saved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(agent)
            .cloned()
    }

    /// 읽기 답(모듈 문서의 모양).
    fn view(&self, editable: bool) -> Value {
        let default = self.default_model();
        let saved = self
            .saved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let roles: Vec<Value> = Role::ALL
            .iter()
            .map(|role| {
                let model = saved.get(role.id()).unwrap_or(&default);
                json!({ "agent": role.id(), "name": role_name(*role), "model": model, "default": default })
            })
            .collect();
        let choices: Vec<Value> = MODEL_CHOICES
            .iter()
            .map(|(id, label, note)| json!({ "id": id, "label": label, "note": note }))
            .collect();
        json!({ ROLES: roles, "choices": choices, "editable": editable })
    }

    /// 몸통대로 통째로 바꾸고 저장한다. 잘못이면 그대로 두고 [`SetupRefusal::InvalidInput`].
    fn change(&self, body: &[u8]) -> Result<(), SetupRefusal> {
        let picked = parse_request(body)?;
        let default = self.default_model();
        let kept: BTreeMap<String, String> = picked
            .into_iter()
            .filter(|(_, model)| *model != default)
            .collect();
        let mut saved = self.saved.lock().unwrap_or_else(PoisonError::into_inner);
        save(&self.home, &kept)?;
        tracing::info!(changed = kept.len(), "담당 모델을 바꿨습니다");
        *saved = kept;
        Ok(())
    }
}

/// 담당 id → 모델 표가 여섯 담당을 한 번씩 담고 이름이 모두 규칙에 맞는가.
fn complete(rows: &[(String, String)]) -> bool {
    rows.len() == Role::ALL.len()
        && Role::ALL
            .iter()
            .all(|role| rows.iter().filter(|(agent, _)| agent == role.id()).count() == 1)
        && rows.iter().all(|(_, model)| valid_model(model))
}

/// 바꾸기 몸통: `{"roles": [{"agent": 글자, "model": 글자}, …]}`(여섯 담당 한 번씩).
fn parse_request(body: &[u8]) -> Result<BTreeMap<String, String>, SetupRefusal> {
    let value: Value = serde_json::from_slice(body).map_err(|_| SetupRefusal::InvalidInput)?;
    let rows = value[ROLES].as_array().ok_or(SetupRefusal::InvalidInput)?;
    let pairs: Option<Vec<(String, String)>> = rows
        .iter()
        .map(|row| {
            let agent = row["agent"].as_str()?;
            let model = row["model"].as_str()?;
            Some((agent.to_string(), model.to_string()))
        })
        .collect();
    let pairs = pairs.ok_or(SetupRefusal::InvalidInput)?;
    if !complete(&pairs) {
        return Err(SetupRefusal::InvalidInput);
    }
    Ok(pairs.into_iter().collect())
}

/// 설정 파일: `{"models": {담당 id: 모델}}`(기본과 다른 담당만). 모양이 다르거나 모르는 담당 · 틀린 이름이면 `None`.
fn parse_file(bytes: &[u8]) -> Option<BTreeMap<String, String>> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    let models = value[MODELS].as_object()?;
    let mut out = BTreeMap::new();
    for (agent, model) in models {
        let model = model.as_str().filter(|model| valid_model(model))?;
        Role::from_id(agent)?;
        out.insert(agent.clone(), model.to_string());
    }
    Some(out)
}

/// 값을 파일에 남긴다(임시 파일에 쓰고 옮긴다).
fn save(home: &Path, models: &BTreeMap<String, String>) -> Result<(), SetupRefusal> {
    let temp = home.join(format!("{TEAM_MODELS_FILE}.tmp"));
    let body = json!({ MODELS: models }).to_string();
    let written = std::fs::write(&temp, body)
        .and_then(|()| std::fs::rename(&temp, home.join(TEAM_MODELS_FILE)));
    written.map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        tracing::warn!(%error, "담당 모델 설정을 저장하지 못했습니다");
        SetupRefusal::SaveFailed
    })
}

/// 이 요청이 그 컴퓨터에서 왔는가: `pi_gate::local_only`와 같은 판단(관문이 넣은 상대 주소가 루프백이고 전달
/// 헤더가 하나도 없다). 상대 주소를 모르면 아니다.
pub(crate) fn from_this_computer(peer: Option<&Peer>, headers: &HeaderMap) -> bool {
    peer.is_some_and(|Peer(ip)| ip.to_canonical().is_loopback())
        && FORWARDING_HEADERS
            .iter()
            .all(|name| !headers.contains_key(*name))
}

/// 세션 경로(`GateBuilder::protected_routes`)에 끼운다. 바꾸기는 그 안에서 `pi_gate::local_only`로 닫는다.
pub(crate) fn routes(desk: Arc<Desk>) -> Router {
    let change = Router::new()
        .route(SETUP_MODELS_PATH, post(write))
        .with_state(Arc::clone(&desk));
    Router::new()
        .route(SETUP_MODELS_PATH, get(read))
        .with_state(desk)
        .merge(pi_gate::local_only(change))
}

async fn read(
    State(desk): State<Arc<Desk>>,
    peer: Option<axum::Extension<Peer>>,
    headers: HeaderMap,
) -> Response {
    let editable = from_this_computer(peer.as_deref(), &headers);
    reply(StatusCode::OK, desk.team_models.view(editable))
}

async fn write(State(desk): State<Arc<Desk>>, request: Request) -> Response {
    let outcome = match body_of(request).await {
        Ok(body) => {
            let changing = Arc::clone(&desk);
            tokio::task::spawn_blocking(move || changing.team_models.change(&body))
                .await
                .unwrap_or(Err(SetupRefusal::SaveFailed))
        }
        Err(why) => Err(why),
    };
    match outcome {
        Ok(()) => reply(StatusCode::OK, desk.team_models.view(true)),
        Err(why) => registered(Err(why)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 모델_이름은_소문자_숫자_빼기_점만_받고_첫_글자는_빼기가_아니다() {
        for good in ["claude-sonnet-5-5", "a", "zz-model.1", "9x"] {
            assert!(valid_model(good), "{good}");
        }
        let long = "a".repeat(MODEL_NAME_MAX + 1);
        for bad in [
            "",
            "Claude",
            "a b",
            "a/b",
            "-x",
            ".x",
            "모델",
            long.as_str(),
        ] {
            assert!(!valid_model(bad), "{bad}");
        }
        assert!(valid_model(&"a".repeat(MODEL_NAME_MAX)));
    }

    #[test]
    fn 깨진_설정_파일은_읽지_않는다() {
        assert!(parse_file(b"{").is_none());
        assert!(parse_file(br#"{"models": {"nobody": "a"}}"#).is_none());
        assert!(parse_file(br#"{"models": {"scout": "A"}}"#).is_none());
        assert!(parse_file(br#"{"models": []}"#).is_none());
        let fine = parse_file(br#"{"models": {"scout": "zz-a"}}"#).unwrap_or_default();
        assert_eq!(fine.get("scout").map(String::as_str), Some("zz-a"));
    }
}
