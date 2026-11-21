//! pi-secrets 8차 작업 테스트. 기준: `docs/dev/modules/secrets.md`의 "8차 작업 (2026-11-21 …)",
//! `docs/dev/decisions.md` 2026-11-21의 2 · 3 · 4번.
//!
//! - 공급자가 이름과 자기 키 칸을 선언한다([`ProviderKeys`]). 이름 규칙을 어기면 `InvalidProviderKeys`.
//! - 줄 이름은 `PRICE_INHA_KEY__{공급자}__{칸}`(대문자).
//! - 등록 · 읽기 · 상태는 옛 칸과 같은 규칙(칸 검사 → 가리기 대상에 넣기 → 확인 → 원자 쓰기)이다.
//! - 보관소를 열 때 `PRICE_INHA_KEY__`로 시작하는 줄 값은 선언을 몰라도 모두 가린다.
//! - 옛 `register` · `register_slot` · `status` · `slot_status`는 그대로다.
//!
//! 선언 둘(네이버 뉴스 · Voyage)은 이 파일 안에서 `ProviderKeys::new`로 만든다.
//! 보관 폴더는 모두 임시 폴더, 확인기는 가짜다. 비밀 값은 진짜 키처럼 보이지 않는 글자만 쓴다.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use pi_secrets::{
    BACKUP_EXCLUDED_FILES, Credentials, ENV_FILE, KeyKind, KeySlot, KeyStatus, ProviderKeys,
    ProviderRegistered, ProviderStatus, REDACT_PREVIOUS_KEPT, REDACTED, SecretStore, SecretsError,
    SlotStatus, Verifier, VerifyFailure,
};
use tempfile::tempdir;

const NAVER: ProviderKeys = ProviderKeys::new("naver_news", &["client_id", "client_secret"]);
const VOYAGE: ProviderKeys = ProviderKeys::new("voyage", &["api_key"]);

const NAVER_ID: &str = "시험용-네이버-아이디-0001";
const NAVER_SECRET: &str = "시험용-네이버-비밀-값-0001";
const VOYAGE_KEY: &str = "시험용-보이지-키-값-0001";
const OTHER_TOKEN: &str = "시험용-모르는-공급자-토큰-0001";
const BROKER_ID: &str = "시험용-증권사-아이디-0001";
const BROKER_SECRET: &str = "시험용-증권사-비밀-값-0001";
const DART: &str = "시험용-다트-키-값-0001";

const NAVER_ID_ENV: &str = "PRICE_INHA_KEY__NAVER_NEWS__CLIENT_ID";
const NAVER_SECRET_ENV: &str = "PRICE_INHA_KEY__NAVER_NEWS__CLIENT_SECRET";
const VOYAGE_ENV: &str = "PRICE_INHA_KEY__VOYAGE__API_KEY";

/// 확인기가 받은 선언과 칸 · 값.
type Call = (ProviderKeys, Vec<(String, String)>);

/// 공급자 키를 확인하는 가짜 확인기. 정한 결과를 돌려주고 받은 것을 적는다.
/// 보관소를 쥐면 확인하는 동안 그 값이 가려지는지도 적는다.
/// 옛 종류 · 칸은 언제나 받아들인다(같은 보관소에 옛 키도 등록하려고).
struct ProviderVerifier {
    result: Result<(), VerifyFailure>,
    calls: Mutex<Vec<Call>>,
    store: Option<Arc<SecretStore>>,
    /// (본 값, 그 값이 든 글을 `redact`한 결과, `contains_registered` 결과)
    probes: Mutex<Vec<(String, String, bool)>>,
}

impl ProviderVerifier {
    fn new(result: Result<(), VerifyFailure>) -> Self {
        Self {
            result,
            calls: Mutex::new(Vec::new()),
            store: None,
            probes: Mutex::new(Vec::new()),
        }
    }

    fn ok() -> Self {
        Self::new(Ok(()))
    }

    fn watching(store: &Arc<SecretStore>, result: Result<(), VerifyFailure>) -> Self {
        let mut verifier = Self::new(result);
        verifier.store = Some(Arc::clone(store));
        verifier
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    fn probes(&self) -> Vec<(String, String, bool)> {
        self.probes.lock().unwrap().clone()
    }
}

impl Verifier for ProviderVerifier {
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
        keys: ProviderKeys,
        values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        let seen: Vec<(String, String)> = values
            .field_names()
            .into_iter()
            .map(|name| {
                let value = values.get(name).unwrap().expose().to_owned();
                (name.to_owned(), value)
            })
            .collect();
        if let Some(store) = &self.store {
            let mut probes = self.probes.lock().unwrap();
            for (_, value) in &seen {
                let redacted = store.redact(&format!("앞 {value} 뒤"));
                let found = store.contains_registered(value.as_bytes());
                probes.push((value.clone(), redacted, found));
            }
        }
        self.calls.lock().unwrap().push((keys, seen));
        self.result
    }
}

