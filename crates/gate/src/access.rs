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
//! - 그 컴퓨터의 요청은 지금처럼 키 없이 들어온다(본문을 보지 않는다).
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
//!
//! # 키 바꾸기 (V41)
//!
//! [`AccessKeys::issue`]가 새 키를 만들고 저장소의 세대를 올린다. 앞 세대로 만든 원격 세션은
//! 모두 끊긴다(보호된 경로 401, 열린 실시간 연결은 닫힌다). 그 컴퓨터의 세션은 그대로다.
//! 앞 키는 더 맞지 않는다.

use std::net::IpAddr;
use std::time::Duration;

use pi_store::UnixMillis;

use crate::GateError;

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
    _private: (),
}

impl AccessKeys {
    /// 새 접속 키를 만든다: 운영체제 난수로 키와 소금을 만들고, 소금 친 SHA-256만
    /// `pi_store::Store::set_access_key`로 새 세대에 둔다. 앞 세대의 원격 세션을 모두 끊고,
    /// 접속 기록에 [`ACCESS_KEY_CHANGED`]를 남긴다.
    ///
    /// 저장에 실패하면 [`GateError::Store`]이고 세션을 끊지 않는다.
    pub async fn issue(&self) -> Result<IssuedKey, GateError> {
        todo!()
    }

    /// 지금 잠긴 기기들(잠금 시간이 지난 것은 빠진다).
    pub fn locked(&self) -> Vec<LockedDevice> {
        todo!()
    }

    /// 그 기기의 잠금과 실패 횟수를 지운다. 잠겨 있었으면 참.
    pub fn unlock(&self, device: IpAddr) -> bool {
        todo!()
    }
}
