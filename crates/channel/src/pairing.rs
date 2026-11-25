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

use chrono::{DateTime, Utc};

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
        let _ = (&self.book, inbound, now);
        Ok(Judgement::Ignored {
            reason: IgnoreReason::TooManyPending,
        })
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
        let _ = (code, now);
        Err(ApproveError::WrongCode)
    }

    /// 허용 목록에서 뺀다. 있었으면 `true`. 뺀 사람은 다시 처음 보는 사람이다.
    pub fn revoke(&self, sender: &SenderId) -> Result<bool, Error> {
        let _ = sender;
        Ok(false)
    }

    /// 지금 허용된 사람(없으면 `None`).
    pub fn allowed(&self) -> Result<Option<SenderId>, Error> {
        Ok(None)
    }
}