/// 8차 전의 확인기: [`Verifier::verify`]만 구현한다(공급자 확인은 기본 구현).
struct KindOnlyVerifier;

impl Verifier for KindOnlyVerifier {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
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

fn voyage(key: &str) -> Credentials {
    Credentials::new().with("api_key", key)
}

fn env_path(dir: &Path) -> std::path::PathBuf {
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

/// 보관 폴더에서 `needle` 바이트가 든 파일 이름들(정렬).
fn files_containing(dir: &Path, needle: &str) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|e| e.unwrap().path())
        .filter(|path| path.is_file())
        .filter(|path| {
            let bytes = fs::read(path).unwrap();
            bytes
                .windows(needle.len())
                .any(|window| window == needle.as_bytes())
        })
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// 값이 그대로 · URL 인코딩 · Base64로 든 글들.
fn encodings(value: &str) -> Vec<String> {
    vec![
        format!("앞 {value} 뒤"),
        format!(
            "주소?키={}&다음=1",
            utf8_percent_encode(value, NON_ALPHANUMERIC)
        ),
        format!("머리 {} 꼬리", STANDARD.encode(value)),
        format!("머리 {} 꼬리", URL_SAFE_NO_PAD.encode(value)),
    ]
}

/// 그 값이 어떤 모양으로 들어 있어도 가려지고 바이트 검사에 걸린다.
fn assert_hidden(store: &SecretStore, value: &str, what: &str) {
    for text in encodings(value) {
        let out = store.redact(&text);
        assert!(out.contains(REDACTED), "{what}: 가려지지 않았다");
        let plain = utf8_percent_encode(value, NON_ALPHANUMERIC).to_string();
        for leaked in [
            value.to_owned(),
            plain,
            STANDARD.encode(value),
            URL_SAFE_NO_PAD.encode(value),
        ] {
            assert!(!out.contains(&leaked), "{what}: 가린 글에 값이 남았다");
        }
        assert!(
            store.contains_registered(text.as_bytes()),
            "{what}: 바이트 검사가 찾지 못했다"
        );
    }
}

fn assert_not_hidden(store: &SecretStore, value: &str, what: &str) {
    let text = format!("앞 {value} 뒤");
    assert_eq!(store.redact(&text), text, "{what}: 가리지 않아야 한다");
    assert!(
        !store.contains_registered(text.as_bytes()),
        "{what}: 바이트 검사에 걸리지 않아야 한다"
    );
}

/// 오류의 글과 `Debug`에 값이 없다.
fn assert_no_value_in_error(err: &SecretsError, values: &[&str]) {
    let shown = format!("{err} {err:?}");
    for value in values {
        assert!(!shown.contains(value), "오류에 값이 들어갔다");
    }
}

// ───────────────────────── 1. 이름 규칙 ─────────────────────────

/// 33자 이름(규칙 밖).
const NAME_33: &str = "a23456789012345678901234567890123";
/// 32자 이름(경계 안).
const NAME_32: &str = "a2345678901234567890123456789012";

/// 이름 규칙을 어기는 선언들과 그 까닭.
fn invalid_declarations() -> Vec<(ProviderKeys, &'static str)> {
    vec![
        (
            ProviderKeys::new("Naver_news", &["client_id"]),
            "대문자 공급자",
        ),
        (
            ProviderKeys::new("naver__news", &["client_id"]),
            "밑줄 둘 공급자",
        ),
        (ProviderKeys::new("", &["api_key"]), "빈 공급자 이름"),
        (ProviderKeys::new(NAME_33, &["api_key"]), "33자 공급자"),
        (ProviderKeys::new("1voyage", &["api_key"]), "숫자로 시작"),
        (ProviderKeys::new("_voyage", &["api_key"]), "밑줄로 시작"),
        (ProviderKeys::new("voyage_", &["api_key"]), "밑줄로 끝남"),
        (ProviderKeys::new("voy-age", &["api_key"]), "붙임표"),
        (ProviderKeys::new("보이지", &["api_key"]), "영문 밖 글자"),
        (ProviderKeys::new("voyage", &[]), "칸 0개"),
        (
            ProviderKeys::new(
                "voyage",
                &["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9"],
            ),
            "칸 9개",
        ),
        (
            ProviderKeys::new("voyage", &["api_key", "api_key"]),
            "겹친 칸",
        ),
        (ProviderKeys::new("voyage", &["Api_key"]), "대문자 칸"),
        (ProviderKeys::new("voyage", &["api__key"]), "밑줄 둘 칸"),
        (ProviderKeys::new("voyage", &[""]), "빈 칸 이름"),
        (ProviderKeys::new("voyage", &[NAME_33]), "33자 칸"),
        (ProviderKeys::new("voyage", &["9key"]), "숫자로 시작하는 칸"),
        (
            ProviderKeys::new("voyage", &["api_key_"]),
            "밑줄로 끝나는 칸",
        ),
    ]
}

// 검증: C3 D9
#[test]
fn 네이버와_보이지_선언은_이름_규칙을_지난다() {
    assert_eq!(NAVER.check().ok(), Some(()));
    assert_eq!(VOYAGE.check().ok(), Some(()));
    assert_eq!(NAVER.provider(), "naver_news");
    assert_eq!(NAVER.fields(), &["client_id", "client_secret"]);
    assert_eq!(VOYAGE.provider(), "voyage");
    assert_eq!(VOYAGE.fields(), &["api_key"]);
}

// 검증: C3
#[test]
fn 이름_규칙의_경계_안쪽은_지난다() {
    assert_eq!(NAME_32.len(), 32);
    assert_eq!(NAME_33.len(), 33);
    let edges = [
        ProviderKeys::new("a", &["k"]),
        ProviderKeys::new(NAME_32, &[NAME_32]),
        ProviderKeys::new("v2_api", &["key_2"]),
        ProviderKeys::new("voyage", &["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8"]),
    ];
    for keys in edges {
        assert_eq!(keys.check().ok(), Some(()), "{keys:?}");
    }
}

// 검증: C3 E38
#[test]
fn 이름_규칙을_어긴_선언은_거부하고_오류에_이름을_싣지_않는다() {
    for (keys, why) in invalid_declarations() {
        let err = keys.check().expect_err(why);
        assert!(
            matches!(err, SecretsError::InvalidProviderKeys),
            "{why}: {err:?}"
        );
        if !keys.provider().is_empty() {
            assert_no_value_in_error(&err, &[keys.provider()]);
        }
    }
}

// 검증: C3 C16 E4
#[tokio::test]
async fn 이름_규칙을_어긴_선언으로는_확인기를_부르지_않고_저장하지_않는다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = ProviderVerifier::ok();

