//! 시작 전 필수 설정(3차 작업): 설정 상태 읽기 · 키 등록 · 종목 등록의 경로와 그 모양.
//!
//! **필수 항목이 모두 등록되기 전에는 시작하지 않는다.** 시작하지 않은 동안 바깥으로 나가는
//! 요청은 키 확인뿐이다(V31). 시연 모드(`--demo`)는 설정 없이 바로 시작한다.
//!
//! # 경로
//!
//! | 경로 | 누가 부를 수 있나 | 하는 일 |
//! |---|---|---|
//! | `GET` [`SETUP_PATH`] | 세션 | 설정 상태([`SetupStatus`])를 JSON으로 준다 |
//! | `POST` [`SETUP_KEYS_PATH`] | 세션 + 같은 출처 + 그 컴퓨터(`pi_gate::local_only`) | 키를 확인하고 등록한다(C3 · C16) |
//! | `POST` [`SETUP_SYMBOLS_PATH`] | 세션 + 같은 출처 + 그 컴퓨터 | 지켜볼 종목을 저장한다 |
//! | `POST` [`crate::SETUP_AI_PATH`] | 세션 + 같은 출처 + 그 컴퓨터 | (6차) AI 연결을 고르고 CLI면 확인한다([`crate::ai_link`]) |
//!
//! # 설정 상태 (`GET` [`SETUP_PATH`], 200)
//!
//! ```json
//! {
//!   "items": [
//!     {"name": "broker",  "required": true,  "registered": false},
//!     {"name": "ai",      "required": true,  "registered": false},
//!     {"name": "symbols", "required": true,  "registered": true},
//!     {"name": "data",    "required": false, "registered": false}
//!   ],
//!   "ready": false,
//!   "started": false,
//!   "demo": false,
//!   "symbols": ["005930", "000660"],
//!   "symbols_source": "argument",
//!   "ai": {"method": "api_key", "command": null, "config_dir": null, "cli": null,
//!          "cli_version": null, "subscription": null, "legacy_auth_file": false}
//! }
//! ```
//!
//! - 칸은 위의 것이 전부다. 키의 값 · 일부 · 길이는 없다(V46). 종목은 비밀이 아니라서 보여 준다.
//! - `items`는 [`SETUP_ITEMS`]의 순서다. (6차) `ai`는 사용자가 고른 연결이 갖춰졌을 때만 등록된 것이다:
//!   API 키를 골랐으면 API 키가 등록됐을 때, CLI를 골랐으면 마지막 확인이 로그인된 구독일 때.
//!   둘 다 있어도 고르지 않은 쪽으로 넘어가지 않는다. 고른 것이 없으면 API 키를 고른 것으로 본다.
//! - (6차) `ai` 칸: 고른 연결 · CLI 경로 · 확인 상태 · 옛 `auth.json` 알림. 모양은 [`crate::ai_link`] 문서.
//! - `ready`: 필수 항목이 모두 등록됐는가. `started`: 시세 받기를 시작했는가(`ready`이거나 시연).
//! - `symbols_source`: [`SymbolsSource::as_str`].
//!
//! # 키 등록 (`POST` [`SETUP_KEYS_PATH`])
//!
//! 본문(JSON): `{"item": "broker", "fields": {"client_id": "...", "client_secret": "..."}}`.
//! `item`은 [`KEY_BROKER`] · [`KEY_AI_API_KEY`] · [`KEY_DATA`] 가운데 하나이고(6차: 구독 토큰 항목은 없다),
//! `fields`의 칸은 `pi_secrets::KeyKind::fields`와 꼭 같아야 한다(값은 글자).
//!
//! - 성공: 200 `{"registered": true}`. 그 밖의 칸은 없다.
//! - 실패: [`SetupRefusal::status`]와 `{"error": "<`[`SetupRefusal::reason`]`>"}`. 그 밖의 칸은 없다.
//! - 받은 값은 응답(본문 · 헤더) · 오류 · 기록 · 이벤트 · 저장소 어디에도 나오지 않는다.
//!   확인에 실패한 키는 저장하지 않는다(E4).
//!
//! 순서: 입력 검사(실패하면 [`SetupRefusal::InvalidInput`], 확인기를 부르지 않는다) →
//! 시도 횟수([`AttemptLimit`], 넘으면 [`SetupRefusal::TooManyAttempts`], 확인기를 부르지 않는다) →
//! `SecretStore::register`(확인기는 [`crate::KeyVerifier`]).
//!
//! # 종목 등록 (`POST` [`SETUP_SYMBOLS_PATH`])
//!
//! 본문(JSON): `{"symbols": ["005930", "000660"]}`. 하나 이상 [`MAX_WATCH_SYMBOLS`]개 이하이고,
//! 종목 코드는 `--symbols`와 같은 글자만 받는다. 성공은 200 `{"registered": true}`.
//!
//! - 보관 폴더의 [`WATCHLIST_FILE`]에 원자 쓰기로 둔다(임시 파일에 쓰고 이름을 바꾼다. 임시 파일은 남지 않는다).
//!   모양은 `{"symbols": ["005930", "000660"]}`. 비밀이 아니다.
//! - 인자 · 환경 변수로 종목을 줬으면 그것이 먼저다: 그 실행 동안 그것만 쓰고, 저장된 파일을 덮지 않는다.
//!   이때 종목 등록은 [`SetupRefusal::FixedByArgument`]로 거부한다(저장된 것과 쓰는 것이 어긋나지 않게).
//! - (4차) 종목 등록에도 횟수 제한이 있다(`Settings::symbols_attempts` · `Settings::symbols_window`,
//!   키 등록과 따로 센다). 넘으면 [`SetupRefusal::TooManyAttempts`]이고 저장하지 않는다.
//! - (4차) 지켜보는 목록이 실제로 바뀌었을 때만 흐름을 깨운다(같은 목록을 다시 등록하면 깨우지 않는다).
//! - (4차) 종목 글자에 `..`가 들어 있으면 받지 않는다(인자 · 화면 · 저장된 파일 모두).
//!
//! # 알림 (실시간 연결의 지나가는 이벤트 [`KIND_SETUP`])
//!
//! 내용: `{"state": "required" | "started", "missing": [빠진 필수 항목 이름들], "demo": bool}`.
//!
//! - 시작 전: `state`는 [`SETUP_REQUIRED`], `missing`은 [`SETUP_ITEMS`] 순서. 시세 상태는
//!   [`crate::FEED_SETUP_REQUIRED`]다.
//! - 마지막 필수 항목이 등록되면 다시 켜지 않아도 바로 시작하고 [`SETUP_STARTED`]를 알린다(`missing`은 비어 있다).
//! - 시작한 뒤 필수 항목이 빠지면(키 파일이 지워짐 등) 멈추고 다시 [`SETUP_REQUIRED`]를 알린다.
//! - 시작한 뒤 키를 다시 등록하면 새 키로 이어 간다. 키나 종목이 등록되면 물러서 있는 중이어도 바로 다시 시도한다.

