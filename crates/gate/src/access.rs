//! 접속 키 · 잠금 · 원격 세션 (9차, T17).
//!
//! 근거: `docs/dev/modules/gate.md` "9차 작업", `dev.md` C1 · M6 · E1 · E2 · N3 · V37~V41,
//! 원본 근거 `docs/research/triggers-approval-memory-deep.md` 5장,
//! `docs/research/remote-access-security.md` 4.5 · 4.6, 결정 2026-11-16 8 · 9 · 10번.
//!
//! # 누가 "그 컴퓨터"인가
//!
//! 연결의 실제 상대 주소가 루프백이고 **전달 헤더([`FORWARDING_HEADERS`])가 하나도 없는** 요청만
//! 그 컴퓨터의 요청이다. 전달 헤더가 하나라도 있으면(값이 무엇이든) 프록시(Tailscale Serve 등)를
//! 거친 요청으로 보고 루프백으로 치지 않는다: 접속 키가 있어야 하고, 잠글 수 있다.
//! 이때 "기기"와 접속 기록의 "어디서"는 여전히 연결의 실제 상대 주소다(헤더 값은 읽지 않는다).
//!
//! # 접속 키로 세션 만들기 (C1 · V37 · V38)
//!
//! `POST` [`crate::SESSION_PATH`]에 JSON 본문 `{"`[`ACCESS_KEY_FIELD`]`": "<키>"}`를 보낸다
//! (`Content-Type: application/json`). Host · Origin 확인은 지금과 같다.
//!
//! - 키가 맞으면 204와 세션 쿠키
//!   `pi_session=<값>; HttpOnly; SameSite=Strict; Path=/; Max-Age=2592000`(30일,
//!   [`REMOTE_SESSION_MAX_AGE`])를 주고, 접속 기록에 [`crate::ACCESS_ENTER`] 한 줄을 남긴다
//!   (시각 [`crate::Clock::now`], 어디서는 실제 상대 IP 글자).
//! - 서버도 원격 세션을 만든 때부터 [`crate::Clock`]으로 30일이 지나면 버린다.
//! - 키가 틀리거나, 본문에 키가 없거나, 아직 둔 키가 없으면 401이다(E1). 응답 본문은 셋이 같다
//!   (어느 경우인지 알려 주지 않는다). 세션을 주지 않는다. 틀린 키는 실패 횟수를 올리고 접속 기록에
//!   [`ACCESS_KEY_WRONG`]을 남긴다. 키 글자는 응답 · 기록 · 로그 어디에도 싣지 않는다.
//! - 그 컴퓨터의 요청은 지금처럼 키 없이 들어온다(본문에 키 칸이 없으면 키를 보지 않는다).
//!   키 칸을 넣어 보냈으면 맞춰 보고, 틀리면 401과 늦춤(아래 "잠금")이다. 맞거나 키가 없으면
//!   그 컴퓨터의 세션(쿠키에 `Max-Age` 없음)을 준다.
//!
//! # 잠금 (N3 · V39 · E2)
//!
//! - 한 기기(실제 상대 IP)가 키를 [`MAX_KEY_FAILURES`]번 틀리면 그때부터 [`LOCKOUT`] 동안
//!   그 기기의 시도를 막는다. 시각은 [`crate::Clock`]이다. 잠긴 동안의 시도는 키가 맞아도
//!   [`LOCKED_STATUS`]이고 세션을 주지 않는다.
//! - 잠기는 순간 [`LockAlert::locked`]를 한 번 부르고 접속 기록에 [`ACCESS_LOCKED`]를 남긴다.
//!   잠긴 동안의 시도로는 다시 부르지 않는다.
//! - 잠금은 메모리에만 둔다(다시 켜면 풀린다). 다른 기기는 잠기지 않는다. 이미 들어온 세션은
//!   잠금과 상관없이 유지된다.
//! - 그 컴퓨터의 요청은 막지 않는다. 그 컴퓨터는 [`AccessKeys::unlock`]으로 잠금을 풀 수 있다
//!   (그 경로를 잇는 것은 조립하는 쪽이다).
//! - (10차) 잠금 확인과 실패 기록은 한 번에 한다: 같은 기기가 동시에 여러 요청을 보내도 키를
//!   맞춰 보는 시도(401을 받는 시도)는 [`MAX_KEY_FAILURES`]번을 넘지 않는다. 나머지는
//!   [`LOCKED_STATUS`]다. 잠금 알림도 한 번뿐이다.
//! - (10차) 세어 두는 기기(틀린 횟수만 있는 기기와 잠긴 기기, 그 컴퓨터의 늦춤 횟수)는
//!   [`crate::GateConfig::max_tracked_devices`]를 넘지 않는다. 잠금 시간이 지난 기기는 상한을
//!   차지하지 않는다(정리한다). 기기는 IPv4로 바꿀 수 있는 IPv6(`::ffff:a.b.c.d`)를 IPv4로 친다.
//!
//! # 원격 세션의 수명 (10차)
//!
//! 원격 세션으로 연 실시간 연결도 세션을 만든 때부터 [`crate::Clock`]으로
//! [`REMOTE_SESSION_MAX_AGE`]가 지나면 닫는다. 늦어도 ping 간격
//! ([`crate::GateConfig::keepalive_interval`])마다 관문의 시계로 확인한다.
//! 그 컴퓨터의 세션으로 연 연결은 이 수명으로 닫지 않는다.
//!
//! # 키 바꾸기 (V41)
//!
//! [`AccessKeys::issue`]가 새 키를 만들고 저장소의 세대를 올린다. 앞 세대로 만든 원격 세션은
//! 모두 끊긴다(보호된 경로 401, 열린 실시간 연결은 닫힌다). 그 컴퓨터의 세션은 그대로다.
//! 앞 키는 더 맞지 않는다.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pi_store::{AccessKeyHash, NewAccess, Store, StoreError, UnixMillis};
use rand::TryRngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use tokio::sync::watch;

