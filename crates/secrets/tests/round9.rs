//! pi-secrets 9차 작업 테스트. 기준: `docs/dev/modules/secrets.md`의 "9차 작업 (2026-11-26 …
//! 공급자 키 지우기)", 결정 24 · A13.
//!
//! - `remove_provider`는 그 공급자의 칸 줄을 모두 지운다. 있었으면 `true`, 없었으면 `false`(오류 아님).
//! - 등록과 같은 원자 쓰기 · 0600. 다른 공급자 · 옛 종류 · 모르는 줄은 그대로.
//! - 지운 뒤 읽기는 `None`, 상태는 등록 안 됨. 지운 값은 이 보관소에서 계속 가린다.
//! - 실패하면 파일은 그대로다.
//!
//! 보관 폴더는 모두 임시 폴더, 확인기는 가짜다. 비밀 값은 진짜 키처럼 보이지 않는 글자만 쓴다.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use pi_secrets::{
    BACKUP_EXCLUDED_FILES, Credentials, ENV_FILE, KeyKind, KeySlot, KeyStatus, ProviderKeys,
    ProviderStatus, REDACTED, SecretStore, SecretsError, Verifier, VerifyFailure,
};
use tempfile::tempdir;

const NAVER: ProviderKeys = ProviderKeys::new("naver_news", &["client_id", "client_secret"]);
const VOYAGE: ProviderKeys = ProviderKeys::new("voyage", &["api_key"]);
const TELEGRAM: ProviderKeys = ProviderKeys::new("telegram", &["bot_token"]);

const NAVER_ID: &str = "시험용-네이버-아이디-0901";
const NAVER_SECRET: &str = "시험용-네이버-비밀-값-0901";
const VOYAGE_KEY: &str = "시험용-보이지-키-값-0901";
const BOT_TOKEN: &str = "시험용-텔레그램-봇-토큰-0901";
const BROKER_ID: &str = "시험용-증권사-아이디-0901";
const BROKER_SECRET: &str = "시험용-증권사-비밀-값-0901";
const AI_KEY: &str = "시험용-인공지능-키-값-0901";

const NAVER_ID_ENV: &str = "PRICE_INHA_KEY__NAVER_NEWS__CLIENT_ID";
const NAVER_SECRET_ENV: &str = "PRICE_INHA_KEY__NAVER_NEWS__CLIENT_SECRET";

/// 모든 확인을 받아들이는 가짜 확인기.
struct Pass;

