//! 6차 작업: AI 연결을 "API 키 또는 Claude Code CLI(구독)" 가운데 사용자가 고른 하나로 잇는다.
//!
//! 근거: `docs/dev/modules/gate.md` "6차 작업", `docs/dev/decisions.md` 2026-11-15, `dev.md` D4,
//! `docs/research/cli-backend.md` 5.7.
//!
//! - 구독 OAuth 토큰을 받는 칸 · 단추 · 등록 길은 없다. 로그인은 사용자가 터미널에서 공식 CLI로 한다.
//! - **사용자가 고른 하나만 쓴다.** 둘 다 있어도 넘어가기는 없다(원본 OpenClaw 2.8).
//!   고른 것이 없으면(예전 보관 폴더) API 키를 고른 것으로 본다.
//! - 고른 방식 · 실행 파일 경로 · 전용 로그인 폴더는 비밀이 아니다. 종목처럼 보관 폴더의
//!   [`AI_CHOICE_FILE`]에 원자 쓰기로 둔다(`.env` · `auth.json`에 넣지 않는다).
//!
//! # AI 연결 고르기 (`POST` [`SETUP_AI_PATH`], 세션 + 같은 출처 + 그 컴퓨터)
//!
//! 본문(JSON) 둘 가운데 하나:
//!
//! - `{"method": "api_key"}`: API 키를 쓴다. 저장하고, API 키가 등록돼 있으면 200 `{"registered": true}`,
//!   아니면 200 `{"registered": false}`(API 키는 `POST` [`crate::SETUP_KEYS_PATH`]의
//!   [`crate::KEY_AI_API_KEY`]로 따로 등록한다).
//! - `{"method": "cli", "command": "<절대 경로>", "config_dir": "<절대 경로>" | "" | null}`:
//!   CLI를 쓴다. `config_dir`은 빠져도 되고, 비었거나 `null`이면 평소 로그인이다.
//!
//! 순서(CLI): 입력 검사(실패하면 [`SetupRefusal::InvalidInput`], 아무것도 띄우지 않고 저장하지 않는다) →
//! 시도 횟수(키 등록과 같은 제한, 넘으면 [`SetupRefusal::TooManyAttempts`], 띄우지 않는다) →
//! 고른 것을 저장 → `pi_ai::ClaudeCliProvider::check`(버전 → 로그인, 모델을 부르지 않는다).
//!
//! - [`pi_ai::CliStatus::LoggedIn`](구독 `claude.ai`)이면 200 `{"registered": true}`이고 AI 연결이 갖춰진다.
//! - 아니면 [`cli_refusal`]의 까닭으로 거부한다. 고른 것은 저장돼 있고 AI 연결은 갖춰지지 않은 것이다
//!   (앞서 갖춰졌어도 지금 확인이 아니면 갖춰지지 않은 것이다. 시작했으면 멈추고 설정이 필요하다고 알린다).
//! - 띄울 때(`crate::App::start`) 고른 것이 CLI면 한 번 확인하고 그 결과를 쓴다(모델을 부르지 않는다).
//!   (7차 11번) 서버는 그 확인을 기다리지 않고 먼저 연결을 받는다. 확인은 뒤에서 하고, 끝나기 전 상태는
//!   [`CLI_CHECKING`]이다(갖춰진 것이 아니다).
//! - (7차 9번) 작업 폴더 준비와 [`AI_CHOICE_FILE`] 쓰기는 막는 작업 자리(`spawn_blocking`)에서 한다.
//! - (7차 10번) 이미 있는 작업 폴더가 비어 있지 않으면 지우지 않고 CLI를 쓰지 않는다
//!   ([`SetupRefusal::CliWorkDirNotEmpty`], 상태는 [`CLI_WORK_NOT_EMPTY`]).
//!   (8차 1번) `pi_ai::is_own_work_file`이 참인 자기 임시 파일만 있으면 비어 있는 것으로 본다.
//!   서버가 시작할 때(호출이 하나도 돌지 않을 때)는 그 남은 파일을 지우고, 호출이 돌 수 있는 다시 고르기에서는
//!   지우지 않는다. 다른 항목이 하나라도 있으면 아무것도 지우지 않고 막는다.
//! - (8차 3번) 서버가 꺼지기 시작한 뒤의 고르기는 [`SetupRefusal::ShuttingDown`]으로 거부한다.
//! - (7차 13번) API 키를 고르는 것도 등록 시도 횟수 제한에 든다.
//! - 확인 결과는 마지막 것을 쥔다. 설정 상태를 읽는 것은 CLI를 띄우지 않는다.
//!
//! # 설정 상태의 `ai` 칸 (`GET` [`crate::SETUP_PATH`])
//!
//! ```json
//! "ai": {
//!   "method": "cli",
//!   "command": "/opt/homebrew/bin/claude",
//!   "config_dir": null,
//!   "cli": "logged_in",
//!   "cli_version": "2.1.316",
//!   "subscription": "max",
//!   "legacy_auth_file": false
//! }
//! ```
//!
//! - `method`: [`AiMethod::as_str`].
//! - `command`: CLI를 골랐으면 저장된 경로. 아니면 서버가 찾은 경로([`find_claude`], 화면이 칸을 채운다).
//!   못 찾으면 `null`.
//! - `config_dir`: 저장된 전용 로그인 폴더. 없으면 `null`.
//! - `cli`: 마지막 확인의 [`cli_state`]. CLI를 고르지 않았으면 `null`.
//!   (7차) 확인이 아직 끝나지 않았으면 [`CLI_CHECKING`], 작업 폴더가 비어 있지 않아 CLI를 쓰지 않으면
//!   [`CLI_WORK_NOT_EMPTY`]다.
//! - `cli_version` · `subscription`: 로그인됐을 때만 글자(버전은 `주.부.수` 글자), 아니면 `null`.
//! - `legacy_auth_file`: 보관 폴더에 옛 [`LEGACY_AUTH_FILE`]이 있는가([`legacy_auth_file`]).
//! - 숫자 칸은 없다.

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use pi_ai::{ClaudeCliConfig, CliStatus};
use serde_json::{Value, json};

