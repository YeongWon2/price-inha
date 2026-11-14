//! 되풀이되는 실패를 다루는 작은 도구: 물러서기(다시 시도 간격 늘리기)와 기록 묶기.

use std::time::{Duration, Instant};

/// 같은 실패가 이어질 때 기록을 다시 남기는 간격.
pub(crate) const LOG_EVERY: Duration = Duration::from_secs(60);

/// 물러서기: 실패할 때마다 기다리는 시간을 두 배로 늘리고 상한에서 멈춘다.
#[derive(Debug)]
pub(crate) struct Backoff {
    initial: Duration,
    max: Duration,
    next: Duration,
}

impl Backoff {
    /// `initial`부터 시작해 `max`에서 멈춘다. 상한이 첫 간격보다 짧으면 첫 간격을 상한으로 본다.
    pub(crate) fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial,
            max: max.max(initial),
            next: initial,
        }
    }

    /// 이번 실패 뒤에 기다릴 시간. 부를 때마다 다음 것이 두 배가 된다(상한까지).
    pub(crate) fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = delay.saturating_mul(2).min(self.max);
        delay
    }

    /// 성공했다. 다음 실패는 다시 첫 간격부터다.
    pub(crate) fn reset(&mut self) {
        self.next = self.initial;
    }
}

/// 기록 묶기: 같은 실패가 되풀이되면 처음 한 번과 정한 간격마다만 기록하게 한다.
///
/// 실패의 종류가 바뀌면(다른 `what`) 바로 기록한다. 풀리면([`LogGate::clear`]) 다음 실패는 다시 처음이다.
#[derive(Debug)]
pub(crate) struct LogGate {
    every: Duration,
    last: Option<(String, Instant)>,
}

impl LogGate {
    /// 같은 실패는 `every`마다 한 번만 기록하게 한다.
    pub(crate) fn new(every: Duration) -> Self {
        Self { every, last: None }
    }

    /// 이 실패(`what`)를 지금(`now`) 기록할 것인가. 참을 돌려주면 기록한 것으로 친다.
    pub(crate) fn should_log(&mut self, what: &str, now: Instant) -> bool {
        let repeated = self.last.as_ref().is_some_and(|(last, at)| {
            last == what && now.saturating_duration_since(*at) < self.every
        });
        if !repeated {
            self.last = Some((what.to_owned(), now));
        }
        !repeated
    }

    /// 실패가 풀렸다. 이어지던 실패가 있었으면 참(풀렸다고 한 번 기록할 때 쓴다).
    pub(crate) fn clear(&mut self) -> bool {
        self.last.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn 물러서기는_두_배씩_늘고_상한에서_멈춘다() {
        let mut backoff = Backoff::new(100 * MS, 400 * MS);
        let delays: Vec<Duration> = (0..6).map(|_| backoff.next_delay()).collect();
        assert_eq!(
            delays,
            [100 * MS, 200 * MS, 400 * MS, 400 * MS, 400 * MS, 400 * MS]
        );
    }

    #[test]
    fn 성공하면_첫_간격으로_돌아온다() {
        let mut backoff = Backoff::new(100 * MS, 400 * MS);
        backoff.next_delay();
        backoff.next_delay();
        backoff.reset();
        assert_eq!(backoff.next_delay(), 100 * MS);
    }

    #[test]
    fn 상한이_두_배의_배수가_아니어도_상한을_넘지_않는다() {
        let mut backoff = Backoff::new(100 * MS, 250 * MS);
        let delays: Vec<Duration> = (0..4).map(|_| backoff.next_delay()).collect();
        assert_eq!(delays, [100 * MS, 200 * MS, 250 * MS, 250 * MS]);
    }

    #[test]
    fn 상한이_첫_간격보다_짧으면_첫_간격에서_멈춘다() {
        let mut backoff = Backoff::new(500 * MS, 100 * MS);
        assert_eq!(backoff.next_delay(), 500 * MS);
        assert_eq!(backoff.next_delay(), 500 * MS);
    }

    #[test]
    fn 아주_큰_간격도_넘치지_않는다() {
        let mut backoff = Backoff::new(Duration::MAX, Duration::MAX);
        assert_eq!(backoff.next_delay(), Duration::MAX);
        assert_eq!(backoff.next_delay(), Duration::MAX);
    }

    #[test]
    fn 되풀이되는_같은_실패는_기록_횟수가_묶여_있다() {
        let mut gate = LogGate::new(Duration::from_secs(60));
        let start = Instant::now();
        // 2초마다 10분 동안 같은 실패: 300번 실패에 기록은 1분에 한 번, 열 번이다.
        let logged = (0..300u32)
            .filter(|n| gate.should_log("깨진 키 파일", start + Duration::from_secs(2) * *n))
            .count();
        assert_eq!(logged, 10);
    }

    #[test]
    fn 실패의_종류가_바뀌면_바로_기록한다() {
        let mut gate = LogGate::new(Duration::from_secs(60));
        let now = Instant::now();
        assert!(gate.should_log("거절", now));
        assert!(!gate.should_log("거절", now + Duration::from_secs(1)));
        assert!(gate.should_log("닿지 않음", now + Duration::from_secs(2)));
        assert!(!gate.should_log("닿지 않음", now + Duration::from_secs(3)));
        assert!(gate.should_log("거절", now + Duration::from_secs(4)));
    }

    #[test]
    fn 풀렸다가_다시_실패하면_다시_기록한다() {
        let mut gate = LogGate::new(Duration::from_secs(60));
        let now = Instant::now();
        assert!(!gate.clear(), "실패한 적이 없으면 풀린 것도 없다");
        assert!(gate.should_log("거절", now));
        assert!(gate.clear());
        assert!(!gate.clear());
        assert!(gate.should_log("거절", now + Duration::from_secs(1)));
    }
}
