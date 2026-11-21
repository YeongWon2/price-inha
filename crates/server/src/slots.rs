//! (8차) 바깥 키 칸(`pi_secrets::KeySlot`: DART · ECOS · FRED · 백업 GitHub 토큰)의 등록 이름과 상태.
//!
//! 등록은 키 등록 경로(`/api/setup/keys` · `/api/remote/keys`)에 `item` 이름만 더한 것이다: 몸통 ·
//! 상한 · 공백 처리 · 값 지우기 · 답(등록 여부뿐)은 기존 키와 같다. 칸 이름은 `KeySlot::fields()` 그대로.
//! 확인은 [`crate::KeyVerifier`]의 `verify_slot`(자료 칸은 `pi_data::verify_slot`, 토큰은
//! `pi_backup::verify_token`)이고, 확인에 실패하면 저장하지 않는다.
//!
//! 칸별 상태는 `GET` [`SETUP_SLOTS_PATH`](세션): `{"slots": [{"name", "registered"}]}`([`KEY_SLOTS`] 순서).
//! 값 · 값의 일부 · 길이는 없다. 자료 키 · 토큰은 선택 항목이라 시작 조건(`ready`)에 들지 않는다.
//! (`GET /api/setup`의 모양은 앞 차수 시험이 칸을 못 박아 두어 따로 둔다.)

use pi_secrets::{Credentials, KeySlot, ProviderKeys, ProviderStatus, SlotStatus};
use serde_json::{Value, json};
use zeroize::Zeroize;

use crate::setup::{key_request_parts, kind_named, read_fields, wipe_value};
use crate::{KeyRequest, SetupRefusal};

/// 키 등록의 `item`: DART 키(`KeySlot::Dart`).
pub const KEY_DART: &str = "dart";
/// 키 등록의 `item`: ECOS 키(`KeySlot::Ecos`).
pub const KEY_ECOS: &str = "ecos";
/// 키 등록의 `item`: FRED 키(`KeySlot::Fred`).
pub const KEY_FRED: &str = "fred";
/// 키 등록의 `item`: 백업 GitHub 토큰(`KeySlot::BackupGithubToken`).
pub const KEY_BACKUP_GITHUB_TOKEN: &str = "backup_github_token";

/// 칸별 등록 여부를 읽는 경로(`GET`, 세션).
pub const SETUP_SLOTS_PATH: &str = "/api/setup/slots";

/// (10차 10b 10번) 공급자 선언(`pi_secrets::ProviderKeys`)에서 만든 키 칸 목록을 읽는 경로(`GET`, 세션).
///
/// 답: `{"providers": [{"provider": 공급자 이름, "fields": [칸 이름], "registered": bool}]}`. 값 · 값의 일부 ·
/// 길이는 없다. 네이버 뉴스(`pi_data::NAVER_NEWS_KEYS`)가 들어 있고, 옛 `item` `data` 칸은 없다.
/// 등록은 키 등록 경로(`/api/setup/keys` · `/api/remote/keys`)에 `item`으로 공급자 이름을 준다
/// (`{"item": "naver_news", "fields": {"client_id": .., "client_secret": ..}}`). 확인은
/// `KeyVerifier`의 `verify_provider`(네이버는 `pi_data::verify_news_keys`)이고, 실패하면 저장하지 않는다.
/// (옛 [`SETUP_SLOTS_PATH`] · `GET /api/setup`의 모양은 앞 차수 시험이 못 박아 두어 그대로다.)
pub const SETUP_PROVIDERS_PATH: &str = "/api/setup/providers";

/// `item` 이름과 칸(이 순서로 상태를 싣는다).
pub const KEY_SLOTS: [(&str, KeySlot); 4] = [
    (KEY_DART, KeySlot::Dart),
    (KEY_ECOS, KeySlot::Ecos),
    (KEY_FRED, KeySlot::Fred),
    (KEY_BACKUP_GITHUB_TOKEN, KeySlot::BackupGithubToken),
];

