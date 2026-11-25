//! 보낸 사람 판정 · 페어링(V88). 원본 OpenClaw `pairing-store-model` · Hermes `gateway/pairing.py`와
//! 같은 수치 · 판정 차례. 허용 1명 · DM만은 결정 24.
//!
//! 판정 차례([`Gatekeeper::judge`]):
//! 1. 1:1 대화가 아니면 [`Judgement::Ignored`]([`IgnoreReason::NotDirect`]) — 허용된 사람이어도.
//! 2. 허용 목록에 있으면 [`Judgement::Allowed`].
//! 3. 만료된 요청(만든 지 [`PAIRING_TTL_SECS`] 지남)을 지운다.
//! 4. 그 사람의 요청이 기다리는 중이면 [`Judgement::PairingPending`](코드를 다시 주지 않는다).
//! 5. 그 사람에게 [`PAIRING_RATE_LIMIT_SECS`] 안에 코드를 준 적이 있으면
//!    [`Judgement::Ignored`]([`IgnoreReason::RateLimited`]).
//! 6. 기다리는 요청이 [`MAX_PENDING`]건이면 [`Judgement::Ignored`]([`IgnoreReason::TooManyPending`]).
//! 7. 새 요청을 만들고 [`Judgement::PairingIssued`] — 코드는 이때만 준다.
//!
//! 코드 · 토큰은 기록(tracing)에 찍지 않는다. [`PairingCode`]의 `Debug`는 코드를 가린다.

use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, TimeDelta, Utc};
use rand::TryRngCore;
use rand::rngs::OsRng;
use zeroize::Zeroize;

use crate::{ApproveError, BookError, Error, Inbound, SenderId};

/// 페어링 코드 길이(원본 8자리).
pub const CODE_LEN: usize = 8;
/// 페어링 코드 글자표(원본 — 헷갈리는 0 · O · 1 · I 없음).
pub const CODE_ALPHABET: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// 요청이 만료되는 시간(원본 1시간).
pub const PAIRING_TTL_SECS: i64 = 3600;
/// 기다리는 요청의 최대 건수(원본 3건).
pub const MAX_PENDING: usize = 3;
/// 같은 보낸 사람에게 새 코드를 주는 최소 간격(원본 Hermes 10분).
pub const PAIRING_RATE_LIMIT_SECS: i64 = 600;
/// 이 횟수만큼 틀린 코드로 승인하면 잠근다(원본 Hermes 5회).
pub const MAX_FAILED_APPROVALS: u32 = 5;
/// 잠금 시간(원본 Hermes 1시간).
pub const PAIRING_LOCKOUT_SECS: i64 = 3600;
/// 허용 목록의 최대 인원(결정 24 — 본인 1명).
pub const MAX_ALLOWED: usize = 1;

/// 페어링 코드. 예측할 수 없게(암호용 난수) 만든다. `Debug`는 코드를 가린다.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PairingCode(String);

impl PairingCode {
    /// 저장된 코드로 만든다(장부를 읽을 때). 새 코드는 [`Gatekeeper::judge`]만 만든다.
    pub fn new(code: String) -> PairingCode {
        PairingCode(code)
    }

    /// 코드 글자. 보낸 사람에게 답으로 줄 때만 쓴다(기록에 찍지 않는다).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingCode(***)")
    }
}

/// 기다리는 페어링 요청 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRequest {
    /// 코드.
    pub code: PairingCode,
    /// 요청한 사람.
    pub sender: SenderId,
    /// 요청이 온 대화.
    pub chat: crate::ChatId,
    /// 만든 시각.
    pub created_at: DateTime<Utc>,
}

/// 보낸 사람에게 코드를 준 기록(10분에 한 번 규칙). 승인 · 빼기로 지우지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueRecord {
    /// 코드를 받은 사람.
    pub sender: SenderId,
    /// 준 시각.
    pub at: DateTime<Utc>,
}