impl Verifier for Pass {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_slot(
        &self,
        _slot: KeySlot,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_provider(
        &self,
        _keys: ProviderKeys,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

fn open(dir: &Path) -> SecretStore {
    SecretStore::open(dir).unwrap()
}

fn naver(id: &str, secret: &str) -> Credentials {
    Credentials::new()
        .with("client_id", id)
        .with("client_secret", secret)
}

fn one(field: &str, value: &str) -> Credentials {
    Credentials::new().with(field, value)
}

fn env_path(dir: &Path) -> PathBuf {
    dir.join(ENV_FILE)
}

fn read_env(dir: &Path) -> String {
    fs::read_to_string(env_path(dir)).unwrap_or_default()
}

fn write_env(dir: &Path, text: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(env_path(dir), text).unwrap();
}

/// 읽은 공급자 키의 한 칸 값.
fn provider_value(store: &SecretStore, keys: ProviderKeys, field: &str) -> Option<String> {
    store
        .read_provider(keys)
        .unwrap()
        .map(|found| found.get(field).unwrap().expose().to_owned())
}

/// 그 공급자의 등록 여부(`provider_status`).
fn registered(store: &SecretStore, keys: ProviderKeys) -> bool {
    let status = store.provider_status(&[keys]).unwrap();
    assert_eq!(status.len(), 1);
    status[0].registered
}

/// 보관 폴더에서 `needle` 바이트가 든 파일 이름들.
fn files_containing(dir: &Path, needle: &str) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|path| path.is_file())
        .filter(|path| {
            fs::read(path)
                .unwrap()
                .windows(needle.len())
                .any(|window| window == needle.as_bytes())
        })
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// 그 값이 그대로 · URL 인코딩 · Base64로 들어 있어도 가려지고 바이트 검사에 걸린다.
fn assert_hidden(store: &SecretStore, value: &str, what: &str) {
    let encoded = [
        value.to_owned(),
        utf8_percent_encode(value, NON_ALPHANUMERIC).to_string(),
        STANDARD.encode(value),
        URL_SAFE_NO_PAD.encode(value),
    ];
    for shown in &encoded {
        let out = store.redact(&format!("앞 {shown} 뒤"));
        assert!(out.contains(REDACTED), "{what}: 가려지지 않았다");
        for leaked in &encoded {
            assert!(
                !out.contains(leaked.as_str()),
                "{what}: 가린 글에 값이 남았다"
            );
        }
    }
    assert!(
        store.contains_registered(value.as_bytes()),
        "{what}: 바이트 검사에 걸리지 않았다"
    );
}

async fn register_naver(store: &SecretStore) {
    store
        .register_provider(NAVER, naver(NAVER_ID, NAVER_SECRET), &Pass)
        .await
        .unwrap();
}

// ───────────────────────── 1. 지우면 참 · 읽기 없음 · 상태 없음 ─────────────────────────

// 검증: C3 D9
#[tokio::test]
async fn 등록한_공급자_키를_지우면_참이고_읽기와_상태는_없음이다() {
    let tmp = tempdir().unwrap();
    let store = open(tmp.path());
    register_naver(&store).await;
    assert!(registered(&store, NAVER));

    assert!(store.remove_provider(NAVER).unwrap());

    assert!(store.read_provider(NAVER).unwrap().is_none());
    assert_eq!(
        store.provider_status(&[NAVER]).unwrap(),
        vec![ProviderStatus {
            provider: "naver_news",
            registered: false,
        }]
    );
    let text = read_env(tmp.path());
    assert!(!text.contains(NAVER_ID_ENV) && !text.contains(NAVER_SECRET_ENV));
}

// 검증: C3 D9
#[tokio::test]
async fn 다시_연_보관소에서도_지운_공급자_키는_없음이다() {
    let tmp = tempdir().unwrap();
    {
        let store = open(tmp.path());
        register_naver(&store).await;
        assert!(store.remove_provider(NAVER).unwrap());
    }
    let reopened = open(tmp.path());
    assert!(reopened.read_provider(NAVER).unwrap().is_none());
    assert!(!registered(&reopened, NAVER));
}

// 검증: C3 D9
#[tokio::test]
async fn 손으로_적은_칸_하나뿐인_공급자_줄도_지우고_참이다() {
    let tmp = tempdir().unwrap();
    write_env(tmp.path(), &format!("{NAVER_ID_ENV}={NAVER_ID}\n"));
    let store = open(tmp.path());
    assert!(store.read_provider(NAVER).unwrap().is_none());

    assert!(store.remove_provider(NAVER).unwrap());

    assert!(!read_env(tmp.path()).contains(NAVER_ID_ENV));
    assert!(
        !store.remove_provider(NAVER).unwrap(),
        "두 번째는 지울 것이 없다"
    );
}

// ───────────────────────── 2. 없는 것을 지우면 거짓 ─────────────────────────

// 검증: C3
#[test]
fn 키_파일이_없을_때_지우면_거짓이고_파일을_만들지_않는다() {
    let tmp = tempdir().unwrap();
    let store = open(tmp.path());

    assert!(!store.remove_provider(VOYAGE).unwrap());

    assert!(!env_path(tmp.path()).exists());
}

// 검증: C3 D9
#[tokio::test]
async fn 등록되지_않은_공급자를_지우면_거짓이고_파일은_글자_그대로다() {
    let tmp = tempdir().unwrap();
    write_env(tmp.path(), "# 내 설명\nOTHER_NAME=keep\n");
    let store = open(tmp.path());
    register_naver(&store).await;
    let before = fs::read(env_path(tmp.path())).unwrap();

    assert!(!store.remove_provider(VOYAGE).unwrap());

    assert_eq!(fs::read(env_path(tmp.path())).unwrap(), before);
    assert_eq!(
        provider_value(&store, NAVER, "client_id").as_deref(),
        Some(NAVER_ID)
    );
}

// ───────────────────────── 3. 다른 키는 그대로 ─────────────────────────

// 검증: D3 C3 D9
#[tokio::test]
async fn 지워도_다른_공급자와_증권_인공지능_키와_모르는_줄은_그대로다() {
    let tmp = tempdir().unwrap();
    write_env(tmp.path(), "# 내 설명\nOTHER_NAME=keep\n");
    let store = open(tmp.path());
    store
        .register(KeyKind::Broker, naver(BROKER_ID, BROKER_SECRET), &Pass)
        .await
        .unwrap();
    store
        .register(KeyKind::AiApiKey, one("api_key", AI_KEY), &Pass)
        .await
        .unwrap();
    register_naver(&store).await;
    store
        .register_provider(VOYAGE, one("api_key", VOYAGE_KEY), &Pass)
        .await
        .unwrap();
    store
        .register_provider(TELEGRAM, one("bot_token", BOT_TOKEN), &Pass)
        .await
        .unwrap();
    let slots_before = store.slot_status().unwrap();

    assert!(store.remove_provider(TELEGRAM).unwrap());

    let broker = store.read(KeyKind::Broker).unwrap().unwrap();
    assert_eq!(broker.get("client_id").unwrap().expose(), BROKER_ID);
    assert_eq!(broker.get("client_secret").unwrap().expose(), BROKER_SECRET);
    let ai = store.read(KeyKind::AiApiKey).unwrap().unwrap();
    assert_eq!(ai.get("api_key").unwrap().expose(), AI_KEY);
    assert_eq!(
        provider_value(&store, NAVER, "client_secret").as_deref(),
        Some(NAVER_SECRET)
    );
    assert_eq!(
        provider_value(&store, VOYAGE, "api_key").as_deref(),
        Some(VOYAGE_KEY)
    );
    assert!(store.read_provider(TELEGRAM).unwrap().is_none());
    assert_eq!(
        store
            .provider_status(&[NAVER, TELEGRAM, VOYAGE])
            .unwrap()
            .iter()
            .map(|status| status.registered)
            .collect::<Vec<_>>(),
        vec![true, false, true]
    );
    assert!(store.status().unwrap().contains(&KeyStatus {
        kind: KeyKind::Broker,
        registered: true,
    }));
    assert_eq!(store.slot_status().unwrap(), slots_before);
    assert!(read_env(tmp.path()).starts_with("# 내 설명\nOTHER_NAME=keep\n"));
}

// ───────────────────────── 4. 권한 · 파일에 값 없음 ─────────────────────────

// 검증: D3 V46 E39 N8
#[cfg(unix)]
#[tokio::test]
async fn 지운_뒤에도_키_파일은_0600이고_그_값은_어느_파일에도_없고_임시_파일이_남지_않는다() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("보관");
    let store = open(&dir);
    register_naver(&store).await;
    store
        .register_provider(VOYAGE, one("api_key", VOYAGE_KEY), &Pass)
        .await
        .unwrap();

    assert!(store.remove_provider(NAVER).unwrap());

    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&env_path(&dir)), 0o600);
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec![ENV_FILE.to_owned()], "임시 파일이 남지 않는다");
    for value in [NAVER_ID, NAVER_SECRET] {
        assert!(
            files_containing(&dir, value).is_empty(),
            "지운 값이 파일에 남았다"
        );
    }
    for file in files_containing(&dir, VOYAGE_KEY) {
        assert!(BACKUP_EXCLUDED_FILES.contains(&file.as_str()));
    }
}

