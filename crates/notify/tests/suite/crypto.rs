//! 웹 푸시 암호화(RFC 8291 aes128gcm). 받는 쪽(시험용 구독자)이 자기 키로 풀어 본다.

use pi_notify::{Error, MAX_PLAINTEXT, encrypt_aes128gcm, encrypt_aes128gcm_with};

use crate::common::Subscriber;

// 검증: N5
#[test]
fn 암호화한_본문을_구독자가_자기_키로_푼다() {
    let sub = Subscriber::new(3);
    let body = encrypt_aes128gcm("승인 요청이 왔다".as_bytes(), &sub.public, &sub.auth).unwrap();
    assert_eq!(sub.open(&body), "승인 요청이 왔다");
}

// 검증: N5
#[test]
fn 머리는_소금_레코드_크기_4096_키_길이_65_보낸_쪽_공개키다() {
    let sub = Subscriber::new(3);
    let body = encrypt_aes128gcm(b"hello", &sub.public, &sub.auth).unwrap();
    assert_eq!(&body[16..20], &4096u32.to_be_bytes());
    assert_eq!(body[20], 65);
    assert_eq!(body[21], 4, "비압축 점");
    // 짧은 평문 한 레코드: 머리 86 + 평문 5 + 구분 1 + 태그 16.
    assert_eq!(body.len(), 86 + 5 + 1 + 16);
}

// 검증: N5
#[test]
fn 같은_글도_매번_소금과_임시_키가_새로_나온다() {
    let sub = Subscriber::new(3);
    let a = encrypt_aes128gcm(b"same", &sub.public, &sub.auth).unwrap();
    let b = encrypt_aes128gcm(b"same", &sub.public, &sub.auth).unwrap();
    assert_ne!(a[..16], b[..16], "소금");
    assert_ne!(a[21..86], b[21..86], "보낸 쪽 공개키");
    assert_eq!(sub.open(&a), "same");
    assert_eq!(sub.open(&b), "same");
}

// 검증: N5
#[test]
fn 임시_키와_소금을_정하면_결과가_같고_머리에_그_값이_들어간다() {
    let sub = Subscriber::new(3);
    let salt = [9u8; 16];
    let as_private = [7u8; 32];
    let a = encrypt_aes128gcm_with(b"fixed", &sub.public, &sub.auth, &as_private, &salt).unwrap();
    let b = encrypt_aes128gcm_with(b"fixed", &sub.public, &sub.auth, &as_private, &salt).unwrap();
    assert_eq!(a, b);
    assert_eq!(&a[..16], &salt);
    let expected_public = p256::SecretKey::from_slice(&as_private)
        .unwrap()
        .public_key()
        .to_sec1_bytes();
    assert_eq!(&a[21..86], expected_public.as_ref());
    assert_eq!(sub.open(&a), "fixed");
    let other_salt =
        encrypt_aes128gcm_with(b"fixed", &sub.public, &sub.auth, &as_private, &[1u8; 16]).unwrap();
    assert_ne!(a, other_salt);
}

// 검증: N5
#[test]
fn 빈_글과_한글과_줄바꿈도_그대로_풀린다() {
    let sub = Subscriber::new(5);
    for text in ["", "한글 😀 emoji\n둘째 줄\0끝", "{\"a\":1}"] {
        let body = encrypt_aes128gcm(text.as_bytes(), &sub.public, &sub.auth).unwrap();
        assert_eq!(sub.open(&body), text);
    }
}

