//! 웹 푸시 본문 암호화: RFC 8291 `aes128gcm`(한 레코드). 순수 Rust(p256 · hkdf · aes-gcm).

use aes_gcm::aead::Aead;
use aes_gcm::{Aes128Gcm, KeyInit, Nonce};
use hkdf::Hkdf;
use p256::{PublicKey, SecretKey};
use sha2::Sha256;

use crate::Error;

/// 한 레코드(4096바이트)에 넣을 수 있는 가장 긴 평문(바이트). 머리 86 + 구분 1 + 태그 16을 뺀 3993.
pub const MAX_PLAINTEXT: usize = 3993;

/// 레코드 크기(`rs`). 한 레코드만 쓴다.
const RECORD_SIZE: u32 = 4096;
/// 비압축 점의 길이.
const POINT_LEN: usize = 65;
/// 인증 값의 길이.
const AUTH_LEN: usize = 16;
/// 소금의 길이.
const SALT_LEN: usize = 16;
/// 보낸 쪽 임시 비밀키의 길이.
const PRIVATE_LEN: usize = 32;
/// 마지막 레코드의 구분 글자.
const LAST_RECORD_DELIMITER: u8 = 0x02;

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
    let salt: [u8; SALT_LEN] = rand::random();
    let as_secret = random_secret_key();
    encrypt_with_key(plaintext, ua_public, auth_secret, &as_secret, &salt)
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
    if as_private.len() != PRIVATE_LEN {
        return Err(Error::InvalidKey(
            "보낸 쪽 임시 키는 32바이트여야 한다".to_owned(),
        ));
    }
    let as_secret = SecretKey::from_slice(as_private)
        .map_err(|_| Error::InvalidKey("보낸 쪽 임시 키가 곡선의 비밀키가 아니다".to_owned()))?;
    encrypt_with_key(plaintext, ua_public, auth_secret, &as_secret, salt)
}

/// 구독의 공개키를 확인한다: 65바이트 비압축 점이고 곡선 위에 있어야 한다.
pub(crate) fn parse_ua_public(ua_public: &[u8]) -> Result<PublicKey, Error> {
    if ua_public.len() != POINT_LEN || ua_public.first() != Some(&4) {
        return Err(Error::InvalidKey(
            "구독 공개키는 65바이트 비압축 점이어야 한다".to_owned(),
        ));
    }
    PublicKey::from_sec1_bytes(ua_public)
        .map_err(|_| Error::InvalidKey("구독 공개키가 곡선 위의 점이 아니다".to_owned()))
}

/// 구독의 인증 값을 확인한다: 16바이트.
pub(crate) fn check_auth_secret(auth_secret: &[u8]) -> Result<(), Error> {
    if auth_secret.len() == AUTH_LEN {
        Ok(())
    } else {
        Err(Error::InvalidKey(
            "구독 인증 값은 16바이트여야 한다".to_owned(),
        ))
    }
}

/// 운영체제 난수로 비밀키를 만든다(0이거나 곡선 차수 이상이면 다시 뽑는다).
pub(crate) fn random_secret_key() -> SecretKey {
    loop {
        let bytes: [u8; PRIVATE_LEN] = rand::random();
        if let Ok(key) = SecretKey::from_slice(&bytes) {
            return key;
        }
    }
}

fn encrypt_with_key(
    plaintext: &[u8],
    ua_public: &[u8],
    auth_secret: &[u8],
    as_secret: &SecretKey,
    salt: &[u8],
) -> Result<Vec<u8>, Error> {
    if plaintext.len() > MAX_PLAINTEXT {
        return Err(Error::PayloadTooLarge {
            len: plaintext.len(),
            max: MAX_PLAINTEXT,
        });
    }
    let ua_key = parse_ua_public(ua_public)?;
    check_auth_secret(auth_secret)?;
    if salt.len() != SALT_LEN {
        return Err(Error::InvalidKey("소금은 16바이트여야 한다".to_owned()));
    }
    let as_public = as_secret.public_key().to_sec1_bytes();

    // RFC 8291 3.3 · 3.4: 공유 비밀 → IKM, RFC 8188 2.2 · 2.3: CEK · NONCE.
    let shared = p256::ecdh::diffie_hellman(as_secret.to_nonzero_scalar(), ua_key.as_affine());
    let mut key_info = Vec::with_capacity(14 + 2 * POINT_LEN);
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(ua_public);
    key_info.extend_from_slice(&as_public);
    let mut ikm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(auth_secret), shared.raw_secret_bytes())
        .expand(&key_info, &mut ikm)
        .map_err(|_| Error::InvalidKey("키 유도 실패".to_owned()))?;
    let prk = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut cek = [0u8; 16];
    let mut nonce = [0u8; 12];
    prk.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .and_then(|()| prk.expand(b"Content-Encoding: nonce\0", &mut nonce))
        .map_err(|_| Error::InvalidKey("키 유도 실패".to_owned()))?;

    let mut record = Vec::with_capacity(plaintext.len() + 1);
    record.extend_from_slice(plaintext);
    record.push(LAST_RECORD_DELIMITER);
    let cipher = Aes128Gcm::new_from_slice(&cek)
        .map_err(|_| Error::InvalidKey("내용 키 길이가 틀리다".to_owned()))?;
    let nonce = Nonce::try_from(nonce.as_slice())
        .map_err(|_| Error::InvalidKey("nonce 길이가 틀리다".to_owned()))?;
    let sealed = cipher
        .encrypt(&nonce, record.as_slice())
        .map_err(|_| Error::InvalidKey("암호화 실패".to_owned()))?;

    let mut body = Vec::with_capacity(SALT_LEN + 5 + POINT_LEN + sealed.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(POINT_LEN as u8);
    body.extend_from_slice(&as_public);
    body.extend_from_slice(&sealed);
    Ok(body)
}
