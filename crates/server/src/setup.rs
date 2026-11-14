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
//!   "symbols_source": "argument"
//! }
//! ```
//!
//! - 칸은 위의 것이 전부다. 키의 값 · 일부 · 길이는 없다(V46). 종목은 비밀이 아니라서 보여 준다.
//! - `items`는 [`SETUP_ITEMS`]의 순서다. `ai`는 API 키와 구독 OAuth 토큰 가운데 하나라도 있으면 등록된 것이다.
//! - `ready`: 필수 항목이 모두 등록됐는가. `started`: 시세 받기를 시작했는가(`ready`이거나 시연).
//! - `symbols_source`: [`SymbolsSource::as_str`].
//!
//! # 키 등록 (`POST` [`SETUP_KEYS_PATH`])
//!
//! 본문(JSON): `{"item": "broker", "fields": {"client_id": "...", "client_secret": "..."}}`.
//! `item`은 [`KEY_BROKER`] · [`KEY_AI_API_KEY`] · [`KEY_AI_OAUTH`] · [`KEY_DATA`] 가운데 하나이고,
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

use std::path::Path;
use std::time::{Duration, Instant};

use pi_market::Symbol;
use pi_secrets::{Credentials, KeyKind};
use serde_json::Value;

use crate::{AppError, SetupRefusal};

/// 설정 상태를 읽는 경로(`GET`). 세션이 있어야 한다.
pub const SETUP_PATH: &str = "/api/setup";
/// 키를 등록하는 경로(`POST`). 세션 · 같은 출처 · 그 컴퓨터여야 한다.
pub const SETUP_KEYS_PATH: &str = "/api/setup/keys";
/// 지켜볼 종목을 등록하는 경로(`POST`). 세션 · 같은 출처 · 그 컴퓨터여야 한다.
pub const SETUP_SYMBOLS_PATH: &str = "/api/setup/symbols";

/// 설정 항목 이름: 증권사 키.
pub const ITEM_BROKER: &str = "broker";
/// 설정 항목 이름: AI 키(API 키 또는 구독 OAuth 토큰 가운데 하나).
pub const ITEM_AI: &str = "ai";
/// 설정 항목 이름: 지켜볼 종목.
pub const ITEM_SYMBOLS: &str = "symbols";
/// 설정 항목 이름: 자료 키(선택).
pub const ITEM_DATA: &str = "data";

/// 키 등록의 `item`: 증권사 키(`KeyKind::Broker`).
pub const KEY_BROKER: &str = "broker";
/// 키 등록의 `item`: AI API 키(`KeyKind::AiApiKey`).
pub const KEY_AI_API_KEY: &str = "ai_api_key";
/// 키 등록의 `item`: AI 구독 OAuth 토큰(`KeyKind::AiOAuth`).
pub const KEY_AI_OAUTH: &str = "ai_oauth";
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
}

impl SetupStatus {
    /// 빠진 필수 항목의 이름들([`SETUP_ITEMS`] 순서). 설정 알림의 `missing`이다.
    pub fn missing(&self) -> Vec<&'static str> {
        todo!("3차: 필수인데 등록되지 않은 항목의 이름")
    }

    /// 응답 본문(모듈 문서의 모양 그대로). 그 밖의 칸을 넣지 않는다.
    pub fn to_json(&self) -> Value {
        todo!("3차: 설정 상태를 JSON으로 적는다")
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
        let _ = body;
        todo!("3차: 키 등록 본문을 읽고 검사한다")
    }
}

/// 종목 등록 요청의 본문을 읽는다: `{"symbols": [..]}`.
///
/// 하나 이상 [`MAX_WATCH_SYMBOLS`]개 이하이고, 종목 코드는 `--symbols`와 같은 글자만 받는다.
/// 아니면 [`SetupRefusal::InvalidInput`].
pub fn parse_symbols(body: &[u8]) -> Result<Vec<Symbol>, SetupRefusal> {
    let _ = body;
    todo!("3차: 종목 등록 본문을 읽고 검사한다")
}

/// 보관 폴더의 [`WATCHLIST_FILE`]에서 저장된 종목을 읽는다. 파일이 없으면 빈 목록이다.
pub fn load_watchlist(home: &Path) -> Result<Vec<Symbol>, AppError> {
    let _ = home;
    todo!("3차: 저장된 종목을 읽는다")
}

/// 종목을 보관 폴더의 [`WATCHLIST_FILE`]에 원자 쓰기로 둔다
/// (같은 폴더의 임시 파일에 쓰고 이름을 바꾼다. 임시 파일은 남지 않는다).
pub fn save_watchlist(home: &Path, symbols: &[Symbol]) -> Result<(), AppError> {
    let _ = (home, symbols);
    todo!("3차: 종목을 원자 쓰기로 저장한다")
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
        let _ = (now, self.max, self.window, &self.recent);
        todo!("3차: 시도 횟수 제한")
    }
}
