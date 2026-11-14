//! pi-secrets 3차 작업 테스트. 기준: `docs/dev/modules/secrets.md`의 "3차 작업" 1 · 2번.
//!
//! 구독 토큰 칸을 없앴다. 예전(1 · 2차)에 쓰던 `auth.json`이 보관 폴더에 남아 있어도
//! 보관소는 그것을 읽어 내주지 않고, 등록된 것으로 보지 않고, 지우지도 고치지도 않는다.
//! 그 파일이 깨져 있어도 API 키 · 증권사 키를 읽는 데 지장이 없다.
//!
//! - 보관 폴더는 모두 임시 폴더다. 실제 홈 폴더와 프로세스 환경 변수를 건드리지 않는다.
//! - 확인기는 가짜다. 바깥 서버를 부르지 않는다.
//! - 비밀 값은 진짜 키처럼 보이지 않는 글자만 쓴다.

use std::fs;
use std::path::Path;

use pi_secrets::{
    Credentials, ENV_FILE, KeyKind, KeyStatus, SecretStore, SecretsError, Verifier, VerifyFailure,
};
use tempfile::tempdir;

/// 예전에 구독 토큰을 두던 파일 이름.
const LEGACY_AUTH_FILE: &str = "auth.json";

const ID_1: &str = "시험용-아이디-0001";
const SECRET_1: &str = "시험용-비밀-값-0001";
const AI_KEY: &str = "시험용-에이아이-값-0001";
const DATA_KEY: &str = "시험용-자료-값-0001";
const OLD_ACCESS: &str = "시험용-옛-접근-토큰-0001";
const OLD_REFRESH: &str = "시험용-옛-갱신-토큰-0001";
const OLD_AI_IN_JSON: &str = "시험용-옛-제이슨-에이아이-값-0001";

/// 1 · 2차 보관소가 쓰던 모양의 `auth.json`: 구독 토큰 묶음과, 같은 모양의 AI 키 묶음.
fn legacy_auth_json() -> String {
    format!(
        "{{\n  \"ai_oauth\": {{\n    \"access_token\": \"{OLD_ACCESS}\",\n    \
         \"refresh_token\": \"{OLD_REFRESH}\"\n  }},\n  \
         \"ai_api_key\": {{\n    \"api_key\": \"{OLD_AI_IN_JSON}\"\n  }}\n}}\n"
    )
}

/// 깨진 `auth.json` 여러 모양: 잘린 JSON, 묶음이 아닌 JSON, UTF-8이 아닌 바이트, 빈 파일.
fn broken_auth_files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "잘린 JSON",
            format!("{{\"ai_oauth\": {{\"access_token\": \"{OLD_ACCESS}").into_bytes(),
        ),
        ("묶음이 아닌 JSON", b"[1, 2, 3]".to_vec()),
        ("UTF-8이 아닌 바이트", vec![b'{', 0xff, 0xfe, b'}']),
        ("빈 파일", Vec::new()),
    ]
}

/// 언제나 통과하는 확인기.
struct Pass;

