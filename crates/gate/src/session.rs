//! 세션: 값은 화면(쿠키)에만 있고, 여기에는 해시만 메모리에 둔다.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::TryRngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

/// 한꺼번에 살아 있을 수 있는 세션 수. 넘으면 가장 오래된 것부터 버린다(메모리가 끝없이 늘지 않게).
pub(crate) const MAX_SESSIONS: usize = 1024;
/// 세션 수명. 만든 지 이만큼 지나면 버린다.
pub(crate) const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// 세션 값의 바이트 수(256비트).
const SESSION_BYTES: usize = 32;
/// 받아 주는 세션 값의 가장 긴 길이. 더 길면 해시도 하지 않고 모르는 값으로 본다.
const MAX_VALUE_LEN: usize = 128;

type Hash = [u8; 32];

/// 운영체제 난수를 얻지 못했다.
#[derive(Debug)]
pub(crate) struct NoRandom;

/// 살아 있는 세션들.
#[derive(Default)]
pub(crate) struct Sessions {
    /// 해시 → 만든 때.
    alive: Mutex<HashMap<Hash, Instant>>,
}

impl Sessions {
    fn lock(&self) -> MutexGuard<'_, HashMap<Hash, Instant>> {
        self.alive.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 새 세션을 만들고 값을 돌려준다. 값은 이 함수 밖 어디에도 남지 않는다.
    pub(crate) fn create(&self) -> Result<String, NoRandom> {
        self.create_at(Instant::now())
    }

    fn create_at(&self, now: Instant) -> Result<String, NoRandom> {
        let mut bytes = [0_u8; SESSION_BYTES];
        OsRng.try_fill_bytes(&mut bytes).map_err(|_| NoRandom)?;
        let value = URL_SAFE_NO_PAD.encode(bytes);
        let mut alive = self.lock();
        alive.retain(|_, made| now.saturating_duration_since(*made) < SESSION_TTL);
        while alive.len() >= MAX_SESSIONS {
            let oldest = alive
                .iter()
                .min_by_key(|(_, made)| **made)
                .map(|(hash, _)| *hash);
            match oldest {
                Some(hash) => alive.remove(&hash),
                None => break,
            };
        }
        alive.insert(hash_of(&value), now);
        Ok(value)
    }

    /// 살아 있는 세션의 값인가.
    pub(crate) fn is_alive(&self, value: &str) -> bool {
        self.is_alive_at(value, Instant::now())
    }

    fn is_alive_at(&self, value: &str, now: Instant) -> bool {
        if value.is_empty() || value.len() > MAX_VALUE_LEN {
            return false;
        }
        let hash = hash_of(value);
        let mut alive = self.lock();
        match alive.get(&hash) {
            Some(made) if now.saturating_duration_since(*made) < SESSION_TTL => true,
            Some(_) => {
                alive.remove(&hash);
                false
            }
            None => false,
        }
    }
}

fn hash_of(value: &str) -> Hash {
    Sha256::digest(value.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(sessions: &Sessions, now: Instant) -> String {
        sessions.create_at(now).unwrap_or_default()
    }

    #[test]
    fn 만든_세션은_살아_있고_모르는_값은_아니다() {
        let sessions = Sessions::default();
        let now = Instant::now();
        let value = make(&sessions, now);
        assert_eq!(value.len(), 43);
        assert!(sessions.is_alive_at(&value, now));
        assert!(!sessions.is_alive_at("", now));
        assert!(!sessions.is_alive_at(&"a".repeat(43), now));
        assert!(!sessions.is_alive_at(&"a".repeat(MAX_VALUE_LEN + 1), now));
    }

    #[test]
    fn 수명이_지난_세션은_죽는다() {
        let sessions = Sessions::default();
        let now = Instant::now();
        let value = make(&sessions, now);
        let almost = now + SESSION_TTL - Duration::from_secs(1);
        assert!(sessions.is_alive_at(&value, almost));
        assert!(!sessions.is_alive_at(&value, now + SESSION_TTL));
        assert!(sessions.lock().is_empty(), "죽은 세션은 지운다");
    }

    #[test]
    fn 세션_수는_상한을_넘지_않고_오래된_것부터_버린다() {
        let sessions = Sessions::default();
        let start = Instant::now();
        let first = make(&sessions, start);
        let mut last = String::new();
        for i in 1..=MAX_SESSIONS {
            let at = start + Duration::from_millis(u64::try_from(i).unwrap_or(u64::MAX));
            last = make(&sessions, at);
        }
        assert_eq!(sessions.lock().len(), MAX_SESSIONS);
        let now = start + Duration::from_secs(60);
        assert!(
            !sessions.is_alive_at(&first, now),
            "가장 오래된 것이 밀려난다"
        );
        assert!(sessions.is_alive_at(&last, now));
    }
}
