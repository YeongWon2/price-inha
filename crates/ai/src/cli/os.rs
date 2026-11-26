//! (58차) 운영체제마다 다른 자리: 자식을 자기 묶음으로 띄우기 · 묶음에 신호 보내기 · 0600 파일 · 출력 끝 사본.
//!
//! 유닉스는 프로세스 그룹과 시스템 `kill`을 쓴다(ai.md 2차 9 · 6차 1). 윈도에는 그룹 신호가 없으므로 새 프로세스
//! 묶음(`CREATE_NEW_PROCESS_GROUP`, 창 없이)으로 띄우고 `taskkill /T`(나무째)로 끝낸다. 윈도에서는 프로세스 번호가
//! 곧 다시 쓰이므로 **거둔 뒤에는 그 번호에 아무것도 보내지 않는다**([`SIGNAL_AFTER_REAP`]), 살아 있는지 묻는
//! 신호(`0`)도 없다(없음으로 본다). 새 의존(윈도 API 크레이트) 없이 표준 라이브러리 · tokio의 API만 쓴다.

/// 그룹에 신호를 보내는 시스템 명령.
pub(super) const KILL_COMMAND: &str = "/bin/kill";

/// 자식을 거둔 뒤에도 남은 손주를 끝내려고 그 묶음에 신호를 보내는가(유닉스만 — 그룹 번호는 그룹이 비기 전에는
/// 다시 쓰이지 않는다. 윈도의 프로세스 번호는 그런 보장이 없다).
pub(super) const SIGNAL_AFTER_REAP: bool = cfg!(unix);

/// 신호 명령의 인자. 그 운영체제에서 보낼 수 없는 신호면 `None`(보내지 못한 것으로 본다).
/// `windows`가 참이면 윈도 `taskkill`의 인자다(시험이 지금 컴퓨터에서 윈도 갈래를 본다).
pub(super) fn kill_args_for(_windows: bool, _signal: &str, _pgid: u32) -> Option<Vec<String>> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(list: &[&str]) -> Option<Vec<String>> {
        Some(list.iter().map(|s| (*s).to_string()).collect())
    }

    #[test]
    fn 유닉스는_그룹_전체에_kill을_보낸다() {
        assert_eq!(kill_args_for(false, "TERM", 42), strings(&["-TERM", "--", "-42"]));
        assert_eq!(kill_args_for(false, "KILL", 42), strings(&["-KILL", "--", "-42"]));
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
