//! (48차, 보안 검토 5번 · 결정 36 · E56) 시연에서 확인 없이 받아 둔 키의 표시와, 시연이 아닌 실행의 첫 확인.
//!
//! 시연(`run --demo` · `price-inha demo`)의 키 등록은 바깥에 닿지 않고 받아 둔다(47차). 그 키는 확인을 거치지
//! 않았으므로, 받아 둔 칸 이름만(값 · 길이 · 일부 없이) 보관 폴더의 [`UNVERIFIED_KEYS_FILE`](0600, 원자적 쓰기)에
//! 적는다. 시연이 아닌 실행은 켜질 때 표시된 칸마다 지금의 키 확인([`crate::KeyVerifier`])을 돌리고
//! ([`check::StartCheck`]), 표시가 남은 칸은 바깥 요청에 쓰지 않는다([`held`] — 증권사 · AI 자격 자리, 자료 ·
//! 뉴스 키([`check::HeldKeys`]), 백업 토큰이 "없음"으로 본다).
//!
//! 표시 상태: [`UNVERIFIED_PENDING`](확인 대기) · [`UNVERIFIED_REMOVED`](거절 → 지움) · [`UNVERIFIED_HELD`]
//! (거절 → 지우는 길이 없어 남긴 채 쓰지 않음). 실제판에서 확인을 지나 다시 등록하면 그 칸의 표시를 지운다.

pub(crate) mod check;

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use pi_secrets::{KeyKind, KeySlot, ProviderKeys};
use serde_json::{Value, json};

use crate::setup::{KEY_AI_API_KEY, KEY_BROKER, KEY_DATA, kind_named};
use crate::slots::{KEY_SLOTS, provider_declarations};

/// 보관 폴더의 표시 파일 이름: `{"keys": [{"item", "state", "announced"}]}`(값은 적지 않는다).
pub const UNVERIFIED_KEYS_FILE: &str = "unverified-keys.json";
/// 표시 상태: 아직 확인하지 못했다(확인 대기 — 쓰지 않고 다음 기회에 다시 확인한다).
pub const UNVERIFIED_PENDING: &str = "pending";
/// 표시 상태: 바깥이 거절해 그 키를 지웠다(다시 등록할 때까지 안내한다).
pub const UNVERIFIED_REMOVED: &str = "removed";
/// 표시 상태: 바깥이 거절했으나 비밀 보관소에 그 칸을 지우는 길이 없어 남긴 채 쓰지 않는다.
pub const UNVERIFIED_HELD: &str = "held";

/// 표시 파일을 읽고 고쳐 쓰는 사이에 다른 쓰기가 끼지 않게 한다(한 프로세스 안).
static FILE_LOCK: Mutex<()> = Mutex::new(());

/// 표시 한 줄: 칸 이름 · 상태 · 알렸는가.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mark {
    pub(crate) item: String,
    pub(crate) state: &'static str,
    pub(crate) announced: bool,
}

/// 표시가 가리키는 비밀 보관소의 칸.
#[derive(Clone, Copy)]
pub(crate) enum Target {
    Kind(KeyKind),
    Slot(KeySlot),
    Provider(ProviderKeys),
}

/// 칸 이름이 가리키는 보관소 칸. 모르는 이름은 없음.
pub(crate) fn target_of(item: &str) -> Option<Target> {
    if let Some(kind) = kind_named(item) {
        return Some(Target::Kind(kind));
    }
    if let Some((_, slot)) = KEY_SLOTS.iter().find(|(name, _)| *name == item) {
        return Some(Target::Slot(*slot));
    }
    provider_declarations()
        .into_iter()
        .find(|keys| keys.provider() == item)
        .map(Target::Provider)
}

/// 키 종류의 칸 이름(키 등록의 `item`).
pub(crate) fn kind_item(kind: KeyKind) -> &'static str {
    match kind {
        KeyKind::Broker => KEY_BROKER,
        KeyKind::AiApiKey => KEY_AI_API_KEY,
        KeyKind::Data => KEY_DATA,
    }
}

/// 바깥 키 칸의 칸 이름(키 등록의 `item`).
pub(crate) fn slot_item(slot: KeySlot) -> &'static str {
    KEY_SLOTS
        .iter()
        .find(|(_, known)| *known == slot)
        .map_or("slot", |(name, _)| *name)
}

/// 사람이 읽는 칸 이름("시연에서 받아 둔 ○○ 키").
fn label(item: &str) -> &str {
    match item {
        "broker" => "증권사",
        "ai_api_key" => "AI API",
        "dart" => "DART",
        "ecos" => "ECOS",
        "fred" => "FRED",
        "backup_github_token" => "백업 GitHub",
        "naver_news" => "네이버 뉴스",
        other => other,
    }
}

