//! (65차, 9차 보강 4) macOS에서 계속 죽는 서비스 알아보기: `launchctl print gui/<uid>/<label>`의 답에서
//! `last exit code = N[: 이름]`을 읽는다. 한 번도 끝나지 않았으면(`(never exited)`) 없음.
//!
//! launchd는 `KeepAlive={SuccessfulExit:false}` · `ThrottleInterval=30`으로 다시 띄우므로, 계속 죽는 서버는
//! "도는 중이 아님(spawn scheduled · waiting 등) + 마지막 종료 코드가 0이 아님"으로 보인다.

/// `launchctl print`의 답에서 마지막 종료 코드.
pub(super) fn launchd_last_exit(printed: &str) -> Option<i32> {
    printed.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("last exit code")?;
        let value = rest.trim_start().strip_prefix('=')?.trim();
        let digits: String = value
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        digits.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 마지막_종료_코드를_읽는다() {
        assert_eq!(
            launchd_last_exit("\tlast exit code = 78: EX_CONFIG\n"),
            Some(78)
        );
        assert_eq!(launchd_last_exit("last exit code = 0"), Some(0));
        assert_eq!(launchd_last_exit("last exit code = (never exited)"), None);
        assert_eq!(launchd_last_exit("state = running"), None);
    }
}
