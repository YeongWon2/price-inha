//! (31차 고침, 결정 22) 설정 화면의 알림음 켜기 · 끄기([`SETUP_SOUND_PATH`], V78 · R18-4).
//!
//! 알림음 설정은 브라우저 저장소가 아니라 **서버**가 둔다. 화면은 브라우저 저장소 · 쿠키 · 주소에 값을 남기지 않는다
//! (V46 · D11 · N8).
//!
//! # 읽기 (`GET` [`SETUP_SOUND_PATH`], 세션, 200)
//!
//! ```json
//! {"enabled": true}
//! ```
//!
//! 맨 위 칸은 `enabled` 하나뿐이다. 저장한 값이 없거나 설정 파일을 읽지 못하면 기본(켜짐, `true`)이다.
//!
//! # 바꾸기 (`POST` [`SETUP_SOUND_PATH`] `{"enabled": bool}`, 세션 · 같은 출처, 200)
//!
//! 답은 읽기와 같은 모양(저장한 뒤의 값)이다. `enabled`가 참 · 거짓이 아니면(없음 · 글자 · 수 · `null` · JSON 아님)
//! 400이고 저장하지 않는다(값은 그대로다).
//!
//! # 저장
//!
//! 보관 폴더(홈)의 설정 파일에 남긴다 — 하루 AI 상한을 두는 방식과 같게, 저장소 판을 올리지 않는다. 서버를 같은
//! 보관 폴더로 다시 만들어도 남는다.
//!
//! # 관문
//!
//! `/api/setup`의 다른 쓰기와 같다: 세션이 필요하고, 원격(다른 기기)은 세션이 없으면 401이며, 쓰기는 같은
//! 출처(`Origin`)만 받는다. 응답에는 `enabled` 밖의 값(키 값 등)이 없다.

/// 알림음 설정을 읽고(`GET`) 바꾸는(`POST`) 경로.
pub const SETUP_SOUND_PATH: &str = "/api/setup/sound";

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use axum::Router;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use serde_json::{Value, json};

use crate::SetupRefusal;
use crate::setup_http::{body_of, registered, reply};

/// 요청 · 응답 · 설정 파일의 칸 이름.
const ENABLED: &str = "enabled";

/// 저장한 값을 두는 파일 이름(보관 폴더 안). 비밀이 아니다. 모양 `{"enabled": bool}`.
pub(crate) const SOUND_FILE: &str = "sound.json";

/// 저장한 값이 없거나 읽지 못했을 때의 값(켜짐).
const SOUND_DEFAULT: bool = true;

/// 알림음 설정: 보관 폴더와 지금 값.
struct SoundPref {
    home: PathBuf,
    enabled: Mutex<bool>,
}

impl SoundPref {
    /// 보관 폴더에 저장한 값을 읽는다. 없거나 읽을 수 없으면 기본(읽을 수 없으면 남긴다).
    fn load(home: PathBuf) -> SoundPref {
        let enabled = match std::fs::read(home.join(SOUND_FILE)) {
            Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|value| value[ENABLED].as_bool())
                .unwrap_or_else(|| {
                    tracing::warn!("저장한 알림음 설정을 읽을 수 없어 켜짐으로 봅니다");
                    SOUND_DEFAULT
                }),
            Err(_) => SOUND_DEFAULT,
        };
        SoundPref {
            home,
            enabled: Mutex::new(enabled),
        }
    }

    fn get(&self) -> bool {
        *self.enabled.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 본문(`{"enabled": bool}`, 다른 칸 없이)대로 바꾸고 저장한 뒤의 값을 돌려준다. 잘못이면 그대로 둔다.
    fn change(&self, body: &[u8]) -> Result<bool, SetupRefusal> {
        let on = parse_request(body)?;
        let mut enabled = self.enabled.lock().unwrap_or_else(PoisonError::into_inner);
        save(&self.home, on)?;
        *enabled = on;
        drop(enabled);
        tracing::info!(enabled = on, "알림음 설정을 바꿨습니다");
        Ok(on)
    }
}

/// 요청 본문: `enabled` 칸 하나만 있는 객체이고 그 값이 참 · 거짓이어야 한다.
fn parse_request(body: &[u8]) -> Result<bool, SetupRefusal> {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(object)) if object.len() == 1 => object
            .get(ENABLED)
            .and_then(Value::as_bool)
            .ok_or(SetupRefusal::InvalidInput),
        _ => Err(SetupRefusal::InvalidInput),
    }
}

/// 값을 파일에 남긴다(임시 파일에 쓰고 옮긴다).
fn save(home: &Path, on: bool) -> Result<(), SetupRefusal> {
    let temp = home.join(format!("{SOUND_FILE}.tmp"));
    let body = json!({ ENABLED: on }).to_string();
    let written =
        std::fs::write(&temp, body).and_then(|()| std::fs::rename(&temp, home.join(SOUND_FILE)));
    written.map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        tracing::warn!(%error, "알림음 설정을 저장하지 못했습니다");
        SetupRefusal::SaveFailed
    })
}

/// 알림음 경로. 세션이 있어야 열린다(`GateBuilder::protected_routes`에 끼운다 — 쓰기의 같은 출처 확인도 관문이 한다).
pub(crate) fn routes(home: PathBuf) -> Router {
    Router::new()
        .route(SETUP_SOUND_PATH, get(read).post(write))
        .with_state(Arc::new(SoundPref::load(home)))
}

async fn read(State(pref): State<Arc<SoundPref>>) -> Response {
    reply(StatusCode::OK, json!({ ENABLED: pref.get() }))
}

async fn write(State(pref): State<Arc<SoundPref>>, request: Request) -> Response {
    let outcome = match body_of(request).await {
        Ok(body) => tokio::task::spawn_blocking(move || pref.change(&body))
            .await
            .unwrap_or(Err(SetupRefusal::SaveFailed)),
        Err(why) => Err(why),
    };
    match outcome {
        Ok(on) => reply(StatusCode::OK, json!({ ENABLED: on })),
        Err(why) => registered(Err(why)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 요청은_enabled_참_거짓_하나만_받는다() {
        assert_eq!(parse_request(br#"{"enabled":true}"#), Ok(true));
        assert_eq!(parse_request(br#"{"enabled":false}"#), Ok(false));
        for bad in [
            &br#"{"enabled":"true"}"#[..],
            br#"{"enabled":1}"#,
            br#"{"enabled":null}"#,
            br#"{}"#,
            br#"{"on":true}"#,
            br#"{"enabled":true,"on":true}"#,
            br#"true"#,
            br#"{"enabled":"#,
        ] {
            assert_eq!(parse_request(bad), Err(SetupRefusal::InvalidInput));
        }
    }

    #[test]
    fn 저장한_값은_다시_읽어도_같고_읽지_못하면_켜짐이다() {
        let dir = tempfile::tempdir().expect("임시 폴더");
        assert!(
            SoundPref::load(dir.path().to_path_buf()).get(),
            "없으면 켜짐"
        );
        let pref = SoundPref::load(dir.path().to_path_buf());
        assert_eq!(pref.change(br#"{"enabled":false}"#), Ok(false));
        assert!(
            !SoundPref::load(dir.path().to_path_buf()).get(),
            "다시 읽어도 꺼짐"
        );
        std::fs::write(dir.path().join(SOUND_FILE), "깨진 값").expect("쓰기");
        assert!(
            SoundPref::load(dir.path().to_path_buf()).get(),
            "읽지 못하면 켜짐"
        );
    }
}