    for (keys, why) in invalid_declarations() {
        let values = keys
            .fields()
            .iter()
            .fold(Credentials::new(), |acc, field| acc.with(field, VOYAGE_KEY));
        let result = store.register_provider(keys, values, &verifier).await;
        assert!(
            matches!(result, Err(SecretsError::InvalidProviderKeys)),
            "{why}: {result:?}"
        );
        assert!(
            matches!(
                store.read_provider(keys),
                Err(SecretsError::InvalidProviderKeys)
            ),
            "{why}: 읽기도 이름 규칙을 먼저 본다"
        );
    }
    assert!(verifier.calls().is_empty(), "확인기를 부르지 않는다");
    assert!(files_containing(dir.path(), VOYAGE_KEY).is_empty());
}

// ───────────────────────── 2. 줄 이름 ─────────────────────────

// 검증: D9 C3
#[test]
fn 줄_이름은_공급자와_칸을_대문자로_밑줄_둘로_잇는다() {
    assert_eq!(NAVER.env_name("client_id").as_deref(), Some(NAVER_ID_ENV));
    assert_eq!(
        NAVER.env_name("client_secret").as_deref(),
        Some(NAVER_SECRET_ENV)
    );
    assert_eq!(VOYAGE.env_name("api_key").as_deref(), Some(VOYAGE_ENV));
    assert_eq!(NAVER.env_name("api_key"), None, "선언에 없는 칸");
    assert_eq!(VOYAGE.env_name("client_id"), None, "선언에 없는 칸");
    assert_eq!(
        ProviderKeys::new("Bad__name", &["api_key"]).env_name("api_key"),
        None,
        "이름 규칙을 어긴 선언"
    );
}

// ───────────────────────── 3. 등록 성공 ─────────────────────────