/// (10b 10번) 새 칸 목록에 싣는 공급자 선언들(선언을 가진 모듈의 상수에서 읽는다). 옛 `item` `data` 칸은 없다.
/// 새 공급자는 여기에 선언 한 줄을 더한다.
pub(crate) fn provider_declarations() -> Vec<ProviderKeys> {
    vec![pi_data::NAVER_NEWS_KEYS.provider_keys()]
}

fn provider_named(item: &str) -> Option<ProviderKeys> {
    provider_declarations()
        .into_iter()
        .find(|keys| keys.provider() == item)
}

fn slot_named(item: &str) -> Option<KeySlot> {
    KEY_SLOTS
        .iter()
        .find(|(name, _)| *name == item)
        .map(|(_, slot)| *slot)
}

fn name_of(slot: KeySlot) -> Option<&'static str> {
    KEY_SLOTS
        .iter()
        .find(|(_, known)| *known == slot)
        .map(|(name, _)| *name)
}

/// 칸 등록 요청. `Debug`를 만들지 않는다(값이 기록으로 나가지 않게).
pub(crate) struct SlotRequest {
    pub(crate) slot: KeySlot,
    pub(crate) values: Credentials,
}

/// (10b 10번) 공급자 키 등록 요청. `Debug`를 만들지 않는다(값이 기록으로 나가지 않게).
pub(crate) struct ProviderRequest {
    pub(crate) keys: ProviderKeys,
    pub(crate) values: Credentials,
}

/// 키 등록 본문: 앞 차수의 키 종류이거나 바깥 키 칸이거나 (10b) 공급자 선언의 키.
pub(crate) enum KeyBody {
    Kind(KeyRequest),
    Slot(SlotRequest),
    Provider(ProviderRequest),
}

impl KeyBody {
    /// [`KeyRequest::parse_wiping`]과 같이 읽되 칸 이름도 받는다. 돌아올 때 `body`는 모두 0이다.
    pub(crate) fn parse_wiping(body: &mut [u8]) -> Result<KeyBody, SetupRefusal> {
        let parsed = serde_json::from_slice::<Value>(body);
        body.zeroize();
        let mut parsed = parsed.map_err(|_| SetupRefusal::InvalidInput)?;
        let request = KeyBody::from_value(&parsed);
        wipe_value(&mut parsed);
        request
    }

    fn from_value(request: &Value) -> Result<KeyBody, SetupRefusal> {
        let (item, fields) = key_request_parts(request).ok_or(SetupRefusal::InvalidInput)?;
        if let Some(kind) = kind_named(item) {
            let values = read_fields(kind.fields(), fields)?;
            return Ok(KeyBody::Kind(KeyRequest { kind, values }));
        }
        if let Some(keys) = provider_named(item) {
            let values = read_fields(keys.fields(), fields)?;
            return Ok(KeyBody::Provider(ProviderRequest { keys, values }));
        }
        let slot = slot_named(item).ok_or(SetupRefusal::InvalidInput)?;
        let values = read_fields(slot.fields(), fields)?;
        Ok(KeyBody::Slot(SlotRequest { slot, values }))
    }
}

/// (10b 10번) 새 칸 목록의 응답 본문: 공급자 이름 · 칸 이름 · 등록 여부뿐이다(값 · 값의 일부 · 길이 없음).
pub(crate) fn providers_json(status: &[(ProviderKeys, bool)]) -> Value {
    let providers: Vec<Value> = status
        .iter()
        .map(|(keys, registered)| {
            json!({ "provider": keys.provider(), "fields": keys.fields(), "registered": registered })
        })
        .collect();
    json!({ "providers": providers })
}

/// 공급자 상태 한 줄에서 등록 여부.
pub(crate) fn registered_of(status: &[ProviderStatus], keys: ProviderKeys) -> bool {
    status
        .iter()
        .any(|s| s.provider == keys.provider() && s.registered)
}

/// 칸 상태의 응답 본문. 이름과 등록 여부뿐이다.
pub(crate) fn slots_json(status: &[SlotStatus]) -> Value {
    let slots: Vec<Value> = status
        .iter()
        .filter_map(|s| Some(json!({ "name": name_of(s.slot)?, "registered": s.registered })))
        .collect();
    json!({ "slots": slots })
}