use crate::session::{Now, Sessions};
use crate::strikes::Strikes;
use crate::{Clock, GateError};

/// 접속 키를 담는 JSON 본문의 칸 이름.
pub const ACCESS_KEY_FIELD: &str = "access_key";
/// 접속 키의 글자 수. 글자마다 5비트라 100비트다(결정 2026-11-16 8번 "약 100비트").
pub const ACCESS_KEY_LEN: usize = 20;
/// 접속 키에 쓰는 글자(Crockford base32: 헷갈리는 I · L · O · U를 뺀 대문자와 숫자).
pub const ACCESS_KEY_ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// 접속 키 해시에 치는 소금의 바이트 수. 키마다 새로 만든다.
/// 해시는 `SHA-256(소금 ‖ 키의 UTF-8 바이트)`다.
pub const ACCESS_KEY_SALT_BYTES: usize = 16;
/// 이만큼 틀리면 그 기기를 잠근다(N3).
pub const MAX_KEY_FAILURES: u32 = 5;
/// 잠금 시간(N3, 15분).
pub const LOCKOUT: Duration = Duration::from_secs(15 * 60);
/// 원격 세션의 수명(쿠키 `Max-Age`와 서버 쪽 수명, 결정 10번).
pub const REMOTE_SESSION_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// (10차) 틀린 횟수 · 잠금을 세어 두는 기기 수 상한의 기본값(원본 OpenClaw 10,000).
/// [`crate::GateConfig::max_tracked_devices`]의 기본값이다.
pub const MAX_TRACKED_DEVICES: usize = 10_000;
/// 잠긴 기기의 시도에 주는 HTTP 상태.
pub const LOCKED_STATUS: u16 = 429;
/// 프록시를 거쳤다는 표시로 보는 요청 헤더(소문자). 하나라도 있으면 루프백으로 치지 않는다.
pub const FORWARDING_HEADERS: [&str; 3] = ["forwarded", "x-forwarded-for", "x-real-ip"];
/// 그 컴퓨터가 키를 틀렸을 때 늦추는 시간의 시작값 기본값(원본 OpenClaw 250ms).
pub const DEFAULT_LOOPBACK_DELAY_START: Duration = Duration::from_millis(250);
/// 그 컴퓨터가 키를 틀렸을 때 늦추는 시간의 상한 기본값(원본 OpenClaw 5초).
pub const DEFAULT_LOOPBACK_DELAY_MAX: Duration = Duration::from_secs(5);
/// 접속 기록의 "무엇을": 접속 키 틀림.
pub const ACCESS_KEY_WRONG: &str = "접속 키 틀림";
/// 접속 기록의 "무엇을": 기기 잠김.
pub const ACCESS_LOCKED: &str = "잠김";
/// 접속 기록의 "무엇을": 접속 키 바꿈.
pub const ACCESS_KEY_CHANGED: &str = "접속 키 바꿈";

/// 새로 만든 접속 키. 키 글자는 이것으로 한 번만 보여 준다(어디에도 저장하지 않는다).
pub struct IssuedKey {
    /// 키 글자([`ACCESS_KEY_LEN`]자, [`ACCESS_KEY_ALPHABET`]의 글자).
    pub key: String,
    /// 저장소에 둔 세대(처음 1, 바꿀 때마다 1씩).
    pub generation: u64,
}

/// 잠금 알림 한 건(E2). 키 글자는 싣지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockNotice {
    /// 잠긴 기기(실제 상대 IP).
    pub device: IpAddr,
    /// 그때까지 틀린 횟수.
    pub failures: u32,
    /// 잠금이 풀리는 시각.
    pub until: UnixMillis,
}