// 검증: C3 C16 E4 D9
#[tokio::test]
async fn 네이버는_칸_둘로_등록되고_확인기가_선언과_값을_받는다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = ProviderVerifier::ok();

    let registered = store
        .register_provider(NAVER, naver(NAVER_ID, NAVER_SECRET), &verifier)
        .await
        .unwrap();

    assert_eq!(
        registered,
        ProviderRegistered {
            provider: "naver_news"
        }
    );
    let shown = format!("{registered:?}");
    assert!(!shown.contains(NAVER_ID) && !shown.contains(NAVER_SECRET));

    let calls = verifier.calls();
    assert_eq!(calls.len(), 1, "확인기는 한 번 불린다");
    assert_eq!(calls[0].0, NAVER);
    let mut seen = calls[0].1.clone();
    seen.sort();
    assert_eq!(
        seen,
        vec![
            ("client_id".to_owned(), NAVER_ID.to_owned()),
            ("client_secret".to_owned(), NAVER_SECRET.to_owned()),
        ]
    );

    assert_eq!(
        provider_value(&store, NAVER, "client_id").as_deref(),
        Some(NAVER_ID)
    );
    assert_eq!(
        provider_value(&store, NAVER, "client_secret").as_deref(),
        Some(NAVER_SECRET)
    );
    let env = read_env(dir.path());
    assert!(env.lines().any(|l| l.starts_with(NAVER_ID_ENV)));
    assert!(env.lines().any(|l| l.starts_with(NAVER_SECRET_ENV)));
}

// 검증: C3 C16 E4
#[tokio::test]
async fn 보이지는_칸_하나로_등록되고_상태에_값이_없다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = ProviderVerifier::ok();

    let registered = store
        .register_provider(VOYAGE, voyage(VOYAGE_KEY), &verifier)
        .await
        .unwrap();

    assert_eq!(registered.provider, "voyage");
    assert_eq!(verifier.calls().len(), 1);
    assert_eq!(verifier.calls()[0].0, VOYAGE);
    assert_eq!(
        provider_value(&store, VOYAGE, "api_key").as_deref(),
        Some(VOYAGE_KEY)
    );
    assert!(
        read_env(dir.path())
            .lines()
            .any(|l| l.starts_with(VOYAGE_ENV))
    );

    let status = store.provider_status(&[VOYAGE]).unwrap();
    assert_eq!(
        status,
        vec![ProviderStatus {
            provider: "voyage",
            registered: true
        }]
    );
    let shown = format!("{status:?}");
    assert!(!shown.contains(VOYAGE_KEY));
    assert_eq!(store.read_provider(NAVER).unwrap().map(|_| ()), None);
}

// ───────────────────────── 4. 칸 검사 ─────────────────────────

// 검증: C3 C16 E4 N8
#[tokio::test]
async fn 칸_검사에서_걸리면_확인기를_부르지_않고_저장하지_않는다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = ProviderVerifier::ok();

    let missing = store
        .register_provider(
            NAVER,
            Credentials::new().with("client_id", NAVER_ID),
            &verifier,
        )
        .await;
    assert!(
        matches!(
            missing,
            Err(SecretsError::MissingField {
                field: "client_secret"
            })
        ),
        "{missing:?}"
    );

    let unknown = store
        .register_provider(
            VOYAGE,
            voyage(VOYAGE_KEY).with("모르는_칸", "시험용-모르는-칸-값-0001"),
            &verifier,
        )
        .await;
    assert!(
        matches!(unknown, Err(SecretsError::UnknownField)),
        "{unknown:?}"
    );

    let short = store
        .register_provider(VOYAGE, voyage("짧은값-1"), &verifier)
        .await;
    assert!(
        matches!(
            short,
            Err(SecretsError::TooShort {
                field: "api_key",
                min: 8
            })
        ),
        "{short:?}"
    );

    let newline = store
        .register_provider(NAVER, naver(NAVER_ID, "시험용-줄\n바꿈-값-0001"), &verifier)
        .await;
    assert!(
        matches!(
            newline,
            Err(SecretsError::InvalidValue {
                field: "client_secret"
            })
        ),
        "{newline:?}"
    );

    for err in [&missing, &unknown, &short, &newline] {
        assert_no_value_in_error(
            err.as_ref().unwrap_err(),
            &[NAVER_ID, VOYAGE_KEY, "짧은값-1", "시험용-모르는-칸-값-0001"],
        );
    }
    assert!(verifier.calls().is_empty(), "확인기를 부르지 않는다");
    assert_eq!(store.read_provider(NAVER).unwrap().map(|_| ()), None);
    assert_eq!(store.read_provider(VOYAGE).unwrap().map(|_| ()), None);
    for value in [NAVER_ID, VOYAGE_KEY, "짧은값-1"] {
        assert!(files_containing(dir.path(), value).is_empty(), "저장 안 됨");
    }
}