use crate::{AppError, SetupRefusal};

/// AI 연결을 고르는 경로(`POST`). 세션 · 같은 출처 · 그 컴퓨터여야 한다.
pub const SETUP_AI_PATH: &str = "/api/setup/ai";

/// 고른 AI 연결을 두는 파일 이름(보관 폴더 안). 비밀이 아니다.
/// 모양: `{"method": "api_key"}` 또는 `{"method": "cli", "command": "...", "config_dir": "..." | null}`.
pub const AI_CHOICE_FILE: &str = "ai.json";

/// 예전 구독 토큰 파일 이름(보관 폴더 안). 서버는 있는지만 보고 읽지도 · 고치지도 · 지우지도 않는다.
/// 이름은 `pi-secrets`가 정한다(키 파일 이름은 `pi-secrets`만 안다).
pub use pi_secrets::LEGACY_AUTH_FILE;

/// CLI 자식의 작업 폴더 이름(보관 폴더 안, 0700, 비어 있다).
pub const CLI_WORK_DIR: &str = "cli-work";

/// CLI에 넘기는 환경 변수의 허용 목록. 이것만 골라 `base_env`로 준다(이 순서).
pub const CLI_ENV_ALLOWLIST: [&str; 7] = [
    "HOME", "PATH", "LANG", "LC_ALL", "TMPDIR", "USER", "LOGNAME",
];

/// `PATH`에 없을 때 사용자 홈 아래에서 찾는 자리(홈에 이어 붙인다).
pub const CLI_USER_LOCATION: &str = ".local/bin/claude";

/// 사용자 홈 다음에 찾는 흔한 설치 위치(이 순서).
pub const CLI_COMMON_LOCATIONS: [&str; 2] = ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"];

/// CLI 공급자의 동시 실행 수.
pub const CLI_CONCURRENCY: usize = 2;

/// CLI를 고르면 화면이 보여 주는 한 줄 알림.
pub const CLI_TERMS_NOTICE: &str = "구독 약관은 자동화된 이용과 증권 매매 목적 이용을 제한할 수 있습니다. 판단은 사용자에게 있습니다.";