/// 잠금 알림을 내는 자리(E2). 보내기는 조립하는 쪽(notify)이 한다.
/// 요청 처리 중에 불리므로 오래 붙잡지 않는다.
pub trait LockAlert: Send + Sync {
    /// 기기가 잠겼다.
    fn locked(&self, notice: LockNotice);
}

/// 알림을 내지 않는다(기본값).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoLockAlert;

impl LockAlert for NoLockAlert {
    fn locked(&self, _notice: LockNotice) {}
}

/// 지금 잠긴 기기 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedDevice {
    /// 기기(실제 상대 IP).
    pub device: IpAddr,
    /// 잠금이 풀리는 시각.
    pub until: UnixMillis,
}

/// 접속 키와 잠금을 다루는 손잡이. [`crate::GateServer::access_keys`]로 얻는다.
#[derive(Clone)]
pub struct AccessKeys {
    access: Arc<Access>,
}

impl AccessKeys {
    pub(crate) fn new(access: Arc<Access>) -> Self {
        Self { access }
    }

    /// 새 접속 키를 만든다: 운영체제 난수로 키와 소금을 만들고, 소금 친 SHA-256만
    /// `pi_store::Store::set_access_key`로 새 세대에 둔다. 앞 세대의 원격 세션을 모두 끊고,
    /// 접속 기록에 [`ACCESS_KEY_CHANGED`]를 남긴다.
    ///
    /// 저장에 실패하면 [`GateError::Store`]이고 세션을 끊지 않는다.
    /// 운영체제 난수를 얻지 못하면 [`GateError::Random`]이고 아무것도 바꾸지 않는다.
    pub async fn issue(&self) -> Result<IssuedKey, GateError> {
        let access = &self.access;
        let (key, salt) = new_key().ok_or(GateError::Random)?;
        let hash = AccessKeyHash {
            sha256: salted(&salt, &key),
            salt,
        };
        let store = access.store.clone();
        let at = access.now();
        let record = tokio::task::spawn_blocking(move || store.set_access_key(hash, at))
            .await
            .map_err(|_| GateError::Task)??;
        access.advance(record.generation);
        access.sessions.drop_remote_before(record.generation);
        // 키는 이미 바뀌었다. 기록을 남기지 못해도 새 키를 돌려준다(못 받으면 아무도 들어오지 못한다).
        if !access.record(KEY_ISSUER, ACCESS_KEY_CHANGED, at).await {
            tracing::error!("접속 키를 바꾼 것을 기록하지 못했습니다");
        }
        Ok(IssuedKey {
            key,
            generation: record.generation,
        })
    }

    /// 지금 잠긴 기기들(잠금 시간이 지난 것은 빠진다).
    pub fn locked(&self) -> Vec<LockedDevice> {
        self.access.strikes.locked(self.access.now())
    }

    /// 그 기기의 잠금과 실패 횟수를 지운다. 잠겨 있었으면 참.
    pub fn unlock(&self, device: IpAddr) -> bool {
        let device = device.to_canonical();
        self.access.strikes.unlock(device, self.access.now())
    }
}

/// 키를 바꾼 기록의 "어디서". 키는 그 컴퓨터에서만 바꾼다(잇는 것은 조립하는 쪽).
const KEY_ISSUER: &str = "이 컴퓨터";

/// 접속 키 · 세션 · 잠금을 함께 쥔 것. 관문의 요청 처리와 [`AccessKeys`]가 나눠 쓴다.
pub(crate) struct Access {
    store: Store,
    clock: Arc<dyn Clock>,
    alert: Arc<dyn LockAlert>,
    pub(crate) sessions: Sessions,
    pub(crate) strikes: Strikes,
    /// 지금까지 본 가장 높은 키 세대. 오르면 앞 세대의 원격 연결이 닫힌다.
    generation: watch::Sender<u64>,
    /// 그 컴퓨터를 늦추는 시간의 시작값과 상한.
    pub(crate) loopback_delay: (Duration, Duration),
}

impl Access {
    pub(crate) fn new(
        store: Store,
        clock: Arc<dyn Clock>,
        alert: Arc<dyn LockAlert>,
        loopback_delay: (Duration, Duration),
        max_tracked: usize,
    ) -> Self {
        Self {
            store,
            clock,
            alert,
            sessions: Sessions::default(),
            strikes: Strikes::new(max_tracked),
            generation: watch::Sender::new(0),
            loopback_delay,
        }
    }

    /// 관문의 시계로 지금.
    pub(crate) fn now(&self) -> UnixMillis {
        self.clock.now()
    }

