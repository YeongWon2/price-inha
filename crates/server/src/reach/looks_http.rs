//! 담당 외형(server 20차 4번): [`OFFICE_LOOKS_PATH`] 읽기 · [`OFFICE_LOOK_PATH`] 바꾸기.
//!
//! 외형 id는 원본 DeskRPG 외형 목록(`office-looks`, 커밋 5d80963)의 id이고 화면은 같은 출처의
//! `/office/deskrpg/characters/office/<id>.glb`를 부른다. 원본은 외형의 정본을 프로필 쪽에 저장한다 — 여기서는
//! 보관 폴더의 [`LOOKS_FILE`]에 남겨 서버를 다시 켜도, 다른 기기에서 열어도 같다(비밀이 아니다).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post};
use pi_office::Agent;
use serde_json::{Value, json};

use super::office_http::reply;
use super::{OFFICE_BAD_INPUT, OFFICE_LOOK_PATH, OFFICE_LOOKS_PATH};

/// 고른 외형을 남기는 파일(보관 폴더 안).
pub(crate) const LOOKS_FILE: &str = "office-looks.json";

/// 원본 외형 id 50개(원본 `public/assets/characters/office/<id>.glb`).
const LOOK_IDS: [&str; 50] = [
    "office-ara",
    "office-bomi",
    "office-chan",
    "office-daeun",
    "office-dami",
    "office-do",
    "office-dohun",
    "office-eun",
    "office-eunsol",
    "office-garam",
    "office-gonu",
    "office-ha",
    "office-haena",
    "office-haram",
    "office-hosu",
    "office-hyejin",
    "office-hyeon",
    "office-hyo",
    "office-ian",
    "office-jaewon",
    "office-jiho",
    "office-jin",
    "office-jiseok",
    "office-jua",
    "office-jun",
    "office-jungwon",
    "office-kyu",
    "office-kyung",
    "office-min",
    "office-mira",
    "office-nari",
    "office-roan",
    "office-rumi",
    "office-sejin",
    "office-seo",
    "office-seok",
    "office-seona",
    "office-sera",
    "office-seul",
    "office-soi",
    "office-suhye",
    "office-sungho",
    "office-tae",
    "office-taemin",
    "office-woojin",
    "office-yena",
    "office-yeon",
    "office-yul",
    "office-yumin",
    "office-yun",
];

/// 담당별 기본 외형(여섯 모두 다르다). 화면 `web/src/office/looks.ts`의 `DEFAULT_LOOKS`와 같다.
fn default_look(agent: Agent) -> &'static str {
    match agent.id() {
        "news" => "office-seo",
        "strategy" => "office-jaewon",
        "risk" => "office-sera",
        "review" => "office-min",
        "trader" => "office-sejin",
        _ => "office-yun",
    }
}

fn known_look(look: &str) -> Option<&'static str> {
    LOOK_IDS.iter().copied().find(|id| *id == look)
}

/// 담당 → 외형. 파일에 없거나 틀린 값은 기본 외형이다.
pub(crate) struct Looks {
    file: PathBuf,
    chosen: Mutex<BTreeMap<&'static str, &'static str>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Looks {
    pub(crate) fn open(home: &Path) -> Self {
        let file = home.join(LOOKS_FILE);
        let saved: Value = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        let chosen = Agent::ALL
            .iter()
            .map(|agent| {
                let look = saved["agents"][agent.id()]
                    .as_str()
                    .and_then(known_look)
                    .unwrap_or_else(|| default_look(*agent));
                (agent.id(), look)
            })
            .collect();
        Self {
            file,
            chosen: Mutex::new(chosen),
        }
    }

    fn view(chosen: &BTreeMap<&'static str, &'static str>) -> Value {
        json!({ "agents": chosen, "available": LOOK_IDS.as_slice() })
    }

    /// 바꾸고 파일에 남긴다. 남기지 못하면 바꾸지 않는다.
    fn choose(&self, agent: Agent, look: &'static str) -> std::io::Result<Value> {
        let mut chosen = lock(&self.chosen);
        let mut next = chosen.clone();
        next.insert(agent.id(), look);
        let text = json!({ "agents": next }).to_string();
        let temp = self.file.with_extension("json.tmp");
        std::fs::write(&temp, text)?;
        std::fs::rename(&temp, &self.file)?;
        *chosen = next;
        Ok(Self::view(&chosen))
    }
}

async fn read(State(looks): State<Arc<Looks>>) -> Response {
    reply(StatusCode::OK, Looks::view(&lock(&looks.chosen)))
}

async fn change(State(looks): State<Arc<Looks>>, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let agent = body["agent"].as_str().and_then(Agent::from_id);
    let look = body["look"].as_str().and_then(known_look);
    let (Some(agent), Some(look)) = (agent, look) else {
        return reply(
            StatusCode::BAD_REQUEST,
            json!({ "error": OFFICE_BAD_INPUT }),
        );
    };
    match looks.choose(agent, look) {
        Ok(view) => reply(StatusCode::OK, view),
        Err(error) => {
            tracing::warn!(%error, "담당 외형을 남기지 못했다");
            reply(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": "internal" }),
            )
        }
    }
}

/// `GateBuilder::protected_routes`에 끼운다(세션 · 같은 출처 확인은 관문이 한다).
pub(crate) fn routes(looks: Arc<Looks>) -> Router {
    Router::new()
        .route(OFFICE_LOOKS_PATH, get(read))
        .route(OFFICE_LOOK_PATH, post(change))
        .with_state(looks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 기본_외형은_여섯이_모두_다르고_목록_안이다() {
        let looks: std::collections::BTreeSet<&str> =
            Agent::ALL.iter().map(|a| default_look(*a)).collect();
        assert_eq!(looks.len(), Agent::ALL.len());
        assert!(looks.iter().all(|look| known_look(look).is_some()));
    }

    #[test]
    fn 틀린_파일은_기본_외형으로_읽는다() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(LOOKS_FILE),
            "{\"agents\":{\"news\":\"../x\"}}",
        )
        .unwrap();
        let looks = Looks::open(dir.path());
        assert_eq!(lock(&looks.chosen)["news"], "office-seo");
    }
}
