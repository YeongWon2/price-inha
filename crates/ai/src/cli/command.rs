//! 한 번 호출에 넘기는 것: 인자 묶음 · 자식 환경 · 표준 입력 글 · 시스템 글 임시 파일(ai.md 2차 3~5).

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{CLI_RESTRICTED_SETTINGS, ClaudeCliConfig};
use crate::types::{Message, Role};

/// 시스템 글 파일 이름을 호출마다 다르게 만드는 번호.
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

/// 대화 둘 이상일 때 끝에 붙이는 한 줄.
const CONTINUE_LINE: &str = "마지막 사용자 요청에 이어서 답한다.";

/// `base_env`에 있어도 빼는 이름의 앞머리(과금 주체를 바꾼다, `cli-backend.md` 2.2).
const BLOCKED_PREFIXES: [&str; 3] = ["ANTHROPIC_", "CLAUDE_CODE_USE_", "OTEL_"];

/// `base_env`에 있어도 빼는 이름.
const BLOCKED_NAMES: [&str; 2] = [
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
];

const GIT_INSTRUCTIONS: &str = "CLAUDE_CODE_DISABLE_GIT_INSTRUCTIONS";
const MAX_OUTPUT_TOKENS: &str = "CLAUDE_CODE_MAX_OUTPUT_TOKENS";
const CONFIG_DIR: &str = "CLAUDE_CONFIG_DIR";

/// 한 번 호출의 인자 묶음. 시스템 글 파일이 없으면 `--system-prompt-file` 두 칸만 빠진다.
pub(super) fn args(model: &str, system_file: Option<&Path>) -> Vec<OsString> {
    let mut out: Vec<OsString> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--model",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    out.push(OsString::from(model));
    if let Some(path) = system_file {
        out.push(OsString::from("--system-prompt-file"));
        out.push(path.as_os_str().to_owned());
    }
    let rest = [
        "--safe-mode",
        "--tools",
        "",
        "--disallowedTools",
        "mcp__*",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--settings",
        CLI_RESTRICTED_SETTINGS,
        "--disable-slash-commands",
        "--no-chrome",
        "--no-session-persistence",
        "--max-turns",
        "1",
        "--permission-mode",
        "dontAsk",
        "--permission-prompts",
        "none",
    ];
    out.extend(rest.iter().map(OsString::from));
    out
}

/// 자식에게 줄 환경의 전부. 부모 환경은 넣지 않는다(부르는 쪽이 비운다).
///
/// `max_tokens`가 있으면 `CLAUDE_CODE_MAX_OUTPUT_TOKENS`로 넣는다(설치 확인에는 없다).
pub(super) fn child_env(
    config: &ClaudeCliConfig,
    max_tokens: Option<u32>,
) -> Vec<(OsString, OsString)> {
    let mut env: Vec<(OsString, OsString)> = config
        .base_env
        .iter()
        .filter(|(name, _)| keep_env(name, config.config_dir.is_some()))
        .map(|(k, v)| (OsString::from(k), OsString::from(v)))
        .collect();
    env.push((GIT_INSTRUCTIONS.into(), "1".into()));
    if let Some(tokens) = max_tokens {
        env.push((MAX_OUTPUT_TOKENS.into(), tokens.to_string().into()));
    }
    if let Some(dir) = &config.config_dir {
        env.push((CONFIG_DIR.into(), dir.as_os_str().to_owned()));
    }
    env
}

fn keep_env(name: &str, has_config_dir: bool) -> bool {
    if BLOCKED_PREFIXES.iter().any(|p| name.starts_with(p)) || BLOCKED_NAMES.contains(&name) {
        return false;
    }
    if name == GIT_INSTRUCTIONS || name == MAX_OUTPUT_TOKENS {
        return false;
    }
    !(has_config_dir && name == CONFIG_DIR)
}

/// 대화를 표준 입력 글 하나로 만든다. 사용자 글 하나뿐이면 그 글 그대로.
pub(super) fn stdin_text(messages: &[Message]) -> String {
    if let [only] = messages
        && only.role == Role::User
    {
        return only.content.clone();
    }
    let mut parts: Vec<String> = messages
        .iter()
        .map(|m| {
            let head = match m.role {
                Role::User => "[사용자]",
                Role::Assistant => "[담당]",
            };
            format!("{head}\n{}", m.content)
        })
        .collect();
    parts.push(CONTINUE_LINE.to_string());
    parts.join("\n\n")
}

/// 작업 폴더 안의 0600 시스템 글 파일. 버려지면(어떤 길로 끝나도) 지운다.
pub(super) struct SystemFile {
    path: PathBuf,
}

impl SystemFile {
    /// 호출마다 다른 이름으로 새 파일을 만들어 글을 쓴다.
    pub(super) fn create(dir: &Path, text: &str) -> io::Result<Self> {
        let mut last_err = io::Error::other("시스템 글 파일을 만들지 못했다");
        for _ in 0..16 {
            let n = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
            let path = dir.join(format!(".pi-ai-system-{}-{n}.txt", std::process::id()));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(mut file) => {
                    let made = Self { path };
                    file.write_all(text.as_bytes())?;
                    file.flush()?;
                    return Ok(made);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => last_err = e,
                Err(e) => return Err(e),
            }
        }
        Err(last_err)
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SystemFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
        }
    }

    #[test]
    fn 담당_글_하나뿐이면_머리_줄을_붙인다() {
        let text = stdin_text(&[msg(Role::Assistant, "가")]);
        assert_eq!(text, format!("[담당]\n가\n\n{CONTINUE_LINE}"));
    }

    #[test]
    fn 로그인_폴더가_없으면_base_env의_claude_config_dir는_남는다() {
        assert!(keep_env(CONFIG_DIR, false));
        assert!(!keep_env(CONFIG_DIR, true));
        assert!(!keep_env("OTEL_SERVICE_NAME", false));
        assert!(keep_env("MY_OTEL", false));
    }
}