impl Verifier for Pass {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

fn broker(id: &str, secret: &str) -> Credentials {
    Credentials::new()
        .with("client_id", id)
        .with("client_secret", secret)
}

fn api_key(value: &str) -> Credentials {
    Credentials::new().with("api_key", value)
}

async fn register_ok(store: &SecretStore, kind: KeyKind, values: Credentials) {
    store
        .register(kind, values, &Pass)
        .await
        .expect("확인을 지난 값은 등록된다");
}

/// 세 종류를 모두 등록한다.
async fn register_all(store: &SecretStore) {
    register_ok(store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    register_ok(store, KeyKind::AiApiKey, api_key(AI_KEY)).await;
    register_ok(store, KeyKind::Data, api_key(DATA_KEY)).await;
}

fn value_of(store: &SecretStore, kind: KeyKind, field: &str) -> Option<String> {
    store
        .read(kind)
        .expect("읽기")
        .and_then(|c| c.get(field).map(|v| v.expose().to_string()))
}

fn all_unregistered() -> Vec<KeyStatus> {
    KeyKind::ALL
        .iter()
        .map(|&kind| KeyStatus {
            kind,
            registered: false,
        })
        .collect()
}

fn all_registered() -> Vec<KeyStatus> {
    KeyKind::ALL
        .iter()
        .map(|&kind| KeyStatus {
            kind,
            registered: true,
        })
        .collect()
}

/// 세 종류의 값이 모두 그대로 읽히는지 본다.
fn assert_reads_all(store: &SecretStore, what: &str) {
    assert_eq!(
        value_of(store, KeyKind::Broker, "client_id").as_deref(),
        Some(ID_1),
        "{what}: 증권사 아이디"
    );
    assert_eq!(
        value_of(store, KeyKind::Broker, "client_secret").as_deref(),
        Some(SECRET_1),
        "{what}: 증권사 비밀"
    );
    assert_eq!(
        value_of(store, KeyKind::AiApiKey, "api_key").as_deref(),
        Some(AI_KEY),
        "{what}: AI API 키"
    );
    assert_eq!(
        value_of(store, KeyKind::Data, "api_key").as_deref(),
        Some(DATA_KEY),
        "{what}: 자료 키"
    );
    assert_eq!(
        store.status().expect("상태 보기"),
        all_registered(),
        "{what}: 상태"
    );
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .expect("권한을 볼 파일이 있어야 한다")
        .permissions()
        .mode()
        & 0o777
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("권한 바꾸기");
}

// ───────────────────────── 3차 1번: 구독 토큰 종류가 없다 ─────────────────────────

// 검증: C3
#[test]
fn ai_키는_api_키_하나고_토큰_칸을_가진_종류가_없다() {
    assert_eq!(
        KeyKind::ALL,
        [KeyKind::Broker, KeyKind::AiApiKey, KeyKind::Data]
    );
    assert_eq!(KeyKind::AiApiKey.fields(), ["api_key"]);
    for kind in KeyKind::ALL {
        assert_eq!(kind.file_name(), ENV_FILE, "{kind:?}");
        for field in kind.fields() {
            assert!(
                !field.contains("token"),
                "{kind:?}에 토큰 칸이 있다: {field}"
            );
        }
    }
}

// 검증: C3, E4
#[tokio::test]
async fn ai_api_키_종류로_구독_토큰_칸을_넣으면_거부하고_파일을_만들지_않는다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    let tokens = Credentials::new()
        .with("access_token", OLD_ACCESS)
        .with("refresh_token", OLD_REFRESH);

    let result = store.register(KeyKind::AiApiKey, tokens, &Pass).await;

    assert!(
        matches!(
            result,
            Err(SecretsError::MissingField { .. } | SecretsError::UnknownField)
        ),
        "구독 토큰 칸이 받아들여졌다: {result:?}"
    );
    assert!(!tmp.path().join(LEGACY_AUTH_FILE).exists());
    assert!(!tmp.path().join(ENV_FILE).exists());
    assert_eq!(store.status().unwrap(), all_unregistered());
}

// ───────────────────────── 3차 2번: 옛 auth.json은 건드리지 않는다 ─────────────────────────

// 검증: D3, C3, V46
#[tokio::test]
async fn 옛_auth_json의_토큰은_읽어_내주지_않고_등록된_것으로_보지_않는다() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join(LEGACY_AUTH_FILE), legacy_auth_json()).unwrap();

    let store = SecretStore::open(tmp.path()).unwrap();

    assert_eq!(store.status().unwrap(), all_unregistered());
    for kind in KeyKind::ALL {
        assert!(
            store.read(kind).unwrap().is_none(),
            "{kind:?}가 옛 auth.json에서 읽혔다"
        );
    }

    // 다시 열어도 같다.
    drop(store);
    let reopened = SecretStore::open(tmp.path()).unwrap();
    assert_eq!(reopened.status().unwrap(), all_unregistered());
    assert!(reopened.read(KeyKind::AiApiKey).unwrap().is_none());
}

// 검증: D3, C3, C16
#[tokio::test]
async fn 옛_auth_json이_있어도_키는_env에_등록되고_auth_json의_값이_섞이지_않는다() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join(LEGACY_AUTH_FILE), legacy_auth_json()).unwrap();

    let store = SecretStore::open(tmp.path()).unwrap();
    register_all(&store).await;

    assert_reads_all(&store, "등록 뒤");
    let env = fs::read_to_string(tmp.path().join(ENV_FILE)).unwrap();
    for old in [OLD_ACCESS, OLD_REFRESH, OLD_AI_IN_JSON] {
        assert!(!env.contains(old), "옛 auth.json의 값이 .env로 옮겨졌다");
    }

    drop(store);
    let reopened = SecretStore::open(tmp.path()).unwrap();
    assert_reads_all(&reopened, "다시 연 뒤");
}

