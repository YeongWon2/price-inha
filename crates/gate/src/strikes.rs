//! 접속 키를 틀린 횟수 세기: 다른 기기는 잠그고(N3), 그 컴퓨터는 늦춘다.
//!
//! 원본(OpenClaw `auth-rate-limit.ts`)처럼 메모리에만 두고, 잠금이 풀리는 시각이 지나면 표시를
//! 지운다. 맞는 키로 들어오면 그 기기의 횟수를 지운다. 루프백은 잠그지 않고, 틀릴 때마다 응답을
//! 시작값에서 두 배씩 상한까지 늦춘다. 우리 스펙은 창 없이 "5번 틀리면 15분"이다(결정 9번).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use pi_store::UnixMillis;

use crate::access::{LOCKOUT, LockNotice, LockedDevice, MAX_KEY_FAILURES};

/// 세어 두는 기기 수의 상한(원본 10,000). 넘으면 잠기지 않은 기기부터 버린다.
const MAX_TRACKED: usize = 10_000;

#[derive(Debug, Clone, Copy, Default)]
struct Count {
    failures: u32,
    until: Option<UnixMillis>,
}

/// 기기마다 틀린 횟수와 잠금.
#[derive(Default)]
pub(crate) struct Strikes {
    remote: Mutex<HashMap<IpAddr, Count>>,
    loopback: Mutex<HashMap<IpAddr, u32>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn lockout_millis() -> i64 {
    i64::try_from(LOCKOUT.as_millis()).unwrap_or(i64::MAX)
}

impl Strikes {
    /// 그 기기가 지금 잠겨 있으면 풀리는 시각. 잠금 시간이 지났으면 표시를 지운다.
    pub(crate) fn locked_until(&self, device: IpAddr, now: UnixMillis) -> Option<UnixMillis> {
        let mut remote = lock(&self.remote);
        let until = remote.get(&device)?.until?;
        if now < until {
            Some(until)
        } else {
            remote.remove(&device);
            None
        }
    }

    /// 다른 기기가 키를 틀렸다. 이번에 잠기면 알림 내용을 돌려준다(이미 잠겨 있으면 없음).
    pub(crate) fn fail(&self, device: IpAddr, now: UnixMillis) -> Option<LockNotice> {
        let mut remote = lock(&self.remote);
        if remote.len() >= MAX_TRACKED && !remote.contains_key(&device) {
            remote.retain(|_, count| count.until.is_some_and(|until| now < until));
        }
        let count = remote.entry(device).or_default();
        if let Some(until) = count.until {
            if now < until {
                return None;
            }
            *count = Count::default();
        }
        count.failures = count.failures.saturating_add(1);
        if count.failures < MAX_KEY_FAILURES {
            return None;
        }
        let until = UnixMillis(now.0.saturating_add(lockout_millis()));
        count.until = Some(until);
        Some(LockNotice {
            device,
            failures: count.failures,
            until,
        })
    }

    /// 다른 기기가 맞는 키로 들어왔다.
    pub(crate) fn succeed(&self, device: IpAddr) {
        lock(&self.remote).remove(&device);
    }

    /// 지금 잠긴 기기들(풀리는 시각 순).
    pub(crate) fn locked(&self, now: UnixMillis) -> Vec<LockedDevice> {
        let mut remote = lock(&self.remote);
        remote.retain(|_, count| count.until.is_none_or(|until| now < until));
        let mut list: Vec<LockedDevice> = remote
            .iter()
            .filter_map(|(device, count)| {
                count.until.map(|until| LockedDevice {
                    device: *device,
                    until,
                })
            })
            .collect();
        list.sort_by_key(|locked| (locked.until, locked.device));
        list
    }

    /// 그 기기의 잠금과 횟수를 지운다. 잠겨 있었으면 참.
    pub(crate) fn unlock(&self, device: IpAddr, now: UnixMillis) -> bool {
        lock(&self.remote)
            .remove(&device)
            .and_then(|count| count.until)
            .is_some_and(|until| now < until)
    }

    /// 그 컴퓨터가 키를 틀렸다. 이번에 늦출 시간을 돌려준다.
    pub(crate) fn loopback_fail(&self, device: IpAddr, start: Duration, max: Duration) -> Duration {
        let mut loopback = lock(&self.loopback);
        if loopback.len() >= MAX_TRACKED && !loopback.contains_key(&device) {
            loopback.clear();
        }
        let failures = loopback.entry(device).or_default();
        *failures = failures.saturating_add(1);
        backoff(*failures, start, max)
    }

    /// 그 컴퓨터가 맞는 키로 들어왔다.
    pub(crate) fn loopback_succeed(&self, device: IpAddr) {
        lock(&self.loopback).remove(&device);
    }
}

/// `failures`번째 실패의 늦춤: `start × 2^(failures-1)`, `max`에서 멈춘다.
fn backoff(failures: u32, start: Duration, max: Duration) -> Duration {
    let doublings = failures.saturating_sub(1).min(31);
    start.saturating_mul(1_u32 << doublings).min(max)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    const A: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9));
    const B: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));

    #[test]
    fn 다섯_번째에_잠기고_알림은_한_번이다() {
        let strikes = Strikes::default();
        let now = UnixMillis(1_000);
        for _ in 1..MAX_KEY_FAILURES {
            assert!(strikes.fail(A, now).is_none());
        }
        let notice = strikes.fail(A, now).expect("다섯 번째에 잠긴다");
        assert_eq!(notice.failures, MAX_KEY_FAILURES);
        assert_eq!(notice.until, UnixMillis(1_000 + lockout_millis()));
        assert!(
            strikes.fail(A, now).is_none(),
            "잠긴 동안은 다시 알리지 않는다"
        );
        assert_eq!(strikes.locked_until(A, now), Some(notice.until));
        assert!(strikes.locked_until(B, now).is_none(), "다른 기기는 그대로");
        assert!(
            strikes.locked_until(A, notice.until).is_none(),
            "시간이 지나면 풀린다"
        );
        assert!(
            strikes.fail(A, notice.until).is_none(),
            "풀린 뒤에는 처음부터 센다"
        );
    }

    #[test]
    fn 맞는_키는_횟수를_지우고_풀기는_잠긴_때만_참이다() {
        let strikes = Strikes::default();
        let now = UnixMillis(0);
        for _ in 1..MAX_KEY_FAILURES {
            strikes.fail(A, now);
        }
        strikes.succeed(A);
        assert!(strikes.fail(A, now).is_none());
        assert!(!strikes.unlock(A, now), "잠기지 않았으면 거짓");
        for _ in 0..MAX_KEY_FAILURES {
            strikes.fail(B, now);
        }
        assert_eq!(strikes.locked(now).len(), 1);
        assert!(strikes.unlock(B, now));
        assert!(strikes.locked(now).is_empty());
    }

    #[test]
    fn 그_컴퓨터는_두_배씩_상한까지_늦춘다() {
        let strikes = Strikes::default();
        let (start, max) = (Duration::from_millis(250), Duration::from_secs(5));
        let got: Vec<u128> = (0..7)
            .map(|_| strikes.loopback_fail(A, start, max).as_millis())
            .collect();
        assert_eq!(got, [250, 500, 1_000, 2_000, 4_000, 5_000, 5_000]);
        strikes.loopback_succeed(A);
        assert_eq!(strikes.loopback_fail(A, start, max), start);
        assert_eq!(backoff(u32::MAX, start, max), max);
    }
}
