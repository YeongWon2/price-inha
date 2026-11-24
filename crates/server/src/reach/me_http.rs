//! 내 캐릭터(server 32차 1번, C17 · M8): [`OFFICE_ME_PATH`] 읽기 · 이름 · 외형 바꾸기,
//! [`OFFICE_ME_POSITION_PATH`] 자리 남기기.
//!
//! 저장은 저장소의 `player_profile`(`player` · `set_player_profile` · `set_player_spot`)에 한다. 이름의 한도는
//! 저장소가 정한 그대로다 — 여기서는 새로 정하지 않고 저장소의 거부를 400으로 옮긴다. 외형이 원본 목록에 있는지와
//! 자리가 사무실 칸 안인지는 여기서 본다(저장소는 보지 않는다). 사용자가 한 명이라 두 화면이 남기면 마지막 것이
//! 이긴다(E59) — 다른 화면에 중계하지 않는다.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post};
use pi_market::Clock;
use pi_store::{Facing, Player, Store, StoreError, UnixMillis};
use serde_json::{Value, json};

use super::looks_http::known_look;
use super::office_http::reply;
use super::{
    OFFICE_BAD_INPUT, OFFICE_COLS, OFFICE_ENTRANCE, OFFICE_ENTRANCE_DIR, OFFICE_ME_PATH,
    OFFICE_ME_POSITION_PATH, OFFICE_ROWS, PLAYER_DEFAULT_LOOK,
};

/// 내 캐릭터 저장소 창구.
pub(crate) struct Me {
    store: Store,
    clock: Arc<dyn Clock>,
    /// 바꾸기를 한 줄로 세운다(읽고 고쳐 쓰기가 섞이지 않게).
    turn: Mutex<()>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn facing_word(facing: Facing) -> &'static str {
    match facing {
        Facing::Up => "up",
        Facing::Down => "down",
        Facing::Left => "left",
        Facing::Right => "right",
    }
}

fn facing_of(word: &str) -> Option<Facing> {
    Facing::ALL.into_iter().find(|f| facing_word(*f) == word)
}

/// 화면에 주는 모양. 외형이 없거나 목록 밖이면 기본 외형, 자리가 없거나 칸 밖이면 입구.
fn view(player: &Player) -> Value {
    let look = player
        .look
        .as_deref()
        .and_then(known_look)
        .unwrap_or(PLAYER_DEFAULT_LOOK);
    let spot = player
        .spot
        .filter(|s| s.x < OFFICE_COLS && s.y < OFFICE_ROWS)
        .map_or(
            (OFFICE_ENTRANCE.0, OFFICE_ENTRANCE.1, OFFICE_ENTRANCE_DIR),
            |s| (s.x, s.y, facing_word(s.facing)),
        );
    json!({ "name": player.name, "look": look, "x": spot.0, "y": spot.1, "dir": spot.2 })
}

/// 칸 좌표 하나: 0 이상 `limit` 미만의 정수.
fn cell(value: &Value, limit: u32) -> Option<u32> {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n < limit)
}

fn bad_input() -> Response {
    reply(
        StatusCode::BAD_REQUEST,
        json!({ "error": OFFICE_BAD_INPUT }),
    )
}

fn internal(error: &StoreError) -> Response {
    tracing::warn!(%error, "내 캐릭터를 읽거나 남기지 못했다");
    reply(
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({ "error": "internal" }),
    )
}

/// 저장소의 답을 화면 답으로. 저장소가 이름을 거부하면 400.
fn answer(result: Result<Player, StoreError>) -> Response {
    match result {
        Ok(player) => reply(StatusCode::OK, view(&player)),
        Err(StoreError::InvalidPlayer(_)) => bad_input(),
        Err(error) => internal(&error),
    }
}

impl Me {
    pub(crate) fn new(store: Store, clock: Arc<dyn Clock>) -> Self {
        Self {
            store,
            clock,
            turn: Mutex::new(()),
        }
    }

    fn now(&self) -> UnixMillis {
        UnixMillis(self.clock.now().timestamp_millis())
    }

