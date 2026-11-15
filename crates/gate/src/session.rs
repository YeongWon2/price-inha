//! 세션: 값은 화면(쿠키)에만 있고, 여기에는 해시만 메모리에 둔다.
//!
//! 세션은 두 가지다(9차).
//! - **그 컴퓨터의 세션**: 만든 지 [`SESSION_TTL`]이 지나면 버린다(`Instant`).
//! - **원격 세션**: 접속 키로 만든다. 만든 때부터 관문의 시계로 [`crate::REMOTE_SESSION_MAX_AGE`]가
//!   지나면 버리고, 만든 때의 키 세대보다 지금 세대가 높으면(키를 바꿨으면) 버린다(V41).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use pi_store::UnixMillis;
use rand::TryRngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

/// 한꺼번에 살아 있을 수 있는 세션 수. 넘으면 가장 오래된 것부터 버린다(메모리가 끝없이 늘지 않게).
pub(crate) const MAX_SESSIONS: usize = 1024;
/// 그 컴퓨터의 세션 수명. 만든 지 이만큼 지나면 버린다.
pub(crate) const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// 세션 값의 바이트 수(256비트).
const SESSION_BYTES: usize = 32;
/// 받아 주는 세션 값의 가장 긴 길이. 더 길면 해시도 하지 않고 모르는 값으로 본다.
const MAX_VALUE_LEN: usize = 128;

type Hash = [u8; 32];

/// 운영체제 난수를 얻지 못했다.
#[derive(Debug)]
pub(crate) struct NoRandom;

/// 세션의 종류. 관문이 세션 확인을 지난 요청의 확장에 넣는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionKind {
    /// 그 컴퓨터에서 키 없이 만든 세션.
    Local,
    /// 접속 키로 만든 원격 세션.
    Remote {
        /// 만든 때 맞춘 키의 세대.
        generation: u64,
        /// 만든 시각(관문의 시계).
        made_at: UnixMillis,
    },
}

/// 세션을 볼 때의 "지금".
#[derive(Debug, Clone, Copy)]
pub(crate) struct Now {
    /// 그 컴퓨터의 세션 수명에 쓴다.
    pub(crate) instant: Instant,
    /// 원격 세션 수명에 쓴다(관문의 시계).
    pub(crate) clock: UnixMillis,
    /// 지금 접속 키 세대. 이보다 낮은 세대의 원격 세션은 죽은 것이다.
    pub(crate) generation: u64,
}

struct Entry {
    made: Instant,
    kind: SessionKind,
}

impl Entry {
    fn alive(&self, now: Now) -> bool {
        match self.kind {
            SessionKind::Local => now.instant.saturating_duration_since(self.made) < SESSION_TTL,
            SessionKind::Remote {
                generation,
                made_at,
            } => generation >= now.generation && remote_alive(made_at, now.clock),
        }
    }
}

/// 원격 세션이 수명 안인가(만든 때부터 [`crate::REMOTE_SESSION_MAX_AGE`] 미만).
fn remote_alive(made_at: UnixMillis, now: UnixMillis) -> bool {
    let life = i64::try_from(crate::REMOTE_SESSION_MAX_AGE.as_millis()).unwrap_or(i64::MAX);
    now.0.saturating_sub(made_at.0) < life
}

/// 살아 있는 세션들.
#[derive(Default)]
pub(crate) struct Sessions {
    /// 해시 → 만든 때와 종류.
    alive: Mutex<HashMap<Hash, Entry>>,
}

impl Sessions {
    fn lock(&self) -> MutexGuard<'_, HashMap<Hash, Entry>> {
        self.alive.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 새 세션을 만들고 값을 돌려준다. 값은 이 함수 밖 어디에도 남지 않는다.
    pub(crate) fn create(&self, kind: SessionKind, now: Now) -> Result<String, NoRandom> {
        let mut bytes = [0_u8; SESSION_BYTES];
        OsRng.try_fill_bytes(&mut bytes).map_err(|_| NoRandom)?;
        let value = URL_SAFE_NO_PAD.encode(bytes);
        let mut alive = self.lock();
        alive.retain(|_, entry| entry.alive(now));
        while alive.len() >= MAX_SESSIONS {
            let oldest = alive
                .iter()
                .min_by_key(|(_, entry)| entry.made)
                .map(|(hash, _)| *hash);
            match oldest {
                Some(hash) => alive.remove(&hash),
                None => break,
            };
        }
        let made = now.instant;
        alive.insert(hash_of(&value), Entry { made, kind });
        Ok(value)
    }