use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pi_market::Symbol;
use pi_secrets::{Credentials, KeyKind, MIN_SECRET_CHARS};
use serde_json::{Value, json};
use zeroize::Zeroize;

use crate::{AiStatus, AppError, SetupRefusal};

/// 설정 상태를 읽는 경로(`GET`). 세션이 있어야 한다.
pub const SETUP_PATH: &str = "/api/setup";
/// 키를 등록하는 경로(`POST`). 세션 · 같은 출처 · 그 컴퓨터여야 한다.
pub const SETUP_KEYS_PATH: &str = "/api/setup/keys";
/// 다른 기기에서 키를 등록하는 경로(`POST`, 몸통은 [`SETUP_KEYS_PATH`]와 같다). 세션(TOTP 로그인) ·
/// 같은 출처면 원격에서도 열린다. 답은 등록 여부뿐이고 키 값을 다시 보여 주지 않는다.
pub const REMOTE_KEYS_PATH: &str = "/api/remote/keys";
/// 다른 기기에서 키만 넣는 작은 화면(`GET`). 세션이 있어야 열린다(공개 경로가 아니다).
pub const KEYS_PAGE_PATH: &str = "/keys";
/// 지켜볼 종목을 등록하는 경로(`POST`). 세션 · 같은 출처 · 그 컴퓨터여야 한다.
pub const SETUP_SYMBOLS_PATH: &str = "/api/setup/symbols";