/// 페어링 상태 전부. [`PairingBook`]이 통째로 읽고 쓴다.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PairingState {
    /// 허용된 사람(많아야 [`MAX_ALLOWED`]명).
    pub allowed: Vec<SenderId>,
    /// 기다리는 요청.
    pub pending: Vec<PendingRequest>,
    /// 코드를 준 기록.
    pub issued: Vec<IssueRecord>,
    /// 잠금 뒤로 쌓인 틀린 승인 횟수.
    pub failed_approvals: u32,
    /// 이 시각까지 승인을 받지 않는다.
    pub locked_until: Option<DateTime<Utc>>,
}

/// 페어링 장부 포트. 채널 하나에 하나. 저장(저장소 표 등)은 조립하는 곳이 넣는다.
pub trait PairingBook: Send + Sync {
    /// 지금 상태. 처음이면 빈 상태([`PairingState::default`]).
    fn load(&self) -> Result<PairingState, BookError>;
    /// 상태를 통째로 쓴다.
    fn save(&self, state: &PairingState) -> Result<(), BookError>;
}

/// 받지 않은 까닭.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreReason {
    /// 1:1 대화가 아니다(그룹 · 채널).
    NotDirect,
    /// 기다리는 요청이 이미 [`MAX_PENDING`]건이다.
    TooManyPending,
    /// 이 사람에게 [`PAIRING_RATE_LIMIT_SECS`] 안에 코드를 준 적이 있다.
    RateLimited,
}

/// 판정.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Judgement {
    /// 허용된 사람의 1:1 글. 명령으로 읽어도 된다.
    Allowed,
    /// 새 페어링 요청을 만들었다. 이 코드를 그 대화로 답한다(이때만).
    PairingIssued {
        /// 새 코드.
        code: PairingCode,
    },
    /// 이 사람의 요청이 이미 기다리는 중이다. 코드를 다시 주지 않는다.
    PairingPending,
    /// 받지 않는다(답하지 않는다).
    Ignored {
        /// 까닭.
        reason: IgnoreReason,
    },
}

/// 보낸 사람 판정 · 페어링 승인. 상태는 [`PairingBook`]에 둔다.
pub struct Gatekeeper {
    book: Arc<dyn PairingBook>,
}

impl fmt::Debug for Gatekeeper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Gatekeeper").finish_non_exhaustive()
    }
}

impl Gatekeeper {
    /// 장부로 만든다.
    pub fn new(book: Arc<dyn PairingBook>) -> Gatekeeper {
        Gatekeeper { book }
    }

    /// 받은 글의 보낸 사람을 판정한다(차례는 모듈 문서). 새 요청을 만들면 장부에 쓴다.
    pub fn judge(&self, inbound: &Inbound, now: DateTime<Utc>) -> Result<Judgement, Error> {
        if !inbound.is_direct {
            return Ok(Judgement::Ignored {
                reason: IgnoreReason::NotDirect,
            });
        }
        let mut state = self.book.load().map_err(book_error)?;
        if state.allowed.contains(&inbound.sender) {
            return Ok(Judgement::Allowed);
        }
        let pruned = prune_expired(&mut state, now);
        let refusal = if state.pending.iter().any(|p| p.sender == inbound.sender) {
            Some(Judgement::PairingPending)
        } else if recently_issued(&state, &inbound.sender, now) {
            Some(Judgement::Ignored {
                reason: IgnoreReason::RateLimited,
            })
        } else if state.pending.len() >= MAX_PENDING {
            Some(Judgement::Ignored {
                reason: IgnoreReason::TooManyPending,
            })
        } else {
            None
        };
        if let Some(judgement) = refusal {
            if pruned {
                self.book.save(&state).map_err(book_error)?;
            }
            return Ok(judgement);
        }
        let code = fresh_code(&state)?;
        state.pending.push(PendingRequest {
            code: code.clone(),
            sender: inbound.sender.clone(),
            chat: inbound.chat.clone(),
            created_at: now,
        });
        state
            .issued
            .retain(|r| r.sender != inbound.sender && within_rate_window(r.at, now));
        state.issued.push(IssueRecord {
            sender: inbound.sender.clone(),
            at: now,
        });
        self.book.save(&state).map_err(book_error)?;
        tracing::info!(channel = %inbound.channel, "페어링 요청을 만들었다");
        Ok(Judgement::PairingIssued { code })
    }