    /// 살아 있는 세션의 값이면 그 종류.
    pub(crate) fn find(&self, value: &str, now: Now) -> Option<SessionKind> {
        if value.is_empty() || value.len() > MAX_VALUE_LEN {
            return None;
        }
        let hash = hash_of(value);
        let mut alive = self.lock();
        let entry = alive.get(&hash)?;
        if entry.alive(now) {
            Some(entry.kind)
        } else {
            alive.remove(&hash);
            None
        }
    }

    /// `generation`보다 낮은 세대의 원격 세션을 모두 버린다(키를 바꿨다).
    pub(crate) fn drop_remote_before(&self, generation: u64) {
        self.lock().retain(|_, entry| match entry.kind {
            SessionKind::Local => true,
            SessionKind::Remote { generation: g, .. } => g >= generation,
        });
    }
}

fn hash_of(value: &str) -> Hash {
    Sha256::digest(value.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(instant: Instant) -> Now {
        Now {
            instant,
            clock: UnixMillis(0),
            generation: 0,
        }
    }

    fn make(sessions: &Sessions, now: Instant) -> String {
        sessions
            .create(SessionKind::Local, at(now))
            .unwrap_or_default()
    }

    #[test]
    fn 만든_세션은_살아_있고_모르는_값은_아니다() {
        let sessions = Sessions::default();
        let now = Instant::now();
        let value = make(&sessions, now);
        assert_eq!(value.len(), 43);
        assert_eq!(sessions.find(&value, at(now)), Some(SessionKind::Local));
        assert!(sessions.find("", at(now)).is_none());
        assert!(sessions.find(&"a".repeat(43), at(now)).is_none());
        assert!(
            sessions
                .find(&"a".repeat(MAX_VALUE_LEN + 1), at(now))
                .is_none()
        );
    }

    #[test]
    fn 수명이_지난_세션은_죽는다() {
        let sessions = Sessions::default();
        let now = Instant::now();
        let value = make(&sessions, now);
        let almost = now + SESSION_TTL - Duration::from_secs(1);
        assert!(sessions.find(&value, at(almost)).is_some());
        assert!(sessions.find(&value, at(now + SESSION_TTL)).is_none());
        assert!(sessions.lock().is_empty(), "죽은 세션은 지운다");
    }

    #[test]
    fn 세션_수는_상한을_넘지_않고_오래된_것부터_버린다() {
        let sessions = Sessions::default();
        let start = Instant::now();
        let first = make(&sessions, start);
        let mut last = String::new();
        for i in 1..=MAX_SESSIONS {
            let when = start + Duration::from_millis(u64::try_from(i).unwrap_or(u64::MAX));
            last = make(&sessions, when);
        }
        assert_eq!(sessions.lock().len(), MAX_SESSIONS);
        let now = at(start + Duration::from_secs(60));
        assert!(
            sessions.find(&first, now).is_none(),
            "가장 오래된 것이 밀려난다"
        );
        assert!(sessions.find(&last, now).is_some());
    }

    #[test]
    fn 원격_세션은_세대가_오르거나_30일이_지나면_죽는다() {
        let sessions = Sessions::default();
        let instant = Instant::now();
        let made_at = UnixMillis(1_000);
        let kind = SessionKind::Remote {
            generation: 2,
            made_at,
        };
        let now = |clock: i64, generation: u64| Now {
            instant,
            clock: UnixMillis(clock),
            generation,
        };
        let value = sessions.create(kind, now(1_000, 2)).unwrap_or_default();
        let life = i64::try_from(crate::REMOTE_SESSION_MAX_AGE.as_millis()).unwrap_or(i64::MAX);
        assert_eq!(sessions.find(&value, now(1_000 + life - 1, 2)), Some(kind));
        assert!(
            sessions.find(&value, now(1_000, 3)).is_none(),
            "세대가 오르면 죽는다"
        );
        let again = sessions.create(kind, now(1_000, 2)).unwrap_or_default();
        assert!(sessions.find(&again, now(1_000 + life, 2)).is_none());
        let kept = make(&sessions, instant);
        let gone = sessions.create(kind, now(1_000, 2)).unwrap_or_default();
        sessions.drop_remote_before(3);
        assert!(
            sessions.find(&kept, now(1_000, 3)).is_some(),
            "그 컴퓨터의 세션은 남는다"
        );
        assert!(
            sessions.find(&gone, now(1_000, 2)).is_none(),
            "앞 세대는 버린다"
        );
    }
}