/// 설정 항목 이름: 증권사 키.
pub const ITEM_BROKER: &str = "broker";
/// 설정 항목 이름: AI 연결(API 키 또는 Claude Code CLI 가운데 사용자가 고른 하나, 6차).
pub const ITEM_AI: &str = "ai";
/// 설정 항목 이름: 지켜볼 종목.
pub const ITEM_SYMBOLS: &str = "symbols";
/// 설정 항목 이름: 자료 키(선택).
pub const ITEM_DATA: &str = "data";

/// 키 등록의 `item`: 증권사 키(`KeyKind::Broker`).
pub const KEY_BROKER: &str = "broker";
/// 키 등록의 `item`: AI API 키(`KeyKind::AiApiKey`).
pub const KEY_AI_API_KEY: &str = "ai_api_key";
/// 키 등록의 `item`: 자료 키(`KeyKind::Data`).
pub const KEY_DATA: &str = "data";

/// 지나가는 이벤트 종류: 설정 알림. 내용은 `{"state", "missing", "demo"}`.
pub const KIND_SETUP: &str = "setup";
/// 설정 알림의 `state`: 필수 항목이 빠져 시작하지 않았다(또는 멈췄다).
pub const SETUP_REQUIRED: &str = "required";
/// 설정 알림의 `state`: 시작했다.
pub const SETUP_STARTED: &str = "started";

/// 지켜볼 종목을 두는 파일 이름(보관 폴더 안). 비밀이 아니다.
pub const WATCHLIST_FILE: &str = "watchlist.json";
/// 지켜볼 종목 수의 상한(증권사 현재가 요청 하나에 담는 수).
pub const MAX_WATCH_SYMBOLS: usize = 200;

/// 설정 항목 하나의 정의.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupItemSpec {
    /// 항목 이름.
    pub name: &'static str,
    /// 필수인가.
    pub required: bool,
}

/// 설정 항목의 목록. **필수 항목은 이 표 한 곳에서 정한다**(템플릿 고르기 같은 항목은 여기에 더한다).
pub const SETUP_ITEMS: [SetupItemSpec; 4] = [
    SetupItemSpec {
        name: ITEM_BROKER,
        required: true,
    },
    SetupItemSpec {
        name: ITEM_AI,
        required: true,
    },
    SetupItemSpec {
        name: ITEM_SYMBOLS,
        required: true,
    },
    SetupItemSpec {
        name: ITEM_DATA,
        required: false,
    },
];

/// 설정 항목 하나의 상태. 값 · 값의 일부 · 길이는 없다(V46).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupItem {
    /// 항목 이름.
    pub name: &'static str,
    /// 필수인가.
    pub required: bool,
    /// 등록됐는가.
    pub registered: bool,
}

/// 지금 쓰는 종목이 어디서 왔는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolsSource {
    /// 인자 · 환경 변수. 그 실행 동안만 쓰고 저장된 것을 덮지 않는다.
    Argument,
    /// 보관 폴더의 [`WATCHLIST_FILE`].
    Saved,
    /// 없다.
    None,
}

impl SymbolsSource {
    /// 설정 상태의 `symbols_source`에 적는 글자: `argument` · `saved` · `none`.
    pub const fn as_str(self) -> &'static str {
        match self {
            SymbolsSource::Argument => "argument",
            SymbolsSource::Saved => "saved",
            SymbolsSource::None => "none",
        }
    }
}

