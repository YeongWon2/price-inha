//! (22차 6번) 하루 AI 호출 상한: 설정 화면이 보는 지금 값과 AI 설정 저장 길로 바꾸기.
//!
//! - 켤 때 인자 · 환경 · 팀 조립으로 준 값(켤 때 값)이 있으면 그 값이 이긴다(17차 4번 규칙 그대로).
//! - 화면에서 저장한 값은 보관 폴더의 [`AI_LIMIT_FILE`]에 남아 다시 켜도 같고, 켤 때 값이 없으면 저장한 그때부터
//!   곧바로 팀의 하루 상한이다(팀은 논의를 열 때마다 [`Desk::saved_ai_limit`]을 읽는다).

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use serde_json::{Value, json};

use super::Desk;
use crate::SetupRefusal;

/// (22차 6번) 하루 AI 호출 상한을 바꾸는 요청 칸. [`crate::SETUP_AI_PATH`]에 `POST` `{"daily_limit": 1 이상 정수 | null}`
/// (다른 칸 없이 이 칸 하나)을 보내면 AI 연결 고르기는 그대로 두고 상한만 바꾼다. `null`은 상한 없음이다.
/// 답은 200 `{"ai_daily_limit": 값 | null}`. 0 · 음수 · 정수가 아닌 값이면 400 `{"error": "invalid_input"}`이고
/// 그대로 둔다. 바꾼 값은 보관 폴더에 남아 다시 켜도 같고, 팀의 하루 상한([`crate::TeamWiring::ai_daily_limit`])이
/// 따로 주어지지 않았으면 그 값이 상한이다. 세션 · 같은 출처 · 그 컴퓨터여야 한다(AI 연결 고르기와 같다).
pub const AI_DAILY_LIMIT_KEY: &str = "daily_limit";

/// (22차 6번) 설정 상태(`GET` [`crate::SETUP_PATH`])의 하루 AI 호출 상한 칸 이름. 값은 지금 쓰는 상한(수) 또는
/// `null`(상한 없음)이다.
pub const SETUP_AI_DAILY_LIMIT: &str = "ai_daily_limit";

/// 저장한 상한을 두는 파일 이름(보관 폴더 안). 비밀이 아니다. 모양 `{"daily_limit": 수 | null}`.
pub(crate) const AI_LIMIT_FILE: &str = "ai-limit.json";

/// 켤 때 값과 저장한 값.
#[derive(Default)]
pub(crate) struct AiLimit {
    /// 팀이 있으면 `Some(켤 때 값)`. 팀이 없으면(설정만 여는 서버) 상한은 쓰이지 않아 설정 상태에 싣지 않는다.
    launch: Mutex<Option<Option<u32>>>,
    saved: Mutex<Option<u32>>,
}

impl AiLimit {
    /// 보관 폴더에 저장한 값을 읽는다. 없거나 읽을 수 없으면 없음(읽을 수 없으면 남긴다).
    pub(crate) fn load(home: &Path) -> AiLimit {
        let saved = match std::fs::read(home.join(AI_LIMIT_FILE)) {
            Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|value| parse_limit(&value[AI_DAILY_LIMIT_KEY]).ok())
                .unwrap_or_else(|| {
                    tracing::warn!("저장한 하루 AI 호출 상한을 읽을 수 없어 없음으로 봅니다");
                    None
                }),
            Err(_) => None,
        };
        AiLimit {
            launch: Mutex::new(None),
            saved: Mutex::new(saved),
        }
    }
}

/// 상한 값: `null`은 없음, 1 이상 `u32` 정수는 그 수. 그 밖은 잘못이다.
fn parse_limit(value: &Value) -> Result<Option<u32>, SetupRefusal> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_u64()
        .filter(|limit| *limit >= 1)
        .and_then(|limit| u32::try_from(limit).ok())
        .map(Some)
        .ok_or(SetupRefusal::InvalidInput)
}

/// 본문이 상한 바꾸기(`{"daily_limit": …}`가 있는 객체)면 그 객체. 아니면 `None`(AI 연결 고르기다).
pub(crate) fn limit_request(body: &[u8]) -> Option<serde_json::Map<String, Value>> {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(object)) if object.contains_key(AI_DAILY_LIMIT_KEY) => Some(object),
        _ => None,
    }
}

