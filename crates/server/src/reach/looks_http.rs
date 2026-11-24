//! 담당 외형(server 20차 4번): [`OFFICE_LOOKS_PATH`] 읽기 · [`OFFICE_LOOK_PATH`] 바꾸기.
//!
//! 외형 id는 원본 DeskRPG 외형 목록(`office-looks`, 커밋 5d80963)의 id이고 화면은 같은 출처의
//! `/office/deskrpg/characters/office/<id>.glb`를 부른다. 원본은 외형의 정본을 프로필 쪽에 저장한다 — 여기서는
//! (21차 6번) 저장소(`office_looks` · `set_office_look`)에 남겨 서버를 다시 켜도, 다른 기기에서 열어도 같고
//! 저장소 스냅샷과 함께 백업 · 되살리기에 따라간다(비밀이 아니다). 20차의 옛 파일([`LOOKS_FILE`])이 있으면
//! 처음 열 때 한 번 저장소로 옮기고 지운다.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post};
use pi_market::Clock;
use pi_office::Agent;
use pi_store::{Store, StoreError, UnixMillis};
use serde_json::{Value, json};

use super::office_http::reply;
use super::{OFFICE_BAD_INPUT, OFFICE_LOOK_PATH, OFFICE_LOOKS_PATH};

/// (20차) 고른 외형을 남기던 옛 파일(보관 폴더 안). 있으면 처음 열 때 저장소로 옮긴다.
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

pub(super) fn known_look(look: &str) -> Option<&'static str> {
    LOOK_IDS.iter().copied().find(|id| *id == look)
}

/// 담당 → 외형. 저장소에 없거나 틀린 값은 기본 외형이다.
pub(crate) struct Looks {
    store: Store,
    clock: Arc<dyn Clock>,
    /// 바꾸기를 한 줄로 세운다(읽기 · 쓰기 · 답이 섞이지 않게).
    turn: Mutex<()>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 저장된 값(담당 → 외형 id)을 담당마다 읽는다. 없거나 모르는 외형이면 기본 외형.
fn chosen_of(saved: &BTreeMap<String, String>) -> BTreeMap<&'static str, &'static str> {
    Agent::ALL
        .iter()
        .map(|agent| {
            let look = saved
                .get(agent.id())
                .and_then(|look| known_look(look))
                .unwrap_or_else(|| default_look(*agent));
            (agent.id(), look)
        })
        .collect()
}

impl Looks {
    /// 저장소로 연다. 옛 파일이 있으면 맞는 값만 저장소로 옮기고(저장소에 이미 있는 담당은 그대로) 지운다.
    pub(crate) fn open(home: &Path, store: Store, clock: Arc<dyn Clock>) -> Self {
        let looks = Self {
            store,
            clock,
            turn: Mutex::new(()),
        };
        if let Err(error) = looks.migrate(home) {
            tracing::warn!(%error, "옛 담당 외형 파일을 저장소로 옮기지 못했다");
        }
        looks
    }

    fn now(&self) -> UnixMillis {
        UnixMillis(self.clock.now().timestamp_millis())
    }

    fn migrate(&self, home: &Path) -> Result<(), StoreError> {
        let file = home.join(LOOKS_FILE);
        let Ok(text) = std::fs::read_to_string(&file) else {
            return Ok(());
        };
        let saved: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let _turn = lock(&self.turn);
        let already = self.store.office_looks()?;
        for agent in Agent::ALL {
            let look = saved["agents"][agent.id()].as_str().and_then(known_look);
            if let Some(look) = look
                && !already.contains_key(agent.id())
            {
                self.store.set_office_look(agent.id(), look, self.now())?;
            }
        }
        if let Err(error) = std::fs::remove_file(&file) {
            tracing::warn!(%error, "옮긴 옛 담당 외형 파일을 지우지 못했다");
        }
        Ok(())
    }

    fn view(chosen: &BTreeMap<&'static str, &'static str>) -> Value {
        json!({ "agents": chosen, "available": LOOK_IDS.as_slice() })
    }

    /// 지금 외형(담당마다).
    fn chosen(&self) -> Result<BTreeMap<&'static str, &'static str>, StoreError> {
        Ok(chosen_of(&self.store.office_looks()?))
    }

    /// 바꾸고 저장소에 남긴다. 남기지 못하면 바꾸지 않는다.
    fn choose(&self, agent: Agent, look: &'static str) -> Result<Value, StoreError> {
        let _turn = lock(&self.turn);
        self.store.set_office_look(agent.id(), look, self.now())?;
        Ok(Self::view(&self.chosen()?))
    }
}

fn internal(error: &StoreError) -> Response {
    tracing::warn!(%error, "담당 외형을 읽거나 남기지 못했다");
    reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({ "error": "internal" }),
    )
}

async fn read(State(looks): State<Arc<Looks>>) -> Response {
    match looks.chosen() {
        Ok(chosen) => reply(StatusCode::OK, Looks::view(&chosen)),
        Err(error) => internal(&error),
    }
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
        Err(error) => internal(&error),
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
    fn 틀린_값은_기본_외형으로_읽는다() {
        let saved = BTreeMap::from([("news".to_string(), "../x".to_string())]);
        assert_eq!(chosen_of(&saved)["news"], "office-seo");
    }
}