    /// 코드로 승인한다(같은 컴퓨터의 터미널 · 화면만 부른다 — 메신저 명령으로는 부르지 않는다).
    ///
    /// 차례: 잠금 중이면 [`ApproveError::Locked`] → 이미 허용된 사람이 있으면
    /// [`ApproveError::OwnerExists`](요청은 그대로 둔다) → 만료되지 않은 요청의 코드와 같으면
    /// 그 사람을 허용 목록에 넣고 요청을 지운 뒤 그 사람을 돌려준다. 코드는 앞뒤 공백을 떼고
    /// 대문자로 맞춰 견준다.
    /// 틀리면 [`ApproveError::WrongCode`]이고 실패를 하나 쌓는다. [`MAX_FAILED_APPROVALS`]번째 실패에서
    /// [`PAIRING_LOCKOUT_SECS`] 동안 잠그고(그 실패도 [`ApproveError::Locked`]) 횟수를 0으로 돌린다.
    pub fn approve(&self, code: &str, now: DateTime<Utc>) -> Result<SenderId, ApproveError> {
        let save =
            |state: &PairingState| self.book.save(state).map_err(|e| ApproveError::Book(e.0));
        let mut state = self.book.load().map_err(|e| ApproveError::Book(e.0))?;
        if let Some(until) = state.locked_until {
            if now < until {
                return Err(ApproveError::Locked { until });
            }
            state.locked_until = None;
        }
        if !state.allowed.is_empty() {
            return Err(ApproveError::OwnerExists);
        }
        prune_expired(&mut state, now);
        let wanted = code.trim().to_uppercase();
        if let Some(index) = find_code(&state.pending, &wanted) {
            let request = state.pending.remove(index);
            state.allowed.push(request.sender.clone());
            save(&state)?;
            tracing::info!("페어링을 승인했다");
            return Ok(request.sender);
        }
        state.failed_approvals = state.failed_approvals.saturating_add(1);
        if state.failed_approvals >= MAX_FAILED_APPROVALS {
            let until = now + TimeDelta::seconds(PAIRING_LOCKOUT_SECS);
            state.locked_until = Some(until);
            state.failed_approvals = 0;
            save(&state)?;
            tracing::warn!("틀린 페어링 코드가 쌓여 승인을 잠갔다");
            return Err(ApproveError::Locked { until });
        }
        save(&state)?;
        tracing::warn!("맞지 않는 페어링 코드로 승인하려 했다");
        Err(ApproveError::WrongCode)
    }

    /// 허용 목록에서 뺀다. 있었으면 `true`. 뺀 사람은 다시 처음 보는 사람이다.
    pub fn revoke(&self, sender: &SenderId) -> Result<bool, Error> {
        let mut state = self.book.load().map_err(book_error)?;
        let before = state.allowed.len();
        state.allowed.retain(|s| s != sender);
        if state.allowed.len() == before {
            return Ok(false);
        }
        self.book.save(&state).map_err(book_error)?;
        Ok(true)
    }

    /// 지금 허용된 사람(없으면 `None`).
    pub fn allowed(&self) -> Result<Option<SenderId>, Error> {
        let state = self.book.load().map_err(book_error)?;
        Ok(state.allowed.first().cloned())
    }
}

fn book_error(e: BookError) -> Error {
    Error::Book(e.0)
}

/// 만든 지 [`PAIRING_TTL_SECS`]가 지난 요청을 지운다. 지운 것이 있으면 `true`.
fn prune_expired(state: &mut PairingState, now: DateTime<Utc>) -> bool {
    let before = state.pending.len();
    let ttl = TimeDelta::seconds(PAIRING_TTL_SECS);
    state.pending.retain(|p| now - p.created_at <= ttl);
    state.pending.len() != before
}

/// `at`이 `now`에서 [`PAIRING_RATE_LIMIT_SECS`] 안인가.
fn within_rate_window(at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now - at < TimeDelta::seconds(PAIRING_RATE_LIMIT_SECS)
}