    /// 세션을 볼 때의 지금.
    pub(crate) fn session_now(&self) -> Now {
        Now {
            instant: Instant::now(),
            clock: self.now(),
            generation: *self.generation.borrow(),
        }
    }

    /// 본 세대를 올린다(내리지 않는다).
    fn advance(&self, generation: u64) {
        self.generation.send_if_modified(|current| {
            let higher = generation > *current;
            if higher {
                *current = generation;
            }
            higher
        });
    }

    /// 세대가 `generation`보다 높아질 때(키를 바꿨을 때) 끝난다.
    pub(crate) async fn revoked(&self, generation: u64) {
        let mut seen = self.generation.subscribe();
        let _ = seen.wait_for(|current| *current > generation).await;
    }

    /// 넣은 키가 저장된 지금 키와 맞으면 그 세대. 둔 키가 없거나 틀리면 없음.
    /// 비교는 해시끼리 상수 시간으로 한다.
    pub(crate) async fn verify(&self, key: &str) -> Result<Option<u64>, StoreError> {
        let store = self.store.clone();
        let Ok(found) = tokio::task::spawn_blocking(move || store.access_key()).await else {
            return Ok(None);
        };
        let Some(record) = found? else {
            return Ok(None);
        };
        if !same_hash(&salted(&record.salt, key), &record.sha256) {
            return Ok(None);
        }
        self.advance(record.generation);
        Ok(Some(record.generation))
    }

    /// 접속 기록 한 줄을 남긴다. 남겼으면 참.
    pub(crate) async fn record(&self, origin: &str, action: &str, at: UnixMillis) -> bool {
        let access = NewAccess {
            at,
            origin: origin.to_owned(),
            action: action.to_owned(),
        };
        let store = self.store.clone();
        let recorded = tokio::task::spawn_blocking(move || store.record_access(access)).await;
        matches!(recorded, Ok(Ok(_)))
    }

    /// 다른 기기가 키를 틀렸다: 기록하고, 이번에 잠기면 잠김을 기록하고 알린다.
    pub(crate) async fn remote_failed(&self, device: IpAddr, now: UnixMillis) {
        let origin = device.to_string();
        if !self.record(&origin, ACCESS_KEY_WRONG, now).await {
            tracing::error!("접속 키 틀림을 기록하지 못했습니다");
        }
        if let Some(notice) = self.strikes.fail(device, now) {
            tracing::warn!(%device, "접속 키를 여러 번 틀린 기기를 잠갔습니다");
            if !self.record(&origin, ACCESS_LOCKED, now).await {
                tracing::error!("잠김을 기록하지 못했습니다");
            }
            self.alert.locked(notice);
        }
    }
}

/// 새 키 글자와 소금. 운영체제 난수를 얻지 못하면 없음.
fn new_key() -> Option<(String, Vec<u8>)> {
    let mut bytes = [0_u8; ACCESS_KEY_LEN];
    let mut salt = vec![0_u8; ACCESS_KEY_SALT_BYTES];
    OsRng.try_fill_bytes(&mut bytes).ok()?;
    OsRng.try_fill_bytes(&mut salt).ok()?;
    let alphabet = ACCESS_KEY_ALPHABET.as_bytes();
    // 글자 수가 32라 바이트의 아래 5비트를 쓰면 치우침이 없다.
    let key = bytes
        .iter()
        .map(|byte| char::from(alphabet[usize::from(byte & 0x1f) % alphabet.len()]))
        .collect();
    Some((key, salt))
}

/// `SHA-256(소금 ‖ 키의 UTF-8 바이트)`.
fn salted(salt: &[u8], key: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(key.as_bytes());
    hasher.finalize().into()
}

/// 두 해시가 같은가. 어디서 다른지에 따라 걸리는 시간이 달라지지 않게 끝까지 본다.
fn same_hash(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let diff = a
        .iter()
        .zip(b.iter())
        .fold(0_u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(diff) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 새_키는_정한_글자_20자이고_매번_다르다() {
        let (first, salt) = new_key().unwrap_or_default();
        let (second, _) = new_key().unwrap_or_default();
        assert_eq!(first.chars().count(), ACCESS_KEY_LEN);
        assert!(first.chars().all(|c| ACCESS_KEY_ALPHABET.contains(c)));
        assert_eq!(salt.len(), ACCESS_KEY_SALT_BYTES);
        assert_ne!(first, second);
    }

    #[test]
    fn 해시_비교는_한_바이트만_달라도_다르다() {
        let a = salted(b"0123456789abcdef", "KEY");
        let mut b = a;
        assert!(same_hash(&a, &b));
        b[31] ^= 1;
        assert!(!same_hash(&a, &b));
        assert_ne!(
            salted(b"0123456789abcdef", "KEY"),
            salted(b"0123456789abcdeg", "KEY")
        );
    }
}