// 검증: D3, N8
#[tokio::test]
async fn 옛_auth_json은_열고_등록하고_읽고_다시_열어도_한_바이트도_바뀌지_않는다() {
    let tmp = tempdir().unwrap();
    let auth = tmp.path().join(LEGACY_AUTH_FILE);
    let before = legacy_auth_json().into_bytes();
    fs::write(&auth, &before).unwrap();

    {
        let store = SecretStore::open(tmp.path()).unwrap();
        register_all(&store).await;
        register_ok(
            &store,
            KeyKind::AiApiKey,
            api_key("시험용-에이아이-값-0002"),
        )
        .await;
        let _ = store.read(KeyKind::AiApiKey).unwrap();
        let _ = store.status().unwrap();
        let _ = store.redact(OLD_ACCESS);
        let failed = store
            .register(
                KeyKind::Broker,
                broker("시험용-아이디-0009", "시험용-비밀-값-0009"),
                &Rejects,
            )
            .await;
        assert!(matches!(failed, Err(SecretsError::VerifyFailed(_))));
    }
    let reopened = SecretStore::open(tmp.path()).unwrap();
    let _ = reopened.status().unwrap();

    assert!(auth.is_file(), "옛 auth.json이 지워졌다");
    assert_eq!(fs::read(&auth).unwrap(), before, "옛 auth.json이 고쳐졌다");
    let mut names: Vec<String> = fs::read_dir(tmp.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![ENV_FILE.to_string(), LEGACY_AUTH_FILE.to_string()]
    );
}

/// 언제나 거부하는 확인기.
struct Rejects;

impl Verifier for Rejects {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Err(VerifyFailure::Rejected)
    }
}

// 검증: D3, N8
#[cfg(unix)]
#[tokio::test]
async fn 옛_auth_json의_권한도_바꾸지_않는다() {
    let tmp = tempdir().unwrap();
    let auth = tmp.path().join(LEGACY_AUTH_FILE);
    fs::write(&auth, legacy_auth_json()).unwrap();
    set_mode(&auth, 0o644);

    {
        let store = SecretStore::open(tmp.path()).unwrap();
        register_all(&store).await;
        let _ = store.read(KeyKind::AiApiKey).unwrap();
        let _ = store.status().unwrap();
    }
    let reopened = SecretStore::open(tmp.path()).unwrap();
    let _ = reopened.status().unwrap();

    assert_eq!(mode_of(&auth), 0o644, "옛 auth.json의 권한이 바뀌었다");
    // 보관소가 쓰는 파일은 여전히 0600이다.
    assert_eq!(mode_of(&tmp.path().join(ENV_FILE)), 0o600);
}

// 검증: D3, C3, E4
#[tokio::test]
async fn 깨진_옛_auth_json이_있어도_등록한_키를_모두_읽는다() {
    for (what, broken) in broken_auth_files() {
        let tmp = tempdir().unwrap();
        {
            let store = SecretStore::open(tmp.path()).unwrap();
            register_all(&store).await;
        }
        let auth = tmp.path().join(LEGACY_AUTH_FILE);
        fs::write(&auth, &broken).unwrap();

        let store = SecretStore::open(tmp.path())
            .unwrap_or_else(|e| panic!("{what}: 깨진 auth.json 때문에 열지 못했다: {e:?}"));
        assert_reads_all(&store, what);

        assert_eq!(
            fs::read(&auth).unwrap(),
            broken,
            "{what}: 깨진 auth.json이 고쳐졌다"
        );
    }
}

// 검증: D3, C3, C16
#[tokio::test]
async fn 깨진_옛_auth_json이_있어도_새로_등록하고_다시_열어_읽는다() {
    for (what, broken) in broken_auth_files() {
        let tmp = tempdir().unwrap();
        let auth = tmp.path().join(LEGACY_AUTH_FILE);
        fs::write(&auth, &broken).unwrap();

        {
            let store = SecretStore::open(tmp.path())
                .unwrap_or_else(|e| panic!("{what}: 열지 못했다: {e:?}"));
            assert_eq!(store.status().unwrap(), all_unregistered(), "{what}");
            register_all(&store).await;
            assert_reads_all(&store, what);
        }
        let reopened = SecretStore::open(tmp.path()).unwrap();
        assert_reads_all(&reopened, what);

        assert_eq!(
            fs::read(&auth).unwrap(),
            broken,
            "{what}: 깨진 auth.json이 고쳐졌다"
        );
    }
}

// 검증: N8, V46, E38
#[tokio::test]
async fn 깨진_옛_auth_json의_내용이_결과나_오류에_실리지_않는다() {
    let tmp = tempdir().unwrap();
    let broken = "시험용-깨진-내용-0001";
    fs::write(
        tmp.path().join(LEGACY_AUTH_FILE),
        format!("{{\"{broken}\": \"{broken}"),
    )
    .unwrap();

    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::AiApiKey, api_key(AI_KEY)).await;

    let shown = [
        format!("{store:?}"),
        format!("{:?}", store.status()),
        format!("{:?}", store.read(KeyKind::AiApiKey)),
        format!("{:?}", store.read(KeyKind::Broker)),
    ];
    for text in &shown {
        assert!(!text.contains(broken), "깨진 파일의 내용이 실렸다: {text}");
    }
}
