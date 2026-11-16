//! 웹 푸시 본문 암호화: RFC 8291 `aes128gcm`(한 레코드). 순수 Rust(p256 · hkdf · aes-gcm).

use crate::Error;

/// 한 레코드(4096바이트)에 넣을 수 있는 가장 긴 평문(바이트). 머리 86 + 구분 1 + 태그 16을 뺀 3993.
pub const MAX_PLAINTEXT: usize = 3993;

/// 구독자(브라우저)의 공개키와 인증 값으로 `plaintext`를 암호화해 요청 본문(머리 + 암호문)을 만든다.
/// 보낸 쪽 임시 키와 소금은 매번 새로 만든다.
///
/// - `ua_public`: 구독의 `p256dh`를 풀어 얻은 65바이트(비압축 점, 첫 바이트 0x04).
/// - `auth_secret`: 구독의 `auth`를 풀어 얻은 16바이트.
/// - 평문이 [`MAX_PLAINTEXT`]를 넘으면 [`Error::PayloadTooLarge`]. 키 길이 · 점이 틀리면 [`Error::InvalidKey`].
pub fn encrypt_aes128gcm(
    plaintext: &[u8],
    ua_public: &[u8],
    auth_secret: &[u8],
) -> Result<Vec<u8>, Error> {
    let _ = (plaintext, ua_public, auth_secret);
    todo!()
}

/// [`encrypt_aes128gcm`]과 같되 보낸 쪽 임시 비밀키(32바이트)와 소금(16바이트)을 받는다.
/// 같은 입력은 같은 결과다(표준 부록 A의 시험값으로 확인하는 길).
pub fn encrypt_aes128gcm_with(
    plaintext: &[u8],
    ua_public: &[u8],
    auth_secret: &[u8],
    as_private: &[u8],
    salt: &[u8],
) -> Result<Vec<u8>, Error> {
    let _ = (plaintext, ua_public, auth_secret, as_private, salt);
    todo!()
}