// 검증: N5
#[test]
fn 한_레코드에_들어가는_가장_긴_글까지_되고_하나_더_길면_거절한다() {
    assert_eq!(MAX_PLAINTEXT, 3993);
    let sub = Subscriber::new(5);
    let longest = vec![b'a'; MAX_PLAINTEXT];
    let body = encrypt_aes128gcm(&longest, &sub.public, &sub.auth).unwrap();
    assert_eq!(body.len(), 4096);
    assert_eq!(sub.open(&body).len(), MAX_PLAINTEXT);

    let too_long = vec![b'a'; MAX_PLAINTEXT + 1];
    match encrypt_aes128gcm(&too_long, &sub.public, &sub.auth) {
        Err(Error::PayloadTooLarge { len, max }) => {
            assert_eq!(len, MAX_PLAINTEXT + 1);
            assert_eq!(max, MAX_PLAINTEXT);
        }
        other => panic!("{other:?}"),
    }
}

// 검증: N5
#[test]
fn 구독자_키가_틀리면_거절한다() {
    let sub = Subscriber::new(5);
    // 압축된 점(33바이트)은 받지 않는다. 구독의 공개키는 비압축 65바이트다.
    let mut compressed = vec![2 + (sub.public[64] & 1)];
    compressed.extend_from_slice(&sub.public[1..33]);
    let bad_publics: [Vec<u8>; 4] = [
        sub.public[..64].to_vec(),
        Vec::new(),
        [vec![4u8], vec![0xFF; 64]].concat(),
        compressed,
    ];
    for public in &bad_publics {
        assert!(
            matches!(
                encrypt_aes128gcm(b"x", public, &sub.auth),
                Err(Error::InvalidKey(_))
            ),
            "{} 바이트",
            public.len()
        );
    }
    assert!(matches!(
        encrypt_aes128gcm(b"x", &sub.public, &sub.auth[..15]),
        Err(Error::InvalidKey(_))
    ));
    assert!(matches!(
        encrypt_aes128gcm(b"x", &sub.public, &[]),
        Err(Error::InvalidKey(_))
    ));
}

// 검증: N5
#[test]
fn 정한_임시_키와_소금의_길이가_틀리면_거절한다() {
    let sub = Subscriber::new(5);
    assert!(matches!(
        encrypt_aes128gcm_with(b"x", &sub.public, &sub.auth, &[7u8; 31], &[9u8; 16]),
        Err(Error::InvalidKey(_))
    ));
    assert!(matches!(
        encrypt_aes128gcm_with(b"x", &sub.public, &sub.auth, &[7u8; 32], &[9u8; 15]),
        Err(Error::InvalidKey(_))
    ));
    assert!(matches!(
        encrypt_aes128gcm_with(b"x", &sub.public, &sub.auth, &[0u8; 32], &[9u8; 16]),
        Err(Error::InvalidKey(_))
    ));
}

/// RFC 8291 부록 A의 값(base64url, 패딩 없음).
mod rfc8291_appendix_a {
    pub const PLAINTEXT: &str = "V2hlbiBJIGdyb3cgdXAsIEkgd2FudCB0byBiZSBhIHdhdGVybWVsb24";
    pub const AS_PUBLIC: &str =
        "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
    pub const AS_PRIVATE: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";
    pub const UA_PUBLIC: &str =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    pub const SALT: &str = "DGv6ra1nlYgDCS1FRnbzlw";
    pub const AUTH_SECRET: &str = "BTBZMqHH6r4Tts7J_aSIgg";
    pub const BODY: &str = "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN";
}

// 검증: N5
#[test]
fn 표준_부록_a의_시험값과_한_바이트도_다르지_않다() {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use rfc8291_appendix_a as a;

    let b64 = |text: &str| URL_SAFE_NO_PAD.decode(text).unwrap();
    let plaintext = b64(a::PLAINTEXT);
    assert_eq!(plaintext, b"When I grow up, I want to be a watermelon");
    let body = encrypt_aes128gcm_with(
        &plaintext,
        &b64(a::UA_PUBLIC),
        &b64(a::AUTH_SECRET),
        &b64(a::AS_PRIVATE),
        &b64(a::SALT),
    )
    .unwrap();
    assert_eq!(URL_SAFE_NO_PAD.encode(&body), a::BODY);
    assert_eq!(&body[21..86], b64(a::AS_PUBLIC).as_slice());
}
