//! VAPID 서명과 키 파일(RFC 8292, 결정 2026-11-16 24번).

use std::fs;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use pi_notify::{Error, VAPID_KEY_FILE, VapidKey};
use pi_store::UnixMillis;

use crate::common::{PUSH_ORIGIN, SEC, SUBJECT, T0, check_vapid};

// 검증: N5
#[test]
fn 공개키는_65바이트_비압축_점이고_base64url_패딩_없음이다() {
    let key = VapidKey::generate();
    let public = key.public_key();
    assert_eq!(public.len(), 65);
    assert_eq!(public[0], 4);
    let text = key.public_key_b64url();
    assert_eq!(text, URL_SAFE_NO_PAD.encode(&public));
    assert!(!text.contains('='));
}

// 검증: N5
#[test]
fn 새로_만든_키는_서로_다르다() {
    assert_ne!(
        VapidKey::generate().public_key(),
        VapidKey::generate().public_key()
    );
}

// 검증: N5
#[test]
fn 서명한_jwt가_공개키로_검증되고_내용이_맞다() {
    let key = VapidKey::generate();
    let now = UnixMillis(T0);
    let jwt = key.sign_jwt(PUSH_ORIGIN, SUBJECT, now);
    let header = format!("vapid t={jwt}, k={}", key.public_key_b64url());
    let claims = check_vapid(&header, &key.public_key_b64url());
    assert_eq!(claims.aud, PUSH_ORIGIN);
    assert_eq!(claims.sub, SUBJECT);
    assert_eq!(claims.header["typ"], "JWT");
    assert_eq!(claims.header["alg"], "ES256");
    let now_secs = T0 / SEC;
    assert!(claims.exp > now_secs, "만료는 지금보다 뒤다");
    assert!(claims.exp <= now_secs + 24 * 3_600, "24시간을 넘지 않는다");
}

// 검증: N5
#[test]
fn 인증_머리글의_aud는_주소의_origin이다() {
    let key = VapidKey::generate();
    let header = key
        .authorization("https://push.invalid/send/abc?x=1", SUBJECT, UnixMillis(T0))
        .unwrap();
    assert_eq!(
        check_vapid(&header, &key.public_key_b64url()).aud,
        PUSH_ORIGIN
    );
    let header = key
        .authorization("https://push.invalid:8443/a/b", SUBJECT, UnixMillis(T0))
        .unwrap();
    assert_eq!(
        check_vapid(&header, &key.public_key_b64url()).aud,
        "https://push.invalid:8443"
    );
    assert!(matches!(
        key.authorization("not a url", SUBJECT, UnixMillis(T0)),
        Err(Error::InvalidKey(_))
    ));
}

// 검증: N5
#[test]
fn 다른_키로_서명한_것은_이_키의_공개키로_검증되지_않는다() {
    let signer = VapidKey::generate();
    let other = VapidKey::generate();
    let jwt = signer.sign_jwt(PUSH_ORIGIN, SUBJECT, UnixMillis(T0));
    let parts: Vec<&str> = jwt.split('.').collect();
    assert_eq!(parts.len(), 3);
    let signed = format!("{}.{}", parts[0], parts[1]);
    let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
    let wrong = VerifyingKey::from_sec1_bytes(&other.public_key()).unwrap();
    assert!(wrong.verify(signed.as_bytes(), &signature).is_err());
    let right = VerifyingKey::from_sec1_bytes(&signer.public_key()).unwrap();
    assert!(right.verify(signed.as_bytes(), &signature).is_ok());
}

// 검증: N5
#[test]
fn 처음_켜면_키_파일을_만들고_다시_켜면_같은_키다() {
    let dir = tempfile::tempdir().unwrap();
    let secret_dir = dir.path().join("비밀").join("폴더");
    let first = VapidKey::load_or_create(&secret_dir).unwrap();
    let file = secret_dir.join(VAPID_KEY_FILE);
    assert!(file.is_file());
    let before = fs::read(&file).unwrap();
    assert!(!before.is_empty());

    let second = VapidKey::load_or_create(&secret_dir).unwrap();
    assert_eq!(first.public_key(), second.public_key());
    assert_eq!(
        fs::read(&file).unwrap(),
        before,
        "다시 열어도 파일이 그대로다"
    );
}

#[cfg(unix)]
// 검증: N5
#[test]
fn 키_파일은_0600이고_넓으면_좁힌다() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    VapidKey::load_or_create(dir.path()).unwrap();
    let file = dir.path().join(VAPID_KEY_FILE);
    let mode = |p: &std::path::Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&file), 0o600);

    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    VapidKey::load_or_create(dir.path()).unwrap();
    assert_eq!(mode(&file), 0o600);
}

// 검증: N5
#[test]
fn 키가_아닌_파일은_거절하고_덮어쓰지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(VAPID_KEY_FILE);
    fs::write(&file, "이것은 키가 아니다").unwrap();
    assert!(matches!(
        VapidKey::load_or_create(dir.path()),
        Err(Error::InvalidKey(_))
    ));
    assert_eq!(fs::read_to_string(&file).unwrap(), "이것은 키가 아니다");
}

// 검증: E38
#[test]
fn 디버그_출력에_비밀_값이_나오지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let key = VapidKey::load_or_create(dir.path()).unwrap();
    let stored = fs::read_to_string(dir.path().join(VAPID_KEY_FILE)).unwrap();
    let secret = stored.trim();
    assert!(!secret.is_empty());
    assert!(!format!("{key:?}").contains(secret));
}

// 검증: N5
#[test]
fn 쓰다_꺼져_빈_키_파일이_남았으면_새로_만들고_다시_켜면_같은_키다() {
    for leftover in ["", "\n", "  \n"] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(VAPID_KEY_FILE);
        fs::write(&file, leftover).unwrap();
        let first = VapidKey::load_or_create(dir.path()).unwrap();
        let written = fs::read_to_string(&file).unwrap();
        assert!(!written.trim().is_empty(), "새 키를 썼다");
        let second = VapidKey::load_or_create(dir.path()).unwrap();
        assert_eq!(first.public_key(), second.public_key());
    }
}

#[cfg(unix)]
// 검증: N5
#[test]
fn 빈_키_파일을_새로_채워도_0600이다() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(VAPID_KEY_FILE);
    fs::write(&file, "").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    VapidKey::load_or_create(dir.path()).unwrap();
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

// 검증: N5
#[test]
fn 키를_만든_뒤_비밀_폴더에는_키_파일_하나만_남는다() {
    let dir = tempfile::tempdir().unwrap();
    VapidKey::load_or_create(dir.path()).unwrap();
    let names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec![VAPID_KEY_FILE.to_owned()],
        "임시 파일이 남지 않는다"
    );
}
