//! 6번: `price-inha doctor`의 키 줄 끝에 텔레그램 봇 토큰의 등록 여부(있음 · 없음)를 덧붙인다. 값 · 길이 · 앞뒤 글자는
//! 싣지 않는다(V46 · N8). 앞의 글은 그대로다(뒤에 덧붙이기만).

use pi_secrets::{Credentials, SecretStore};
use price_inha::TELEGRAM_KEYS;

use crate::doctor_cli::{AcceptAll, home_with_keys, keys_line};

/// 시험용 봇 토큰(진짜 토큰이 아니다).
const BOT_TOKEN: &str = "round42e-telegram-token-not-real-zz";

// 검증: D1 V46 V87
#[tokio::test]
async fn doctor_키_줄_끝에_텔레그램_없음을_덧붙인다() {
    let home = tempfile::tempdir().unwrap();
    home_with_keys(home.path()).await;
    let line = keys_line(home.path());
    assert!(line.ends_with(" · 텔레그램 봇 토큰 없음"), "{line}");
    assert!(
        line.starts_with("증권사 키 있음"),
        "앞의 글은 그대로다: {line}"
    );
}

// 검증: D1 V46 V87
#[tokio::test]
async fn doctor_키_줄_끝에_텔레그램_있음을_값_없이_덧붙인다() {
    let home = tempfile::tempdir().unwrap();
    home_with_keys(home.path()).await;
    let secrets = SecretStore::open(home.path()).unwrap();
    secrets
        .register_provider(
            TELEGRAM_KEYS,
            Credentials::new().with("bot_token", BOT_TOKEN),
            &AcceptAll,
        )
        .await
        .unwrap();
    drop(secrets);
    let line = keys_line(home.path());
    assert!(line.ends_with(" · 텔레그램 봇 토큰 있음"), "{line}");
    assert!(!line.contains(&BOT_TOKEN[..8]), "토큰 값은 싣지 않는다");
    assert!(
        !line.contains(&BOT_TOKEN[BOT_TOKEN.len() - 6..]),
        "토큰 값은 싣지 않는다"
    );
}