/// 옛 [`LEGACY_AUTH_FILE`]이 있으면 화면이 AI 줄 아래에 보여 주는 한 줄 알림.
pub const LEGACY_AUTH_NOTICE: &str =
    "예전 구독 토큰 파일(auth.json)은 더 쓰지 않습니다. 필요 없으면 지워도 됩니다.";

/// 설정 상태 `ai.cli`: 설치돼 있고 구독으로 로그인돼 있다.
pub const CLI_LOGGED_IN: &str = "logged_in";
/// 설정 상태 `ai.cli`: 설치돼 있지 않다.
pub const CLI_NOT_INSTALLED: &str = "not_installed";
/// 설정 상태 `ai.cli`: 버전이 너무 낮다.
pub const CLI_TOO_OLD: &str = "too_old";
/// 설정 상태 `ai.cli`: 로그인돼 있지 않다(터미널에서 `claude auth login`).
pub const CLI_NOT_LOGGED_IN: &str = "not_logged_in";
/// 설정 상태 `ai.cli`: 로그인은 됐지만 구독이 아니다(API 키로 로그인돼 있다).
pub const CLI_NOT_SUBSCRIPTION: &str = "not_subscription";
/// 설정 상태 `ai.cli`: 확인하지 못했다(시간 초과 · 읽지 못함).
pub const CLI_UNCHECKED: &str = "unchecked";
/// (7차 11번) 설정 상태 `ai.cli`: CLI 확인이 아직 끝나지 않았다(확인 중).
/// 서버는 CLI 확인을 기다리지 않고 먼저 연결을 받는다. 확인은 뒤에서 하고, 끝나기 전에는 이 상태다.
pub const CLI_CHECKING: &str = "checking";
/// (7차 10번) 설정 상태 `ai.cli`: 이미 있는 작업 폴더([`CLI_WORK_DIR`])가 비어 있지 않아 CLI를 쓰지 않는다.
/// 그 폴더 안의 것은 지우지 않는다.
pub const CLI_WORK_NOT_EMPTY: &str = "work_dir_not_empty";

/// AI 연결 방식.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiMethod {
    /// API 키(`pi_ai::AnthropicProvider`).
    ApiKey,
    /// 공식 Claude Code CLI(`pi_ai::ClaudeCliProvider`, 구독).
    Cli,
}

impl AiMethod {
    /// 본문 · 파일 · 설정 상태에 적는 글자: `api_key` · `cli`.
    pub const fn as_str(self) -> &'static str {
        match self {
            AiMethod::ApiKey => "api_key",
            AiMethod::Cli => "cli",
        }
    }
}

/// 사용자가 고른 AI 연결. 비밀이 아니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiChoice {
    /// 방식.
    pub method: AiMethod,
    /// CLI 실행 파일의 절대 경로(CLI일 때만).
    pub command: Option<PathBuf>,
    /// 전용 로그인 폴더의 절대 경로(CLI일 때만, 없으면 평소 로그인).
    pub config_dir: Option<PathBuf>,
}

impl AiChoice {
    /// API 키를 고른 것. 고른 것이 없을 때의 값이다.
    pub fn api_key() -> Self {
        Self {
            method: AiMethod::ApiKey,
            command: None,
            config_dir: None,
        }
    }