/// 설정 상태. `GET` [`SETUP_PATH`]의 응답이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupStatus {
    /// 항목들([`SETUP_ITEMS`] 순서).
    pub items: Vec<SetupItem>,
    /// 필수 항목이 모두 등록됐는가.
    pub ready: bool,
    /// 시세 받기를 시작했는가(`ready`이거나 시연 모드).
    pub started: bool,
    /// 시연 모드인가.
    pub demo: bool,
    /// 지금 쓰는 종목(비밀이 아니다).
    pub symbols: Vec<Symbol>,
    /// 그 종목이 어디서 왔는가.
    pub symbols_source: SymbolsSource,
    /// (6차) AI 연결: 고른 방식 · CLI 경로 · 확인 상태 · 옛 `auth.json` 알림.
    pub ai: AiStatus,
}

impl SetupStatus {
    /// 빠진 필수 항목의 이름들([`SETUP_ITEMS`] 순서). 설정 알림의 `missing`이다.
    pub fn missing(&self) -> Vec<&'static str> {
        self.items
            .iter()
            .filter(|item| item.required && !item.registered)
            .map(|item| item.name)
            .collect()
    }

    /// 응답 본문(모듈 문서의 모양 그대로). 그 밖의 칸을 넣지 않는다.
    pub fn to_json(&self) -> Value {
        let items: Vec<Value> = self
            .items
            .iter()
            .map(|item| {
                json!({
                    "name": item.name,
                    "required": item.required,
                    "registered": item.registered,
                })
            })
            .collect();
        let symbols: Vec<&str> = self.symbols.iter().map(Symbol::as_str).collect();
        json!({
            "items": items,
            "ready": self.ready,
            "started": self.started,
            "demo": self.demo,
            "symbols": symbols,
            "symbols_source": self.symbols_source.as_str(),
            "ai": self.ai.to_json(),
        })
    }
}

/// 키 등록 요청. `Debug`를 만들지 않는다(값이 기록으로 나가지 않게).
pub struct KeyRequest {
    /// 등록할 종류.
    pub kind: KeyKind,
    /// 칸 값들.
    pub values: Credentials,
}

impl KeyRequest {
    /// 요청 본문을 읽는다. 확인기를 부르기 전에 거르는 것:
    /// JSON이 아님 · 모르는 `item` · 빠진 칸 · 모르는 칸 · 글자가 아닌 값 · 빈 값 ·
    /// `pi_secrets::MIN_SECRET_CHARS`보다 짧은 값 · 줄바꿈이 든 값.
    /// 모두 [`SetupRefusal::InvalidInput`]이고, 오류에 받은 값을 싣지 않는다.
    pub fn parse(body: &[u8]) -> Result<KeyRequest, SetupRefusal> {
        let mut parsed =
            serde_json::from_slice::<Value>(body).map_err(|_| SetupRefusal::InvalidInput)?;
        let request = KeyRequest::from_value(&parsed);
        wipe_value(&mut parsed);
        request
    }

    /// [`KeyRequest::parse`]와 같이 읽고, 다 읽은 뒤 받은 본문을 0으로 덮는다. (4차, 참고 6)
    ///
    /// 성공해도 실패해도 돌아올 때 `body`의 모든 바이트는 0이다. 읽는 동안 만든 중간 값(JSON 글자 등)도
    /// 다 쓰면 바로 0으로 덮는다. 키 등록 경로는 받은 본문을 이것으로 읽는다.
    pub fn parse_wiping(body: &mut [u8]) -> Result<KeyRequest, SetupRefusal> {
        let request = KeyRequest::parse(body);
        body.zeroize();
        request
    }