// ───────────────────────── 5. 확인 실패 ─────────────────────────

// 검증: E4 C16 E38 N8
#[tokio::test]
async fn 확인에_실패하면_저장하지_않고_값은_계속_가린다() {
    for failure in [VerifyFailure::Rejected, VerifyFailure::Unreachable] {
        let dir = tempdir().unwrap();
        let store = open(dir.path());
        let verifier = ProviderVerifier::new(Err(failure));

        let result = store
            .register_provider(NAVER, naver(NAVER_ID, NAVER_SECRET), &verifier)
            .await;

        let err = result.unwrap_err();
        assert!(
            matches!(err, SecretsError::VerifyFailed(f) if f == failure),
            "{err:?}"
        );
        assert_no_value_in_error(&err, &[NAVER_ID, NAVER_SECRET]);
        assert_eq!(verifier.calls().len(), 1);
        assert_eq!(store.read_provider(NAVER).unwrap().map(|_| ()), None);
        assert_eq!(
            store.provider_status(&[NAVER]).unwrap(),
            vec![ProviderStatus {
                provider: "naver_news",
                registered: false
            }]
        );
        assert!(files_containing(dir.path(), NAVER_ID).is_empty());
        assert!(files_containing(dir.path(), NAVER_SECRET).is_empty());
        assert_hidden(&store, NAVER_ID, "확인 실패한 아이디");
        assert_hidden(&store, NAVER_SECRET, "확인 실패한 비밀");
    }
}

// 검증: E4 C16 N8
#[tokio::test]
async fn 공급자_확인을_모르는_확인기로는_저장되지_않는다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());

    let result = store
        .register_provider(VOYAGE, voyage(VOYAGE_KEY), &KindOnlyVerifier)
        .await;

    assert!(
        matches!(
            result,
            Err(SecretsError::VerifyFailed(VerifyFailure::Unreachable))
        ),
        "{result:?}"
    );
    assert_eq!(store.read_provider(VOYAGE).unwrap().map(|_| ()), None);
    assert!(files_containing(dir.path(), VOYAGE_KEY).is_empty());
    assert_hidden(&store, VOYAGE_KEY, "기본 확인기로 실패한 값");
}

// 검증: N8 E38
#[tokio::test]
async fn 확인기가_도는_동안에도_값은_가려진다() {
    let dir = tempdir().unwrap();
    let store = Arc::new(open(dir.path()));

    let attempts = [
        (Ok(()), NAVER_ID, NAVER_SECRET),
        (
            Err(VerifyFailure::Rejected),
            "시험용-네이버-아이디-0003",
            "시험용-네이버-비밀-값-0003",
        ),
    ];
    for (result, id, secret) in attempts {
        let verifier = ProviderVerifier::watching(&store, result);
        let _ = store
            .register_provider(NAVER, naver(id, secret), &verifier)
            .await;
        let probes = verifier.probes();
        assert_eq!(probes.len(), 2, "두 칸을 모두 들여다봤다");
        for (value, redacted, found) in probes {
            assert_eq!(redacted, format!("앞 {REDACTED} 뒤"), "값: 가려지지 않음");
            assert!(found, "확인 중인 값도 바이트 검사에 걸린다");
            assert!(!redacted.contains(&value));
        }
    }
}

// ───────────────────────── 6. 다시 연 보관소 ─────────────────────────

// 검증: D3 E38 N8 E39
#[tokio::test]
async fn 다시_연_보관소는_공급자_값을_읽고_가린다() {
    let dir = tempdir().unwrap();
    {
        let store = open(dir.path());
        let verifier = ProviderVerifier::ok();
        store
            .register_provider(NAVER, naver(NAVER_ID, NAVER_SECRET), &verifier)
            .await
            .unwrap();
        store
            .register_provider(VOYAGE, voyage(VOYAGE_KEY), &verifier)
            .await
            .unwrap();
    }

    let reopened = open(dir.path());
    assert_eq!(
        provider_value(&reopened, NAVER, "client_id").as_deref(),
        Some(NAVER_ID)
    );
    assert_eq!(
        provider_value(&reopened, NAVER, "client_secret").as_deref(),
        Some(NAVER_SECRET)
    );
    assert_eq!(
        provider_value(&reopened, VOYAGE, "api_key").as_deref(),
        Some(VOYAGE_KEY)
    );
    for value in [NAVER_ID, NAVER_SECRET, VOYAGE_KEY] {
        assert_hidden(&reopened, value, "다시 연 보관소");
    }
}