    /// 고르기 요청 본문을 읽는다(모양은 모듈 문서). 거르는 것:
    /// JSON 객체가 아님 · 모르는 `method` · 모르는 칸 · `api_key`에 딸린 칸 ·
    /// CLI인데 `command`가 없거나 글자가 아니거나 비었거나 절대 경로가 아님 ·
    /// `config_dir`가 절대 경로가 아님(빈 글자와 `null`은 없는 것) · 제어 문자가 든 경로.
    /// 모두 [`SetupRefusal::InvalidInput`]이다.
    pub fn parse(body: &[u8]) -> Result<AiChoice, SetupRefusal> {
        let invalid = SetupRefusal::InvalidInput;
        let value: Value = serde_json::from_slice(body).map_err(|_| invalid)?;
        let fields = value.as_object().ok_or(invalid)?;
        let method = fields
            .get("method")
            .and_then(Value::as_str)
            .ok_or(invalid)?;
        let allowed: &[&str] = match method {
            "api_key" => &["method"],
            "cli" => &["method", "command", "config_dir"],
            _ => return Err(invalid),
        };
        if fields.keys().any(|name| !allowed.contains(&name.as_str())) {
            return Err(invalid);
        }
        if method == "api_key" {
            return Ok(AiChoice::api_key());
        }
        let command = match fields.get("command") {
            Some(Value::String(text)) if !text.is_empty() => absolute(text)?,
            _ => return Err(invalid),
        };
        let config_dir = match fields.get("config_dir") {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) if text.is_empty() => None,
            Some(Value::String(text)) => Some(absolute(text)?),
            Some(_) => return Err(invalid),
        };
        Ok(AiChoice {
            method: AiMethod::Cli,
            command: Some(command),
            config_dir,
        })
    }

    /// [`AI_CHOICE_FILE`]에 쓰는 모양.
    pub fn to_json(&self) -> Value {
        match self.method {
            AiMethod::ApiKey => json!({ "method": AiMethod::ApiKey.as_str() }),
            AiMethod::Cli => json!({
                "method": AiMethod::Cli.as_str(),
                "command": self.command.as_deref().and_then(Path::to_str),
                "config_dir": self.config_dir.as_deref().and_then(Path::to_str),
            }),
        }
    }
}

/// 설정 상태의 `ai` 칸(모양은 모듈 문서). 비밀 값은 없다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStatus {
    /// 고른 방식.
    pub method: AiMethod,
    /// CLI를 골랐으면 저장된 경로, 아니면 서버가 찾은 경로([`find_claude`]). 없으면 `None`.
    pub command: Option<PathBuf>,
    /// 저장된 전용 로그인 폴더.
    pub config_dir: Option<PathBuf>,
    /// 마지막 CLI 확인 결과. CLI를 고르지 않았거나 아직 확인 중이면 `None`.
    pub cli: Option<CliStatus>,
    /// (7차 10번) 작업 폴더가 비어 있지 않아 CLI를 쓰지 않는다.
    pub work_dir_not_empty: bool,
    /// 보관 폴더에 옛 [`LEGACY_AUTH_FILE`]이 있는가.
    pub legacy_auth_file: bool,
}

impl AiStatus {
    /// 설정 상태의 `ai` 칸(모듈 문서의 모양 그대로). 숫자 칸은 없다.
    pub fn to_json(&self) -> Value {
        let logged_in = match &self.cli {
            Some(CliStatus::LoggedIn {
                version,
                subscription_type,
                ..
            }) => Some((version, subscription_type)),
            _ => None,
        };
        json!({
            "method": self.method.as_str(),
            "command": self.command.as_deref().and_then(Path::to_str),
            "config_dir": self.config_dir.as_deref().and_then(Path::to_str),
            "cli": self.cli_text(),
            "cli_version": logged_in.map(|(v, _)| format!("{}.{}.{}", v.major, v.minor, v.patch)),
            "subscription": logged_in.and_then(|(_, kind)| kind.clone()),
            "legacy_auth_file": self.legacy_auth_file,
        })
    }

    /// `ai.cli` 글자: CLI가 아니면 없음, 작업 폴더가 비어 있지 않으면 [`CLI_WORK_NOT_EMPTY`],
    /// 확인 결과가 아직 없으면 [`CLI_CHECKING`], 아니면 [`cli_state`].
    fn cli_text(&self) -> Option<&'static str> {
        if self.method != AiMethod::Cli {
            return None;
        }
        if self.work_dir_not_empty {
            return Some(CLI_WORK_NOT_EMPTY);
        }
        Some(self.cli.as_ref().map_or(CLI_CHECKING, cli_state))
    }
}