    /// 읽은 JSON에서 요청을 꺼낸다. 값은 [`Credentials`]로 옮기고 JSON 쪽은 부른 쪽이 지운다.
    fn from_value(request: &Value) -> Result<KeyRequest, SetupRefusal> {
        let (kind, fields) = key_request_parts(request).ok_or(SetupRefusal::InvalidInput)?;
        let names = kind.fields();
        if fields.len() != names.len() {
            return Err(SetupRefusal::InvalidInput);
        }
        let mut values = Credentials::new();
        for &name in names {
            let value = fields
                .get(name)
                .and_then(Value::as_str)
                .and_then(usable_secret)
                .ok_or(SetupRefusal::InvalidInput)?;
            values = values.with(name, value);
        }
        Ok(KeyRequest { kind, values })
    }
}

/// JSON 값 안의 모든 글자를 0으로 덮고 비운다(받은 키 값이 든 중간 값을 남기지 않는다).
fn wipe_value(value: &mut Value) {
    match value {
        Value::String(text) => text.zeroize(),
        Value::Array(items) => items.iter_mut().for_each(wipe_value),
        Value::Object(map) => map.values_mut().for_each(wipe_value),
        Value::Number(_) | Value::Bool(_) | Value::Null => {}
    }
}

/// 본문에서 종류와 칸 묶음을 꺼낸다. 본문은 `item`과 `fields` 두 칸뿐인 JSON 객체여야 한다.
fn key_request_parts(request: &Value) -> Option<(KeyKind, &serde_json::Map<String, Value>)> {
    let request = request.as_object()?;
    let kind = match request.get("item")?.as_str()? {
        KEY_BROKER => KeyKind::Broker,
        KEY_AI_API_KEY => KeyKind::AiApiKey,
        KEY_DATA => KeyKind::Data,
        _ => return None,
    };
    let Some(Value::Object(fields)) = request.get("fields") else {
        return None;
    };
    (request.len() == 2).then_some((kind, fields))
}

/// 등록할 수 있는 값이면 앞뒤 공백을 뗀 글자를 준다(붙여 넣다 딸려 온 공백은 값이 아니다).
/// 빈 값 · 공백뿐인 값 · 너무 짧은 값 · 줄바꿈 같은 제어 문자가 든 값은 `None`.
fn usable_secret(raw: &str) -> Option<&str> {
    let value = raw.trim();
    let long_enough = value.chars().count() >= MIN_SECRET_CHARS;
    let writable = !value
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'));
    (long_enough && writable).then_some(value)
}

/// 종목 등록 요청의 본문을 읽는다: `{"symbols": [..]}`.
///
/// 하나 이상 [`MAX_WATCH_SYMBOLS`]개 이하이고, 종목 코드는 `--symbols`와 같은 글자만 받는다.
/// 아니면 [`SetupRefusal::InvalidInput`].
pub fn parse_symbols(body: &[u8]) -> Result<Vec<Symbol>, SetupRefusal> {
    let request: Value = serde_json::from_slice(body).map_err(|_| SetupRefusal::InvalidInput)?;
    let list = match request.as_object() {
        Some(request) if request.len() == 1 => request.get("symbols"),
        _ => None,
    };
    symbols_from(list.ok_or(SetupRefusal::InvalidInput)?)
}

/// JSON 목록을 종목들로 읽는다. 하나 이상 [`MAX_WATCH_SYMBOLS`]개 이하의 글자 목록이어야 하고
/// 글자마다 종목 글자 규칙(`pi_market::Symbol::parse`, 7차 4번)에 맞아야 한다. 아니면
/// [`SetupRefusal::InvalidInput`]이다(어느 글자인지는 싣지 않는다 — 받은 글자를 되돌려 보이지 않는다).
/// 같은 종목을 두 번 적었으면 처음 것만 남긴다.
fn symbols_from(list: &Value) -> Result<Vec<Symbol>, SetupRefusal> {
    let list = list.as_array().ok_or(SetupRefusal::InvalidInput)?;
    if list.is_empty() || list.len() > MAX_WATCH_SYMBOLS {
        return Err(SetupRefusal::InvalidInput);
    }
    let mut symbols: Vec<Symbol> = Vec::with_capacity(list.len());
    for code in list {
        let code = code.as_str().ok_or(SetupRefusal::InvalidInput)?;
        let symbol = crate::settings::symbol(code).ok_or(SetupRefusal::InvalidInput)?;
        if !symbols.contains(&symbol) {
            symbols.push(symbol);
        }
    }
    Ok(symbols)
}