// 검증: E38 N8 E39
#[test]
fn 등록하지_않고_연_보관소도_키_줄_값을_선언을_몰라도_가린다() {
    let dir = tempdir().unwrap();
    write_env(
        dir.path(),
        &format!(
            "{NAVER_ID_ENV}={NAVER_ID}\n{NAVER_SECRET_ENV}={NAVER_SECRET}\n\
             PRICE_INHA_KEY__OTHER__TOKEN={OTHER_TOKEN}\n"
        ),
    );

    let store = open(dir.path());

    assert_eq!(
        provider_value(&store, NAVER, "client_secret").as_deref(),
        Some(NAVER_SECRET)
    );
    for value in [NAVER_ID, NAVER_SECRET, OTHER_TOKEN] {
        assert_hidden(&store, value, "손으로 적은 키 줄");
    }
}

// 검증: E38 N8
#[test]
fn 짧은_키_줄_값은_가리기_대상이_아니고_등록된_것도_아니다() {
    let dir = tempdir().unwrap();
    write_env(
        dir.path(),
        &format!("PRICE_INHA_KEY__OTHER__TOKEN=짧은-값\n{VOYAGE_ENV}=짧은-보이지\n"),
    );

    let store = open(dir.path());

    assert_not_hidden(&store, "짧은-값", "8자 미만 모르는 줄");
    assert_not_hidden(&store, "짧은-보이지", "8자 미만 보이지 줄");
    assert_eq!(store.read_provider(VOYAGE).unwrap().map(|_| ()), None);
}

// ───────────────────────── 7. 덜 적힌 줄 · 깨진 줄 ─────────────────────────

// 검증: D3 C3
#[test]
fn 칸_하나만_적혔거나_짧은_값이면_등록되지_않은_것이다() {
    let dir = tempdir().unwrap();
    write_env(
        dir.path(),
        &format!("{NAVER_ID_ENV}={NAVER_ID}\n{VOYAGE_ENV}=짧은값\n"),
    );
    let store = open(dir.path());

    assert_eq!(store.read_provider(NAVER).unwrap().map(|_| ()), None);
    assert_eq!(store.read_provider(VOYAGE).unwrap().map(|_| ()), None);
    assert_eq!(
        store.provider_status(&[NAVER, VOYAGE]).unwrap(),
        vec![
            ProviderStatus {
                provider: "naver_news",
                registered: false
            },
            ProviderStatus {
                provider: "voyage",
                registered: false
            },
        ]
    );
}

// 검증: D3 E38
#[test]
fn 깨진_줄은_그_공급자만_깨졌다고_알리고_다른_읽기에는_지장이_없다() {
    let dir = tempdir().unwrap();
    let text = format!(
        "{NAVER_ID_ENV}={NAVER_ID}\n{NAVER_SECRET_ENV}=\"닫히지 않음\n\
         {VOYAGE_ENV}={VOYAGE_KEY}\nPRICE_INHA_AI_API_KEY=시험용-에이아이-키-0001\n"
    );
    write_env(dir.path(), &text);
    let store = open(dir.path());

    let broken = store.read_provider(NAVER);
    assert!(
        matches!(broken, Err(SecretsError::Corrupt { file }) if file == ENV_FILE),
        "{broken:?}"
    );
    if let Err(err) = &broken {
        assert_no_value_in_error(err, &[NAVER_ID, "닫히지 않음"]);
    }
    assert_eq!(
        provider_value(&store, VOYAGE, "api_key").as_deref(),
        Some(VOYAGE_KEY)
    );
    let ai = store.read(KeyKind::AiApiKey).unwrap().unwrap();
    assert_eq!(
        ai.get("api_key").unwrap().expose(),
        "시험용-에이아이-키-0001"
    );
    assert_eq!(read_env(dir.path()), text, "깨진 파일을 고치지 않는다");
}

// ───────────────────────── 8. 옛 칸과 섞임 ─────────────────────────