/// 이 사람에게 [`PAIRING_RATE_LIMIT_SECS`] 안에 코드를 준 적이 있는가.
fn recently_issued(state: &PairingState, sender: &SenderId, now: DateTime<Utc>) -> bool {
    state
        .issued
        .iter()
        .any(|r| &r.sender == sender && within_rate_window(r.at, now))
}

/// 기다리는 요청 가운데 코드가 `wanted`와 같은 것의 자리. 모든 요청을 끝까지 견주고(일찍 멈추지 않는다),
/// 견주기는 상수 시간이다.
fn find_code(pending: &[PendingRequest], wanted: &str) -> Option<usize> {
    let mut found = None;
    for (index, request) in pending.iter().enumerate() {
        if constant_time_eq(request.code.as_str().as_bytes(), wanted.as_bytes()) {
            found = Some(index);
        }
    }
    found
}

/// 두 바이트 열이 같은가. 길이가 같으면 내용과 상관없이 같은 시간이 걸린다.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let diff = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(diff) == 0
}

/// 새 코드를 뽑는 최대 횟수(기다리는 요청과 겹칠 때만 다시 뽑는다 — 32^8가지라 사실상 한 번).
const CODE_ATTEMPTS: usize = 16;

/// 기다리는 요청과 겹치지 않는 새 코드.
fn fresh_code(state: &PairingState) -> Result<PairingCode, Error> {
    for _ in 0..CODE_ATTEMPTS {
        let code = random_code()?;
        if !state.pending.iter().any(|p| p.code == code) {
            return Ok(code);
        }
    }
    Err(Error::Config(
        "겹치지 않는 페어링 코드를 만들지 못했다".to_owned(),
    ))
}

/// OS 난수로 [`CODE_ALPHABET`]에서 고르게(치우침 없이 — 나머지가 남는 바이트는 버린다) [`CODE_LEN`]자를 뽑는다.
fn random_code() -> Result<PairingCode, Error> {
    let alphabet = CODE_ALPHABET.as_bytes();
    let size = alphabet.len();
    // 이 값 아래의 바이트만 쓴다(256을 글자 수로 나눈 나머지만큼 버려 고르게 한다).
    let zone = 256 - (256 % size);
    let mut code = String::with_capacity(CODE_LEN);
    let mut buf = [0u8; 32];
    while code.len() < CODE_LEN {
        OsRng
            .try_fill_bytes(&mut buf)
            .map_err(|_| Error::Config("OS 난수를 얻지 못했다".to_owned()))?;
        for &byte in &buf {
            let value = usize::from(byte);
            if value < zone && code.len() < CODE_LEN {
                code.push(char::from(alphabet[value % size]));
            }
        }
    }
    buf.zeroize();
    Ok(PairingCode(code))
}

#[cfg(test)]
mod tests {
    use super::{CODE_ALPHABET, CODE_LEN, constant_time_eq, random_code};

    #[test]
    fn 상수_시간_비교는_같을_때만_참이다() {
        assert!(constant_time_eq(b"ABCDEFGH", b"ABCDEFGH"));
        assert!(!constant_time_eq(b"ABCDEFGH", b"ABCDEFGZ"));
        assert!(!constant_time_eq(b"ABCDEFGH", b"ABCDEFG"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn 코드_글자는_글자표에_고르게_퍼진다() {
        let mut counts = [0usize; 32];
        let rounds = 4000;
        for _ in 0..rounds {
            let code = random_code().unwrap();
            assert_eq!(code.as_str().len(), CODE_LEN);
            for c in code.as_str().chars() {
                let index = CODE_ALPHABET.find(c).unwrap();
                counts[index] += 1;
            }
        }
        // 기댓값 1000(= 4000 × 8 / 32). 치우침이 크면(±30% 밖) 실패한다.
        let expected = rounds * CODE_LEN / CODE_ALPHABET.len();
        for count in counts {
            assert!(
                count > expected * 7 / 10 && count < expected * 13 / 10,
                "{counts:?}"
            );
        }
    }
}