/// 설정 상태 · 알림에 싣는 안내 글.
pub(crate) fn message(mark: &Mark) -> String {
    let name = label(&mark.item);
    match mark.state {
        UNVERIFIED_REMOVED => {
            format!("시연에서 받아 둔 {name} 키가 확인되지 않아 지웠습니다 — 다시 등록하세요")
        }
        UNVERIFIED_HELD => {
            format!("시연에서 받아 둔 {name} 키가 확인되지 않아 쓰지 않습니다 — 다시 등록하세요")
        }
        _ => format!(
            "시연에서 받아 둔 {name} 키를 아직 확인하지 못해 쓰지 않습니다(확인 대기) — 다시 확인합니다"
        ),
    }
}

fn state_named(state: &str) -> &'static str {
    match state {
        UNVERIFIED_REMOVED => UNVERIFIED_REMOVED,
        UNVERIFIED_HELD => UNVERIFIED_HELD,
        _ => UNVERIFIED_PENDING,
    }
}

/// 표시 파일을 읽는다. 없으면 빈 목록, 읽지 못하거나 모양이 틀리면 오류(덮어쓰지 않는다).
pub(crate) fn load(home: &Path) -> std::io::Result<Vec<Mark>> {
    let text = match std::fs::read_to_string(home.join(UNVERIFIED_KEYS_FILE)) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "표시 파일 모양이 틀렸다");
    let parsed: Value = serde_json::from_str(&text).map_err(|_| bad())?;
    let list = parsed["keys"].as_array().ok_or_else(bad)?;
    list.iter()
        .map(|entry| {
            let item = entry["item"].as_str().ok_or_else(bad)?;
            Ok(Mark {
                item: item.to_string(),
                state: state_named(entry["state"].as_str().unwrap_or_default()),
                announced: entry["announced"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// 표시 파일을 고쳐 쓴다(잠금 안에서 읽기 → 고치기 → 쓰기). 비면 파일을 지운다.
pub(crate) fn update(home: &Path, change: impl FnOnce(&mut Vec<Mark>)) -> std::io::Result<()> {
    let _guard = FILE_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let mut marks = load(home)?;
    let before = marks.clone();
    change(&mut marks);
    if marks == before {
        return Ok(());
    }
    let path = home.join(UNVERIFIED_KEYS_FILE);
    if marks.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let keys: Vec<Value> = marks
        .iter()
        .map(|m| json!({ "item": m.item, "state": m.state, "announced": m.announced }))
        .collect();
    let bytes = serde_json::to_vec_pretty(&json!({ "keys": keys }))
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    write_private(&path, &bytes)
}

/// 같은 폴더의 임시 파일(0600)에 쓰고 디스크에 내린 뒤 이름을 바꾼다.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let temp = dir.join(format!(
        ".{UNVERIFIED_KEYS_FILE}.{}.tmp",
        std::process::id()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options.open(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(error) = written.and_then(|()| std::fs::rename(&temp, path)) {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

/// 키 등록 앞: 시연이면 그 칸을 확인 대기로 표시한다(저장하기 **전에** — 표시 없이 저장되지 않게).
pub(crate) fn before_register(home: &Path, item: &str, trial: bool) -> std::io::Result<()> {
    if !trial {
        return Ok(());
    }
    update(home, |marks| {
        marks.retain(|mark| mark.item != item);
        marks.push(Mark {
            item: item.to_string(),
            state: UNVERIFIED_PENDING,
            announced: false,
        });
    })
}

/// 키 등록이 끝난 뒤: 실제판에서 확인을 지나 저장했으면 그 칸의 표시를 지운다.
pub(crate) fn after_register(home: &Path, item: &str, trial: bool) {
    if trial {
        return;
    }
    if let Err(error) = update(home, |marks| marks.retain(|mark| mark.item != item)) {
        tracing::warn!(item, %error, "시연 표시를 지우지 못했습니다");
    }
}

/// 그 칸이 표시돼 있어 바깥 요청에 쓰면 안 되는가. 표시 파일을 읽지 못하면 안전하게 "쓰지 않음"으로 본다.
pub(crate) fn held(home: &Path, item: &str) -> bool {
    match load(home) {
        Ok(marks) => marks.iter().any(|mark| mark.item == item),
        Err(_) => true,
    }
}

/// 설정 상태에 `unverified`(`[{"item", "state", "message"}]`)를 더한다. 표시가 없으면 더하지 않는다.
pub(crate) fn add_to_status(home: &Path, body: &mut Value) {
    let marks = load(home).unwrap_or_default();
    if marks.is_empty() {
        return;
    }
    let list: Vec<Value> = marks
        .iter()
        .map(|m| json!({ "item": m.item, "state": m.state, "message": message(m) }))
        .collect();
    body["unverified"] = Value::Array(list);
}

/// `price-inha doctor`의 한 줄: 확인 대기 키가 있으면 "시연에서 받아 둔 키 n개 — 다음 실행에서 확인합니다".
pub(crate) fn doctor_line(home: &Path) -> Option<String> {
    let pending = load(home)
        .ok()?
        .iter()
        .filter(|mark| mark.state == UNVERIFIED_PENDING)
        .count();
    (pending > 0).then(|| format!("시연에서 받아 둔 키 {pending}개 — 다음 실행에서 확인합니다"))
}