// 검증: D3 V46
#[cfg(unix)]
#[tokio::test]
async fn 넓은_권한의_키_파일도_지운_뒤에는_0600이다() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempdir().unwrap();
    write_env(
        tmp.path(),
        &format!("OTHER_NAME=keep\nPRICE_INHA_KEY__VOYAGE__API_KEY={VOYAGE_KEY}\n"),
    );
    fs::set_permissions(env_path(tmp.path()), fs::Permissions::from_mode(0o644)).unwrap();
    let store = open(tmp.path());

    assert!(store.remove_provider(VOYAGE).unwrap());

    assert_eq!(mode(&env_path(tmp.path())), 0o600);
    assert_eq!(read_env(tmp.path()), "OTHER_NAME=keep\n");
}

// ───────────────────────── 5. 지운 값은 계속 가린다 ─────────────────────────

// 검증: N8 E38 V46
#[tokio::test]
async fn 지운_값은_이_보관소에서_계속_가려진다() {
    let tmp = tempdir().unwrap();
    let store = open(tmp.path());
    register_naver(&store).await;

    assert!(store.remove_provider(NAVER).unwrap());

    for value in [NAVER_ID, NAVER_SECRET] {
        assert_hidden(&store, value, "지운 직후");
    }
}

// 검증: N8 E38
#[test]
fn 열_때_파일에_있던_값을_지워도_계속_가려진다() {
    let tmp = tempdir().unwrap();
    write_env(
        tmp.path(),
        &format!("PRICE_INHA_KEY__TELEGRAM__BOT_TOKEN={BOT_TOKEN}\n"),
    );
    let store = open(tmp.path());

    assert!(store.remove_provider(TELEGRAM).unwrap());

    assert_hidden(&store, BOT_TOKEN, "열 때 읽은 값을 지운 뒤");
}