/// 저장한 값을 파일에 남긴다(임시 파일에 쓰고 옮긴다).
fn save(home: &Path, limit: Option<u32>) -> Result<(), SetupRefusal> {
    let temp = home.join(format!("{AI_LIMIT_FILE}.tmp"));
    let body = json!({ AI_DAILY_LIMIT_KEY: limit }).to_string();
    let written =
        std::fs::write(&temp, body).and_then(|()| std::fs::rename(&temp, home.join(AI_LIMIT_FILE)));
    written.map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        tracing::warn!(%error, "하루 AI 호출 상한을 저장하지 못했습니다");
        SetupRefusal::SaveFailed
    })
}

impl Desk {
    /// 팀을 조립했다고 적고 켤 때 준 상한을 적는다(저장한 값보다 앞선다).
    pub(crate) fn set_launch_ai_limit(&self, limit: Option<u32>) {
        *self
            .ai_limit
            .launch
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(limit);
    }

    /// 화면에서 저장한 상한.
    pub(crate) fn saved_ai_limit(&self) -> Option<u32> {
        *self
            .ai_limit
            .saved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// 지금 쓰는 상한: 켤 때 값이 있으면 그 값, 없으면 저장한 값.
    pub(crate) fn ai_daily_limit(&self) -> Option<u32> {
        self.team_ai_limit()
            .flatten()
            .or_else(|| self.saved_ai_limit())
    }

    /// 팀이 있으면 `Some(켤 때 값)`, 팀이 없으면 `None`.
    fn team_ai_limit(&self) -> Option<Option<u32>> {
        *self
            .ai_limit
            .launch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// 설정 상태에 싣는 상한: 팀이 있을 때만 `Some(지금 쓰는 상한)`.
    pub(crate) fn shown_ai_limit(&self) -> Option<Option<u32>> {
        self.team_ai_limit().map(|_| self.ai_daily_limit())
    }

    /// 상한을 바꾼다: 본문은 [`AI_DAILY_LIMIT_KEY`] 칸 하나다. 잘못이면 그대로 두고 [`SetupRefusal::InvalidInput`].
    /// 저장한 뒤 지금 쓰는 상한을 돌려준다.
    pub(crate) fn change_ai_limit(
        &self,
        request: &serde_json::Map<String, Value>,
    ) -> Result<Option<u32>, SetupRefusal> {
        if request.len() != 1 {
            return Err(SetupRefusal::InvalidInput);
        }
        let limit = parse_limit(&request[AI_DAILY_LIMIT_KEY])?;
        let mut saved = self
            .ai_limit
            .saved
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        save(&self.home, limit)?;
        *saved = limit;
        drop(saved);
        tracing::info!(limit = ?limit, "하루 AI 호출 상한을 바꿨습니다");
        Ok(self.ai_daily_limit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 상한_값은_null_또는_1_이상_정수만_받는다() {
        assert_eq!(parse_limit(&Value::Null), Ok(None));
        assert_eq!(parse_limit(&json!(3)), Ok(Some(3)));
        for bad in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!("3"),
            json!([3]),
            json!(u64::MAX),
        ] {
            assert_eq!(parse_limit(&bad), Err(SetupRefusal::InvalidInput), "{bad}");
        }
    }

    #[test]
    fn 저장하고_다시_읽는다() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(*AiLimit::load(dir.path()).saved.lock().unwrap(), None);
        save(dir.path(), Some(7)).unwrap();
        assert_eq!(*AiLimit::load(dir.path()).saved.lock().unwrap(), Some(7));
        save(dir.path(), None).unwrap();
        assert_eq!(*AiLimit::load(dir.path()).saved.lock().unwrap(), None);
    }

    #[test]
    fn 상한_바꾸기_본문만_가려낸다() {
        assert!(limit_request(br#"{"daily_limit": 3}"#).is_some());
        assert!(limit_request(br#"{"daily_limit": null}"#).is_some());
        assert!(limit_request(br#"{"method": "api_key"}"#).is_none());
        assert!(limit_request(b"not json").is_none());
    }
}
