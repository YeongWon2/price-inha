//! (58차) 운영체제마다 다른 자리: 자식을 자기 묶음으로 띄우기 · 묶음에 신호 보내기 · 0600 파일 · 출력 끝 사본.
//!
//! 유닉스는 프로세스 그룹과 시스템 `kill`을 쓴다(ai.md 2차 9 · 6차 1). 윈도에는 그룹 신호가 없으므로 새 프로세스
//! 묶음(`CREATE_NEW_PROCESS_GROUP`, 창 없이)으로 띄우고 `taskkill /T`(나무째)로 끝낸다. 윈도에서는 프로세스 번호가
//! 곧 다시 쓰이므로 **거둔 뒤에는 그 번호에 아무것도 보내지 않는다**([`SIGNAL_AFTER_REAP`]), 살아 있는지 묻는
//! 신호(`0`)도 없다(없음으로 본다). 새 의존(윈도 API 크레이트) 없이 표준 라이브러리 · tokio의 API만 쓴다.

use std::fs::OpenOptions;

use tokio::process::{Child, Command};

/// 그룹에 신호를 보내는 시스템 명령.
#[cfg(unix)]
pub(super) const KILL_COMMAND: &str = "/bin/kill";
/// 프로세스 나무를 끝내는 시스템 명령(윈도).
#[cfg(not(unix))]
pub(super) const KILL_COMMAND: &str = "taskkill";

/// 자식을 거둔 뒤에도 남은 손주를 끝내려고 그 묶음에 신호를 보내는가(유닉스만 — 그룹 번호는 그룹이 비기 전에는
/// 다시 쓰이지 않는다. 윈도의 프로세스 번호는 그런 보장이 없다).
pub(super) const SIGNAL_AFTER_REAP: bool = cfg!(unix);

/// 신호 명령의 인자. 그 운영체제에서 보낼 수 없는 신호면 `None`(보내지 못한 것으로 본다).
/// `windows`가 참이면 윈도 `taskkill`의 인자다(시험이 지금 컴퓨터에서 윈도 갈래를 본다).
pub(super) fn kill_args_for(windows: bool, signal: &str, pgid: u32) -> Option<Vec<String>> {
    let pid = pgid.to_string();
    if !windows {
        return Some(vec![
            format!("-{signal}"),
            "--".to_string(),
            format!("-{pid}"),
        ]);
    }
    let mut args = vec!["/PID".to_string(), pid, "/T".to_string()];
    match signal {
        "TERM" => {}
        "KILL" => args.push("/F".to_string()),
        _ => return None,
    }
    Some(args)
}

/// 이 운영체제의 신호 명령 인자.
pub(super) fn kill_args(signal: &str, pgid: u32) -> Option<Vec<String>> {
    kill_args_for(cfg!(windows), signal, pgid)
}

/// 자식을 자기 묶음으로 띄우게 한다(유닉스: 새 프로세스 그룹, 윈도: 새 프로세스 묶음 · 창 없이).
pub(super) fn own_group(cmd: &mut Command) {
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    {
        /// `CREATE_NEW_PROCESS_GROUP` | `CREATE_NO_WINDOW`.
        const FLAGS: u32 = 0x0000_0200 | 0x0800_0000;
        cmd.creation_flags(FLAGS);
    }
}

/// 새 파일을 본인만 읽게 연다(유닉스 0600). 윈도에는 그 비트가 없다 — 작업 폴더가 사용자 프로필 아래에 있다.
pub(super) fn private_file(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = options;
}

/// 표준 출력 · 표준 오류 읽는 끝의 사본(유닉스: 읽기를 그만둔 뒤에도 자식이 `SIGPIPE`로 먼저 죽지 않게).
#[cfg(unix)]
pub(super) type HeldPipe = std::os::fd::OwnedFd;
/// 윈도에는 `SIGPIPE`가 없어 사본을 쥐지 않는다.
#[cfg(not(unix))]
pub(super) type HeldPipe = ();

/// 출력 읽는 끝의 사본들.
#[cfg(unix)]
pub(super) fn held_outputs(child: &Child) -> Vec<HeldPipe> {
    use std::os::fd::AsFd;
    let out = child
        .stdout
        .as_ref()
        .map(|p| p.as_fd().try_clone_to_owned());
    let err = child
        .stderr
        .as_ref()
        .map(|p| p.as_fd().try_clone_to_owned());
    [out, err]
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .collect()
}

/// 윈도: 쥐지 않는다.
#[cfg(not(unix))]
pub(super) fn held_outputs(_child: &Child) -> Vec<HeldPipe> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(list: &[&str]) -> Option<Vec<String>> {
        Some(list.iter().map(|s| (*s).to_string()).collect())
    }

    #[test]
    fn 유닉스는_그룹_전체에_kill을_보낸다() {
        assert_eq!(
            kill_args_for(false, "TERM", 42),
            strings(&["-TERM", "--", "-42"])
        );
        assert_eq!(
            kill_args_for(false, "KILL", 42),
            strings(&["-KILL", "--", "-42"])
        );
        assert_eq!(kill_args_for(false, "0", 42), strings(&["-0", "--", "-42"]));
    }

    #[test]
    fn 윈도는_taskkill로_나무째_끝내고_살아_있는지는_묻지_않는다() {
        assert_eq!(
            kill_args_for(true, "TERM", 42),
            strings(&["/PID", "42", "/T"])
        );
        assert_eq!(
            kill_args_for(true, "KILL", 42),
            strings(&["/PID", "42", "/T", "/F"])
        );
        assert_eq!(kill_args_for(true, "0", 42), None);
        assert_eq!(kill_args_for(true, "HUP", 42), None);
    }
}