// 검증: N8 E38
#[test]
fn 연_뒤_바깥에서_적힌_값을_지워도_가려진다() {
    let tmp = tempdir().unwrap();
    let store = open(tmp.path());
    write_env(
        tmp.path(),
        &format!("PRICE_INHA_KEY__TELEGRAM__BOT_TOKEN={BOT_TOKEN}\n"),
    );

    assert!(store.remove_provider(TELEGRAM).unwrap());

    assert_hidden(&store, BOT_TOKEN, "바깥에서 적힌 값을 지운 뒤");
}

// ───────────────────────── 6. 다시 등록하면 읽힌다 ─────────────────────────

// 검증: C3 C16
#[tokio::test]
async fn 지운_뒤_다시_등록하면_읽힌다() {
    let tmp = tempdir().unwrap();
    let store = open(tmp.path());
    register_naver(&store).await;
    assert!(store.remove_provider(NAVER).unwrap());

    let new_secret = "시험용-네이버-비밀-값-0902";
    store
        .register_provider(NAVER, naver(NAVER_ID, new_secret), &Pass)
        .await
        .unwrap();

    assert_eq!(
        provider_value(&store, NAVER, "client_secret").as_deref(),
        Some(new_secret)
    );
    assert!(registered(&store, NAVER));
    let reopened = open(tmp.path());
    assert_eq!(
        provider_value(&reopened, NAVER, "client_id").as_deref(),
        Some(NAVER_ID)
    );
}

// ───────────────────────── 7. 실패하면 파일은 그대로 ─────────────────────────

// 검증: C3
#[tokio::test]
async fn 이름_규칙을_어긴_선언으로_지우면_오류이고_파일은_그대로다() {
    let tmp = tempdir().unwrap();
    let store = open(tmp.path());
    register_naver(&store).await;
    let before = fs::read(env_path(tmp.path())).unwrap();

    for bad in [
        ProviderKeys::new("Naver_news", &["client_id"]),
        ProviderKeys::new("naver__news", &["client_id"]),
        ProviderKeys::new("naver_news", &[]),
    ] {
        assert!(matches!(
            store.remove_provider(bad),
            Err(SecretsError::InvalidProviderKeys)
        ));
    }

    assert_eq!(fs::read(env_path(tmp.path())).unwrap(), before);
    assert!(registered(&store, NAVER));
}

// 검증: D3 E38
#[test]
fn 키_파일이_깨져_있으면_지우기는_오류이고_파일은_바이트_그대로다() {
    let tmp = tempdir().unwrap();
    let mut bytes = format!("PRICE_INHA_KEY__TELEGRAM__BOT_TOKEN={BOT_TOKEN}\n").into_bytes();
    bytes.extend_from_slice(&[0xff, 0xfe, b'\n']);
    fs::write(env_path(tmp.path()), &bytes).unwrap();
    let store = open(tmp.path());

    let err = store.remove_provider(TELEGRAM).unwrap_err();

    assert!(matches!(err, SecretsError::Corrupt { .. }));
    assert!(
        !format!("{err} {err:?}").contains(BOT_TOKEN),
        "오류에 값이 없다"
    );
    assert_eq!(fs::read(env_path(tmp.path())).unwrap(), bytes);
    assert_eq!(
        fs::read_dir(tmp.path()).unwrap().count(),
        1,
        "임시 파일이 남지 않는다"
    );
}