/// 보관 폴더의 [`AI_CHOICE_FILE`]에서 고른 것을 읽는다. 파일이 없으면 `None`.
/// 읽지 못하거나 읽을 수 없는 모양이면(`InvalidData`) [`AppError::BadFile`]이고, 오류에는
/// 파일 이름([`AI_CHOICE_FILE`])만 있다(7차 12번, 파일의 값은 넣지 않는다).
pub fn load_ai_choice(home: &Path) -> Result<Option<AiChoice>, AppError> {
    let bad = |kind| AppError::BadFile {
        file: AI_CHOICE_FILE,
        kind,
    };
    let bytes = match std::fs::read(home.join(AI_CHOICE_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(bad(error.kind())),
    };
    // 읽을 수 없는 모양이면 말없이 API 키로 넘어가지 않고 알린다(부른 쪽이 정한다).
    AiChoice::parse(&bytes)
        .map(Some)
        .map_err(|_| bad(ErrorKind::InvalidData))
}

/// 고른 것을 보관 폴더의 [`AI_CHOICE_FILE`]에 원자 쓰기로 둔다(임시 파일은 남지 않는다).
pub fn save_ai_choice(home: &Path, choice: &AiChoice) -> Result<(), AppError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let text = choice.to_json().to_string();
    // 함께 도는 저장이 같은 임시 파일을 쓰지 않게 이름을 다르게 한다.
    let temp = home.join(format!(
        ".{AI_CHOICE_FILE}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = write_new(&temp, text.as_bytes())
        .and_then(|()| std::fs::rename(&temp, home.join(AI_CHOICE_FILE)));
    written.map_err(|error| {
        // 쓰다 만 임시 파일을 남기지 않는다(지우지 못해도 더 할 수 있는 일이 없다).
        let _ = std::fs::remove_file(&temp);
        AppError::Home { kind: error.kind() }
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

/// 절대 경로이고 제어 문자가 없는 글자만 경로로 받는다.
fn absolute(text: &str) -> Result<PathBuf, SetupRefusal> {
    let path = Path::new(text);
    if !path.is_absolute() || text.chars().any(char::is_control) {
        return Err(SetupRefusal::InvalidInput);
    }
    Ok(path.to_path_buf())
}

/// 환경에서 [`CLI_ENV_ALLOWLIST`]의 이름만 골라 그 순서로 준다. 없는 이름은 뺀다.
/// (58차) 윈도에서는 사용자 폴더 · 시스템 폴더 이름을 더한다([`crate::cli_env_names`]).
pub fn cli_base_env(env: &dyn Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    crate::platform::cli_env_names(crate::platform::this_os())
        .into_iter()
        .filter_map(|name| env(name).map(|value| (name.to_string(), value)))
        .collect()
}

/// CLI 실행 파일을 찾는다: `path_var`(유닉스는 `:`, 윈도는 `;`로 나눈 폴더들)의 `claude`(윈도는 `claude.exe` ·
/// `claude.cmd`) → `user_home`/[`CLI_USER_LOCATION`](윈도는 `.local\bin\claude.exe`) → `fixed`(보통
/// [`CLI_COMMON_LOCATIONS`]) 순서. 실행할 수 있는 파일만 본다. 없으면 `None`. 아무것도 띄우지 않는다.
/// (7차 13번) `path_var`의 상대 경로 항목(`.` · `bin` · `../x` 등)은 후보가 아니다
/// (서버의 작업 폴더에 따라 다른 파일을 가리키게 된다).
pub fn find_claude(
    path_var: Option<&str>,
    user_home: Option<&Path>,
    fixed: &[PathBuf],
) -> Option<PathBuf> {
    let os = crate::platform::this_os();
    let on_path = path_var
        .into_iter()
        .flat_map(|var| crate::platform::claude_path_candidates(os, var))
        .map(PathBuf::from);
    let in_home = user_home.map(|home| home.join(crate::platform::claude_user_location(os)));
    on_path
        .chain(in_home)
        .chain(fixed.iter().cloned())
        .find(|candidate| crate::platform::is_runnable_file(candidate))
}

/// CLI 자식의 작업 폴더(보관 폴더/[`CLI_WORK_DIR`])를 준비한다: 없으면 만들고, 권한은 0700이다.
///
/// (7차 10번) 이미 있는 폴더는 비어 있어야 한다. 안에 무엇이 있으면 지우지 않고
/// [`AppError::Home`](`DirectoryNotEmpty`)로 거부한다(권한도 바꾸지 않는다).
/// (8차 1번) 다만 `pi-ai`의 자기 임시 파일(`pi_ai::is_own_work_file`이 참인 보통 파일)만 있으면
/// 비어 있는 것으로 본다. 이 함수는 그 파일을 지우지 않는다(도는 호출의 것일 수 있다).
/// 막히는 파일 작업이므로 비동기 자리에서는 `spawn_blocking`으로 부른다.
pub fn prepare_cli_work_dir(home: &Path) -> Result<PathBuf, AppError> {
    prepare_work_dir(home, Leftovers::Keep)
}

/// (8차 1번) 작업 폴더에 남은 `pi-ai` 자기 임시 파일을 어떻게 하는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Leftovers {
    /// 그대로 둔다(호출이 돌 수 있을 때: 다시 고르기).
    Keep,
    /// 지운다(호출이 하나도 돌지 않을 때: 서버가 시작할 때). 다른 항목이 있으면 아무것도 지우지 않는다.
    Remove,
}

/// [`prepare_cli_work_dir`]와 같고, 자기 임시 파일만 있을 때 `leftovers`대로 한다.
pub(crate) fn prepare_work_dir(home: &Path, leftovers: Leftovers) -> Result<PathBuf, AppError> {
    let work = home.join(CLI_WORK_DIR);
    let fail = |error: std::io::Error| AppError::Home { kind: error.kind() };
    let existed = match crate::platform::create_private_dir(&work) {
        Ok(()) => false,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => true,
        Err(error) => return Err(fail(error)),
    };
    // 이미 있으면 폴더인지 보고(링크는 따라가지 않는다), 비었는지 보고, 권한을 본인만으로 좁힌다.
    let meta = std::fs::symlink_metadata(&work).map_err(fail)?;
    if !meta.is_dir() {
        return Err(AppError::Home {
            kind: ErrorKind::InvalidData,
        });
    }
    if existed {
        let own = own_files_only(&work).map_err(fail)?.ok_or(AppError::Home {
            kind: ErrorKind::DirectoryNotEmpty,
        })?;
        if leftovers == Leftovers::Remove {
            for path in own {
                // 지우지 못한 남은 파일은 이름이 겹치지 않으므로 다음 호출을 막지 않는다.
                if let Err(error) = std::fs::remove_file(&path)
                    && error.kind() != ErrorKind::NotFound
                {
                    tracing::warn!(kind = %error.kind(), "작업 폴더의 남은 임시 파일을 지우지 못했습니다");
                }
            }
        }
    }
    crate::platform::narrow_private_dir(&work, &meta).map_err(fail)?;
    Ok(work)
}

/// 폴더 안의 항목이 모두 `pi-ai` 자기 임시 파일(이름이 맞는 보통 파일, 링크 · 폴더는 아니다)이면
/// 그 경로들, 다른 항목이 하나라도 있으면 `None`. 아무것도 지우지 않는다.
fn own_files_only(work: &Path) -> std::io::Result<Option<Vec<PathBuf>>> {
    let mut own = Vec::new();
    for entry in std::fs::read_dir(work)? {
        let entry = entry?;
        if !(entry.file_type()?.is_file() && pi_ai::is_own_work_file(&entry.file_name())) {
            return Ok(None);
        }
        own.push(entry.path());
    }
    Ok(Some(own))
}

/// 고른 CLI 연결로 공급자 설정을 만든다: 동시 실행 [`CLI_CONCURRENCY`],
/// 시간 제한은 `timeout`(AI 요청 시간 제한과 같은 값), 환경은 `base_env`뿐.
pub fn cli_config(
    choice: &AiChoice,
    work_dir: PathBuf,
    base_env: Vec<(String, String)>,
    timeout: Duration,
) -> ClaudeCliConfig {
    let mut config = ClaudeCliConfig::new(choice.command.clone(), work_dir, base_env, timeout);
    config.config_dir = choice.config_dir.clone();
    config.max_concurrency = CLI_CONCURRENCY;
    config
}

/// 확인 결과를 설정 상태의 `ai.cli` 글자로 옮긴다([`CLI_LOGGED_IN`] 등).
/// 시간 초과와 읽지 못함은 모두 [`CLI_UNCHECKED`]다.
pub fn cli_state(status: &CliStatus) -> &'static str {
    match status {
        CliStatus::LoggedIn { .. } => CLI_LOGGED_IN,
        CliStatus::NotInstalled => CLI_NOT_INSTALLED,
        CliStatus::TooOld { .. } => CLI_TOO_OLD,
        CliStatus::NotLoggedIn => CLI_NOT_LOGGED_IN,
        CliStatus::NotSubscription { .. } => CLI_NOT_SUBSCRIPTION,
        CliStatus::TimedOut | CliStatus::Unreadable => CLI_UNCHECKED,
    }
}

/// 확인 결과가 "등록됨"이 아니면 그 까닭. 로그인된 구독이면 `None`.
///
/// | 확인 결과 | 까닭 |
/// |---|---|
/// | `NotInstalled` | [`SetupRefusal::CliNotInstalled`] |
/// | `TooOld` | [`SetupRefusal::CliTooOld`] |
/// | `NotLoggedIn` | [`SetupRefusal::CliNotLoggedIn`] |
/// | `NotSubscription` | [`SetupRefusal::CliNotSubscription`] |
/// | `TimedOut` · `Unreadable` | [`SetupRefusal::CliUnchecked`] |
pub fn cli_refusal(status: &CliStatus) -> Option<SetupRefusal> {
    match status {
        CliStatus::LoggedIn { .. } => None,
        CliStatus::NotInstalled => Some(SetupRefusal::CliNotInstalled),
        CliStatus::TooOld { .. } => Some(SetupRefusal::CliTooOld),
        CliStatus::NotLoggedIn => Some(SetupRefusal::CliNotLoggedIn),
        CliStatus::NotSubscription { .. } => Some(SetupRefusal::CliNotSubscription),
        CliStatus::TimedOut | CliStatus::Unreadable => Some(SetupRefusal::CliUnchecked),
    }
}

/// 보관 폴더에 옛 [`LEGACY_AUTH_FILE`]이 있는가. 있는지만 본다(열지 · 읽지 · 고치지 · 지우지 않는다).
/// 읽을 수 없는 권한의 파일이어도 있으면 참이다.
pub fn legacy_auth_file(home: &Path) -> bool {
    // 이름의 자리만 본다(링크를 따라가지 않는다). 파일을 열지 않는다.
    pi_secrets::legacy_auth_file_present(home)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: &str = ".pi-ai-system-1-0.txt";

    fn work_with(entries: &[&str]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join(CLI_WORK_DIR);
        std::fs::create_dir(&work).unwrap();
        for name in entries {
            std::fs::write(work.join(name), "x").unwrap();
        }
        (dir, work)
    }

    #[test]
    fn 자기_임시_파일만_있으면_비어_있음이고_지우기는_고른_대로다() {
        let (dir, work) = work_with(&[OWN]);
        prepare_work_dir(dir.path(), Leftovers::Keep).unwrap();
        assert!(work.join(OWN).exists(), "두기면 지우지 않는다");
        prepare_work_dir(dir.path(), Leftovers::Remove).unwrap();
        assert!(!work.join(OWN).exists(), "지우기면 지운다");
    }

    #[cfg(unix)]
    #[test]
    fn 이름이_맞아도_폴더나_링크면_다른_항목이다() {
        let (dir, work) = work_with(&[]);
        std::fs::create_dir(work.join(OWN)).unwrap();
        assert!(prepare_work_dir(dir.path(), Leftovers::Remove).is_err());
        assert!(work.join(OWN).is_dir(), "아무것도 지우지 않는다");

        let (dir, work) = work_with(&[]);
        std::os::unix::fs::symlink(dir.path(), work.join(OWN)).unwrap();
        assert!(prepare_work_dir(dir.path(), Leftovers::Remove).is_err());
        assert!(std::fs::symlink_metadata(work.join(OWN)).is_ok());
    }
}