// 검증: C3 D9 D3
#[tokio::test]
async fn 옛_등록과_공급자_등록은_서로의_줄을_지우지_않는다() {
    let dir = tempdir().unwrap();
    write_env(dir.path(), "# 손으로 적은 설명\nOTHER_NAME=keep\n");
    let store = open(dir.path());
    let verifier = ProviderVerifier::ok();

    store
        .register(
            KeyKind::Broker,
            Credentials::new()
                .with("client_id", BROKER_ID)
                .with("client_secret", BROKER_SECRET),
            &verifier,
        )
        .await
        .unwrap();
    store
        .register_slot(
            KeySlot::Dart,
            Credentials::new().with("api_key", DART),
            &verifier,
        )
        .await
        .unwrap();
    let old_status: Vec<KeyStatus> = store.status().unwrap();
    let old_slot_status: Vec<SlotStatus> = store.slot_status().unwrap();

    store
        .register_provider(NAVER, naver(NAVER_ID, NAVER_SECRET), &verifier)
        .await
        .unwrap();

    let env = read_env(dir.path());
    assert!(env.contains("# 손으로 적은 설명"));
    assert!(env.contains("OTHER_NAME=keep"));
    let broker = store.read(KeyKind::Broker).unwrap().unwrap();
    assert_eq!(broker.get("client_secret").unwrap().expose(), BROKER_SECRET);
    assert_eq!(
        store
            .read_slot(KeySlot::Dart)
            .unwrap()
            .unwrap()
            .get("api_key")
            .unwrap()
            .expose(),
        DART
    );
    assert_eq!(store.status().unwrap(), old_status, "옛 상태는 그대로");
    assert_eq!(store.slot_status().unwrap(), old_slot_status);
    assert_eq!(
        store.status().unwrap().len(),
        KeyKind::ALL.len(),
        "옛 상태에 공급자 칸이 나오지 않는다"
    );
    assert_eq!(store.slot_status().unwrap().len(), KeySlot::ALL.len());

    // 옛 길로 다시 등록해도 공급자 줄은 남는다.
    store
        .register(
            KeyKind::Broker,
            Credentials::new()
                .with("client_id", BROKER_ID)
                .with("client_secret", "시험용-증권사-비밀-값-0002"),
            &verifier,
        )
        .await
        .unwrap();
    store
        .register_slot(
            KeySlot::Dart,
            Credentials::new().with("api_key", "시험용-다트-키-값-0002"),
            &verifier,
        )
        .await
        .unwrap();
    assert_eq!(
        provider_value(&store, NAVER, "client_secret").as_deref(),
        Some(NAVER_SECRET)
    );
    assert!(read_env(dir.path()).contains("OTHER_NAME=keep"));
}

// 검증: C3 D3
#[tokio::test]
async fn 다시_등록하면_그_공급자_줄만_바뀐다() {
    let dir = tempdir().unwrap();
    write_env(
        dir.path(),
        "# 설명 줄\nOTHER_NAME=keep\nPRICE_INHA_KEY__OTHER__TOKEN=시험용-모르는-공급자-토큰-0001\n",
    );
    let store = open(dir.path());
    let verifier = ProviderVerifier::ok();
    store
        .register_provider(NAVER, naver(NAVER_ID, NAVER_SECRET), &verifier)
        .await
        .unwrap();
    store
        .register_provider(VOYAGE, voyage(VOYAGE_KEY), &verifier)
        .await
        .unwrap();
    let not_naver = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| !l.starts_with("PRICE_INHA_KEY__NAVER_NEWS__"))
            .map(str::to_owned)
            .collect()
    };
    let before = not_naver(&read_env(dir.path()));

    store
        .register_provider(
            NAVER,
            naver("시험용-네이버-아이디-0002", "시험용-네이버-비밀-값-0002"),
            &verifier,
        )
        .await
        .unwrap();

    let after = read_env(dir.path());
    assert_eq!(not_naver(&after), before, "다른 줄은 글자 그대로");
    assert_eq!(
        provider_value(&store, NAVER, "client_secret").as_deref(),
        Some("시험용-네이버-비밀-값-0002")
    );
    assert_eq!(
        after
            .lines()
            .filter(|l| l.starts_with(NAVER_SECRET_ENV))
            .count(),
        1,
        "같은 줄이 둘 생기지 않는다"
    );
    assert!(files_containing(dir.path(), NAVER_SECRET).is_empty());
    assert_eq!(
        provider_value(&store, VOYAGE, "api_key").as_deref(),
        Some(VOYAGE_KEY)
    );
}

// ───────────────────────── 9. 권한 ─────────────────────────

