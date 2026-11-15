//! 설치 · 로그인 확인(ai.md 2차 11, `cli-backend.md` 5.7). 모델을 부르지 않는다.
//!
//! 확인이 제한을 넘기면 기다리기만 그만두고 프로세스는 죽이지 않는다(토큰 갱신 잠금 — 원본 2.6).

use std::io;
use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;

use serde_json::Value;
use tokio::process::Command;

use super::command;
use super::{CLI_MIN_VERSION, ClaudeCliConfig, CliStatus, CliVersion};

/// 구독 로그인의 `authMethod` 값.
const SUBSCRIPTION_METHOD: &str = "claude.ai";

/// 확인 명령 하나의 결과.
enum Probe {
    Done(Output),
    Missing,
    Failed,
    TimedOut,
}

/// 버전과 로그인을 차례로 확인한다.
pub(super) async fn check(config: &ClaudeCliConfig) -> CliStatus {
    let Some(program) = &config.command else {
        return CliStatus::NotInstalled;
    };
    let version = match probe(config, program, &["--version"], config.version_timeout).await {
        Probe::Done(out) if out.status.success() => match parse_version(&out.stdout) {
            Some(v) => v,
            None => return CliStatus::Unreadable,
        },
        Probe::Done(_) | Probe::Failed => return CliStatus::Unreadable,
        Probe::Missing => return CliStatus::NotInstalled,
        Probe::TimedOut => return CliStatus::TimedOut,
    };
    if version < CLI_MIN_VERSION {
        return CliStatus::TooOld { version };
    }
    let args = ["auth", "status", "--json"];
    match probe(config, program, &args, config.auth_timeout).await {
        Probe::Done(out) => auth_status(&out, version),
        Probe::Failed => CliStatus::Unreadable,
        Probe::Missing => CliStatus::NotInstalled,
        Probe::TimedOut => CliStatus::TimedOut,
    }
}

/// 확인 명령을 실제 호출과 같은 환경 · 작업 폴더로 띄운다. 제한을 넘기면 놓아둔다.
async fn probe(config: &ClaudeCliConfig, program: &Path, args: &[&str], limit: Duration) -> Probe {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env_clear()
        .envs(command::child_env(config, None))
        .current_dir(&config.work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Probe::Missing,
        Err(_) => return Probe::Failed,
    };
    match tokio::time::timeout(limit, child.wait_with_output()).await {
        Ok(Ok(out)) => Probe::Done(out),
        Ok(Err(_)) => Probe::Failed,
        Err(_) => Probe::TimedOut,
    }
}

/// 첫 낱말을 `주.부.수`로 읽는다.
fn parse_version(stdout: &[u8]) -> Option<CliVersion> {
    let text = std::str::from_utf8(stdout).ok()?;
    let word = text.split_whitespace().next()?;
    let mut parts = word.split('.').map(|p| p.parse::<u32>().ok());
    let (Some(Some(major)), Some(Some(minor)), Some(Some(patch)), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    Some(CliVersion {
        major,
        minor,
        patch,
    })
}

fn auth_status(out: &Output, version: CliVersion) -> CliStatus {
    match out.status.code() {
        Some(1) => return CliStatus::NotLoggedIn,
        Some(0) => {}
        _ => return CliStatus::Unreadable,
    }
    let Ok(value) = serde_json::from_slice::<Value>(&out.stdout) else {
        return CliStatus::Unreadable;
    };
    match value.get("loggedIn").and_then(Value::as_bool) {
        Some(true) => {}
        Some(false) => return CliStatus::NotLoggedIn,
        None => return CliStatus::Unreadable,
    }
    let Some(auth_method) = value.get("authMethod").and_then(Value::as_str) else {
        return CliStatus::Unreadable;
    };
    if auth_method != SUBSCRIPTION_METHOD {
        return CliStatus::NotSubscription {
            auth_method: auth_method.to_string(),
        };
    }
    CliStatus::LoggedIn {
        version,
        auth_method: auth_method.to_string(),
        subscription_type: value
            .get("subscriptionType")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 버전은_세_수만_읽는다() {
        assert_eq!(
            parse_version(b"2.1.316 (Claude Code)\n"),
            Some(CliVersion {
                major: 2,
                minor: 1,
                patch: 316,
            })
        );
        assert_eq!(parse_version(b"2.1"), None);
        assert_eq!(parse_version(b"2.1.3.4"), None);
        assert_eq!(parse_version(b"v2.1.3"), None);
        assert_eq!(parse_version(b""), None);
    }
}
