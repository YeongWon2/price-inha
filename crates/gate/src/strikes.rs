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

use crate::access::{LOCKOUT, LockNotice, LockedDevice, MAX_KEY_FAILURES, MAX_TRACKED_DEVICES};

#[derive(Debug, Clone, Copy, Default)]
struct Count {
    failures: u32,
    /// 키를 맞춰 보는 중인 시도 수(자리를 잡아 둔 것). 동시 요청이 5번을 넘기지 못하게 한다.
    pending: u32,
    until: Option<UnixMillis>,
}

/// 잡아 둔 시도 자리. 끝내지 않고 버려지면(요청 취소) 자리를 돌려준다.
pub(crate) struct Slot<'a> {
    strikes: &'a Strikes,
    device: IpAddr,
}

impl Slot<'_> {
    /// 키를 틀렸다(자리를 쓴다). 이번에 잠기면 알림 내용을 돌려준다.
    pub(crate) fn fail(self, now: UnixMillis) -> Option<LockNotice> {
        let this = std::mem::ManuallyDrop::new(self);
        this.strikes.record_failure(this.device, now)
    }

    /// 맞는 키로 들어왔다.
    pub(crate) fn succeed(self) {
        let this = std::mem::ManuallyDrop::new(self);
        this.strikes.clear(this.device);
    }

    /// 키를 맞춰 보지 않고 자리를 돌려준다.
    pub(crate) fn release(self) {}
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.strikes.give_back(self.device);
    }
}

/// 기기마다 틀린 횟수와 잠금.
pub(crate) struct Strikes {
    remote: Mutex<HashMap<IpAddr, Count>>,
    loopback: Mutex<HashMap<IpAddr, u32>>,
    /// 세어 두는 기기 수의 상한(원본 10,000).
    max_tracked: usize,
}