/// 파일 작업 실패는 종류만 남긴다.
fn home_error(error: &std::io::Error) -> AppError {
    AppError::Home { kind: error.kind() }
}

/// 보관 폴더의 [`WATCHLIST_FILE`]에서 저장된 종목을 읽는다. 파일이 없으면 빈 목록이다.
///
/// (8차 2번) 읽지 못하거나, 읽을 수 없는 모양이거나, 지금 등록 규칙([`parse_symbols`]:
/// `pi_market::Symbol::parse` 등)에 맞지 않는 종목이 있으면(`InvalidData`) [`AppError::BadFile`]이고,
/// 오류에는 파일 이름([`WATCHLIST_FILE`])만 있다(종목 값은 넣지 않는다). 파일은 고치지 않는다.
pub fn load_watchlist(home: &Path) -> Result<Vec<Symbol>, AppError> {
    let bad = |kind| AppError::BadFile {
        file: WATCHLIST_FILE,
        kind,
    };
    let bytes = match std::fs::read(home.join(WATCHLIST_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(bad(error.kind())),
    };
    // 읽을 수 없는 모양이면 말없이 비우지 않고 알린다(부른 쪽이 정한다).
    parse_symbols(&bytes).map_err(|_| bad(ErrorKind::InvalidData))
}

/// 종목을 보관 폴더의 [`WATCHLIST_FILE`]에 원자 쓰기로 둔다
/// (같은 폴더의 임시 파일에 쓰고 이름을 바꾼다. 임시 파일은 남지 않는다).
pub fn save_watchlist(home: &Path, symbols: &[Symbol]) -> Result<(), AppError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let codes: Vec<&str> = symbols.iter().map(Symbol::as_str).collect();
    let text = json!({ "symbols": codes }).to_string();
    // 함께 도는 저장이 같은 임시 파일을 쓰지 않게 이름을 다르게 한다.
    let temp = home.join(format!(
        ".{WATCHLIST_FILE}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = write_new(&temp, text.as_bytes())
        .and_then(|()| std::fs::rename(&temp, home.join(WATCHLIST_FILE)));
    written.map_err(|error| {
        // 쓰다 만 임시 파일을 남기지 않는다(지우지 못해도 더 할 수 있는 일이 없다).
        let _ = std::fs::remove_file(&temp);
        home_error(&error)
    })
}

/// 새 파일을 만들어 쓰고 디스크에 내린다. 같은 이름이 이미 있으면 실패한다.
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// 키 등록 시도의 횟수 제한(3차 규칙 9): `window` 동안 `max`번까지만 확인기를 부른다.
///
/// 입력 검사를 지나 확인기를 부르게 되는 시도만 센다(성공 · 실패 모두). 넘으면 확인기를 부르지 않는다.
/// 시각은 인자로 받는다.
#[derive(Debug)]
pub struct AttemptLimit {
    max: u32,
    window: Duration,
    /// `window` 안에 있었던 시도들의 시각.
    recent: Vec<Instant>,
}

impl AttemptLimit {
    /// `window` 동안 `max`번까지 허용하는 제한.
    pub fn new(max: u32, window: Duration) -> Self {
        Self {
            max,
            window,
            recent: Vec::new(),
        }
    }

    /// 지금 시도해도 되는가. 되면 그 시도를 센다. `window`가 지난 시도는 잊는다.
    pub fn allow(&mut self, now: Instant) -> bool {
        let window = self.window;
        self.recent
            .retain(|at| now.saturating_duration_since(*at) < window);
        let allowed = u32::try_from(self.recent.len()).is_ok_and(|used| used < self.max);
        if allowed {
            self.recent.push(now);
        }
        allowed
    }
}