    /// 이름 · 외형을 바꾼다. `None`인 칸은 지금 값 그대로.
    fn profile(
        &self,
        name: Option<&str>,
        look: Option<&'static str>,
    ) -> Result<Player, StoreError> {
        let _turn = lock(&self.turn);
        let now = self.store.player()?;
        let name = name.unwrap_or(&now.name).to_owned();
        let look = look
            .or_else(|| now.look.as_deref().and_then(known_look))
            .unwrap_or(PLAYER_DEFAULT_LOOK);
        self.store.set_player_profile(&name, look, self.now())
    }

    fn spot(&self, x: u32, y: u32, facing: Facing) -> Result<Player, StoreError> {
        let _turn = lock(&self.turn);
        self.store.set_player_spot(x, y, facing, self.now())
    }
}

async fn read(State(me): State<Arc<Me>>) -> Response {
    match me.store.player() {
        Ok(player) => reply(StatusCode::OK, view(&player)),
        Err(error) => internal(&error),
    }
}

/// 이름 · 외형 바꾸기. 한 칸이라도 틀리면 아무것도 저장하지 않는다.
async fn change(State(me): State<Arc<Me>>, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let Some(fields) = body.as_object() else {
        return bad_input();
    };
    let name = match fields.get("name") {
        None => None,
        Some(Value::String(name)) => Some(name.as_str()),
        Some(_) => return bad_input(),
    };
    let look = match fields.get("look") {
        None => None,
        Some(look) => match look.as_str().and_then(known_look) {
            Some(look) => Some(look),
            None => return bad_input(),
        },
    };
    answer(me.profile(name, look))
}

/// 자리 남기기. 칸 밖 · 모양이 틀리면 저장하지 않는다.
async fn place(State(me): State<Arc<Me>>, body: Bytes) -> Response {
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if !body.is_object() {
        return bad_input();
    }
    let x = cell(&body["x"], OFFICE_COLS);
    let y = cell(&body["y"], OFFICE_ROWS);
    let facing = body["dir"].as_str().and_then(facing_of);
    let (Some(x), Some(y), Some(facing)) = (x, y, facing) else {
        return bad_input();
    };
    answer(me.spot(x, y, facing))
}

/// `GateBuilder::protected_routes`에 끼운다(세션 · 같은 출처 확인은 관문이 한다).
pub(crate) fn routes(me: Arc<Me>) -> Router {
    Router::new()
        .route(OFFICE_ME_PATH, get(read).post(change))
        .route(OFFICE_ME_POSITION_PATH, post(place))
        .with_state(me)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_store::PlayerSpot;

    fn player(spot: Option<PlayerSpot>, look: Option<&str>) -> Player {
        Player {
            name: "나".to_owned(),
            look: look.map(str::to_owned),
            spot,
            updated_at: None,
        }
    }

    #[test]
    fn 방향_글자는_넷이고_되돌릴_수_있다() {
        for facing in Facing::ALL {
            assert_eq!(facing_of(facing_word(facing)), Some(facing));
        }
        assert_eq!(facing_of("north"), None);
    }

    #[test]
    fn 칸_밖이거나_모르는_외형으로_저장된_값은_기본으로_읽는다() {
        let far = PlayerSpot {
            x: OFFICE_COLS,
            y: 0,
            facing: Facing::Left,
        };
        let body = view(&player(Some(far), Some("../x")));
        assert_eq!(body["look"], PLAYER_DEFAULT_LOOK);
        assert_eq!(body["x"], OFFICE_ENTRANCE.0);
        assert_eq!(body["dir"], OFFICE_ENTRANCE_DIR);
    }

    #[test]
    fn 칸_좌표는_범위_안의_정수만() {
        assert_eq!(cell(&json!(0), 3), Some(0));
        assert_eq!(cell(&json!(3), 3), None);
        assert_eq!(cell(&json!(-1), 3), None);
        assert_eq!(cell(&json!(1.0), 3), None);
    }
}