impl Default for Strikes {
    fn default() -> Self {
        Self::new(MAX_TRACKED_DEVICES)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn lockout_millis() -> i64 {
    i64::try_from(LOCKOUT.as_millis()).unwrap_or(i64::MAX)
}

impl Strikes {
    pub(crate) fn new(max_tracked: usize) -> Self {
        Self {
            remote: Mutex::default(),
            loopback: Mutex::default(),
            max_tracked: max_tracked.max(1),
        }
    }

    /// 키를 맞춰 볼 자리를 잡는다. 잠겨 있거나 이미 맞춰 보는 중인 시도까지 합쳐 틀릴 수 있는
    /// 횟수를 채웠으면 거짓(잠금 확인과 시도 기록을 한 번에 한다). 잠금 시간이 지났으면 표시를 지운다.
    /// 받은 [`Slot`]은 `fail` · `succeed` · `release`로 끝낸다. 요청이 중간에 취소되어 버려지면
    /// `Drop`이 자리를 돌려준다.
    pub(crate) fn reserve(&self, device: IpAddr, now: UnixMillis) -> Option<Slot<'_>> {
        let mut remote = lock(&self.remote);
        if let Some(count) = remote.get_mut(&device) {
            if count.until.is_some_and(|until| now >= until) {
                remote.remove(&device);
            } else if count.until.is_some()
                || count.failures.saturating_add(count.pending) >= MAX_KEY_FAILURES
            {
                return None;
            }
        }
        if !remote.contains_key(&device) {
            make_room(&mut remote, self.max_tracked, now);
        }
        remote.entry(device).or_default().pending += 1;
        Some(Slot {
            strikes: self,
            device,
        })
    }

    /// 잡은 자리를 키를 맞춰 보지 않고 돌려준다.
    fn give_back(&self, device: IpAddr) {
        let mut remote = lock(&self.remote);
        if let Some(count) = remote.get_mut(&device) {
            count.pending = count.pending.saturating_sub(1);
            if count.pending == 0 && count.failures == 0 && count.until.is_none() {
                remote.remove(&device);
            }
        }
    }

    /// 다른 기기가 키를 틀렸다(잡아 둔 자리를 쓴다). 이번에 잠기면 알림 내용을 돌려준다
    /// (이미 잠겨 있으면 없음).
    fn record_failure(&self, device: IpAddr, now: UnixMillis) -> Option<LockNotice> {
        let mut remote = lock(&self.remote);
        if !remote.contains_key(&device) {
            make_room(&mut remote, self.max_tracked, now);
        }
        let count = remote.entry(device).or_default();
        count.pending = count.pending.saturating_sub(1);
        if let Some(until) = count.until {
            if now < until {
                return None;
            }
            count.failures = 0;
            count.until = None;
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
    fn clear(&self, device: IpAddr) {
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
        if loopback.len() >= self.max_tracked && !loopback.contains_key(&device) {
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

/// 새 기기를 넣을 자리를 만든다: 잠금이 끝난 것을 지우고, 그래도 가득 차면 맞춰 보는 중이 아닌
/// 틀림만 있는 기기를, 그래도 가득 차면 가장 오래 잠긴(풀리는 시각이 가장 이른) 기기를 버린다.
fn make_room(remote: &mut HashMap<IpAddr, Count>, max: usize, now: UnixMillis) {
    if remote.len() < max {
        return;
    }
    remote.retain(|_, count| count.until.is_none_or(|until| now < until) || count.pending > 0);
    if remote.len() < max {
        return;
    }
    remote.retain(|_, count| count.until.is_some() || count.pending > 0);
    while remote.len() >= max {
        let oldest = remote
            .iter()
            .min_by_key(|(device, count)| (count.until.is_none(), count.until, **device))
            .map(|(device, _)| *device);
        let Some(oldest) = oldest else { return };
        remote.remove(&oldest);
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
            assert!(strikes.reserve(A, now).unwrap().fail(now).is_none());
        }
        let notice = strikes
            .reserve(A, now)
            .unwrap()
            .fail(now)
            .expect("다섯 번째에 잠긴다");
        assert_eq!(notice.failures, MAX_KEY_FAILURES);
        assert_eq!(notice.until, UnixMillis(1_000 + lockout_millis()));
        assert!(
            strikes.reserve(A, now).is_none(),
            "잠긴 동안은 자리를 주지 않는다"
        );
        assert!(strikes.reserve(B, now).is_some(), "다른 기기는 그대로");
        let slot = strikes
            .reserve(A, notice.until)
            .expect("시간이 지나면 풀린다");
        assert!(
            slot.fail(notice.until).is_none(),
            "풀린 뒤에는 처음부터 센다"
        );
    }

    #[test]
    fn 맞는_키는_횟수를_지우고_풀기는_잠긴_때만_참이다() {
        let strikes = Strikes::default();
        let now = UnixMillis(0);
        for _ in 1..MAX_KEY_FAILURES {
            strikes.reserve(A, now).unwrap().fail(now);
        }
        strikes.reserve(A, now).unwrap().succeed();
        assert!(strikes.reserve(A, now).unwrap().fail(now).is_none());
        assert!(!strikes.unlock(A, now), "잠기지 않았으면 거짓");
        for _ in 0..MAX_KEY_FAILURES {
            strikes.reserve(B, now).unwrap().fail(now);
        }
        assert_eq!(strikes.locked(now).len(), 1);
        assert!(strikes.unlock(B, now));
        assert!(strikes.locked(now).is_empty());
    }

    #[test]
    fn 동시에_잡는_자리는_다섯을_넘지_않고_상한이_차면_오래_잠긴_기기부터_버린다() {
        let strikes = Strikes::new(2);
        let now = UnixMillis(0);
        let mut slots: Vec<Slot<'_>> = (0..30).filter_map(|_| strikes.reserve(A, now)).collect();
        assert_eq!(slots.len(), MAX_KEY_FAILURES as usize);
        slots.pop().unwrap().release();
        assert!(strikes.reserve(A, now).is_some(), "돌려준 자리는 다시 쓴다");
        drop(slots);
        assert!(strikes.reserve(A, now).is_some(), "버려진 자리도 돌아온다");
        let strikes = Strikes::new(2);
        let c = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        for (offset, device) in [(0_i64, A), (1, B), (2, c)] {
            let at = UnixMillis(offset);
            let mut tried = 0;
            while let Some(slot) = strikes.reserve(device, at) {
                slot.fail(at);
                tried += 1;
                assert!(tried <= 5);
            }
        }
        let locked: Vec<IpAddr> = strikes.locked(now).iter().map(|l| l.device).collect();
        assert_eq!(locked, [B, c], "가장 오래 잠긴 A가 빠진다");
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