// 검증: D3 V46 E39
#[cfg(unix)]
#[tokio::test]
async fn 공급자_키도_0700_폴더의_0600_키_파일에만_있고_임시_파일이_남지_않는다() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("보관");
    let store = open(&dir);
    let verifier = ProviderVerifier::ok();

    store
        .register_provider(NAVER, naver(NAVER_ID, NAVER_SECRET), &verifier)
        .await
        .unwrap();
    store
        .register_provider(VOYAGE, voyage(VOYAGE_KEY), &verifier)
        .await
        .unwrap();

    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&env_path(&dir)), 0o600);
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec![ENV_FILE.to_owned()], "임시 파일이 남지 않는다");
    for value in [NAVER_ID, NAVER_SECRET, VOYAGE_KEY] {
        for file in files_containing(&dir, value) {
            assert!(
                BACKUP_EXCLUDED_FILES.contains(&file.as_str()),
                "값이 백업 대상 파일에 있다: {file}"
            );
        }
        assert!(!files_containing(&dir, value).is_empty());
    }
}

// 검증: D3 V46
#[cfg(unix)]
#[tokio::test]
async fn 넓은_권한의_키_파일은_공급자_등록_전에_0600으로_좁힌다() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().unwrap();
    write_env(dir.path(), "OTHER_NAME=keep\n");
    fs::set_permissions(env_path(dir.path()), fs::Permissions::from_mode(0o644)).unwrap();
    let store = open(dir.path());

    store
        .register_provider(VOYAGE, voyage(VOYAGE_KEY), &ProviderVerifier::ok())
        .await
        .unwrap();

    assert_eq!(mode(&env_path(dir.path())), 0o600);
}

// ───────────────────────── 10. 가리기 대상에 끝이 있다 ─────────────────────────

// 검증: N1 N8 E38
#[tokio::test]
async fn 같은_칸을_여러_번_다시_등록해도_가리기_대상이_끝없이_늘지_않는다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = ProviderVerifier::ok();
    let total = REDACT_PREVIOUS_KEPT + 5;
    let values: Vec<String> = (0..total)
        .map(|n| format!("시험용-보이지-돌린-값-{n:04}"))
        .collect();

    for value in &values {
        store
            .register_provider(VOYAGE, voyage(value), &verifier)
            .await
            .unwrap();
    }

    let current = total - 1;
    let oldest_kept = current - REDACT_PREVIOUS_KEPT;
    assert_hidden(&store, &values[current], "지금 값");
    for value in &values[oldest_kept..current] {
        assert_hidden(&store, value, "정한 개수 안의 앞 값");
    }
    for value in &values[..oldest_kept] {
        assert_not_hidden(&store, value, "정한 개수를 넘은 옛 값");
    }
    assert_eq!(
        provider_value(&store, VOYAGE, "api_key").as_deref(),
        Some(values[current].as_str())
    );
}

// 검증: N1 N8
#[tokio::test]
async fn 개수는_공급자_칸마다_따로_센다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = ProviderVerifier::ok();
    store
        .register_provider(NAVER, naver(NAVER_ID, NAVER_SECRET), &verifier)
        .await
        .unwrap();

    for n in 0..(REDACT_PREVIOUS_KEPT + 5) {
        store
            .register_provider(
                VOYAGE,
                voyage(&format!("시험용-보이지-돌린-값-{n:04}")),
                &verifier,
            )
            .await
            .unwrap();
    }

    assert_hidden(&store, NAVER_ID, "바뀌지 않은 다른 공급자 값");
    assert_hidden(&store, NAVER_SECRET, "바뀌지 않은 다른 공급자 값");
}

// ───────────────────────── 11. 상태 ─────────────────────────

// 검증: C3 D9
#[tokio::test]
async fn 공급자_상태는_넘긴_순서대로_등록_여부만_알려_준다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());

    assert_eq!(store.provider_status(&[]).unwrap(), Vec::new());
    assert_eq!(
        store.provider_status(&[VOYAGE, NAVER]).unwrap(),
        vec![
            ProviderStatus {
                provider: "voyage",
                registered: false
            },
            ProviderStatus {
                provider: "naver_news",
                registered: false
            },
        ]
    );

    store
        .register_provider(
            NAVER,
            naver(NAVER_ID, NAVER_SECRET),
            &ProviderVerifier::ok(),
        )
        .await
        .unwrap();

    let status = store.provider_status(&[VOYAGE, NAVER]).unwrap();
    assert_eq!(
        status,
        vec![
            ProviderStatus {
                provider: "voyage",
                registered: false
            },
            ProviderStatus {
                provider: "naver_news",
                registered: true
            },
        ]
    );
    assert_eq!(
        store.provider_status(&[NAVER, VOYAGE]).unwrap()[0],
        ProviderStatus {
            provider: "naver_news",
            registered: true
        }
    );
    let shown = format!("{status:?}");
    assert!(!shown.contains(NAVER_ID) && !shown.contains(NAVER_SECRET));
}
