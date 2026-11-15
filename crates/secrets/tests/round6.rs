//! pi-secrets 6차 작업 테스트. 기준: `docs/dev/modules/secrets.md`의 "6차 작업" 1~4번,
//! `docs/dev/decisions.md` 2026-11-16의 18 · 23 · 24번.
//!
//! - 자료 키는 출처(DART · ECOS · FRED)마다 따로 칸을 둔다(D9). 확인 규칙은 다른 키와 같다.
//! - 백업 GitHub fine-grained 토큰 칸 하나(D7). 등록 결과와 상태에 값이 없다.
//! - VAPID 비밀키는 처음 켤 때 만든 값을 0600으로 두고 백업 대상에서 뺀다.
//! - `contains_registered(&[u8])`: 바이트 안에 등록된 값이 그대로 · URL 인코딩 · Base64로 있는지 본다(E39 · N8).
//!
//! 보관 폴더는 모두 임시 폴더다. 비밀 값은 진짜 키처럼 보이지 않는 글자만 쓴다.

use std::cell::Cell;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use pi_secrets::{
    BACKUP_EXCLUDED_FILES, Credentials, ENV_FILE, KeyKind, KeySlot, LEGACY_AUTH_FILE, REDACTED,
    SecretStore, SecretString, SecretsError, SlotRegistered, SlotStatus, Verifier, VerifyFailure,
};
use tempfile::tempdir;

const DART: &str = "시험용-다트-키-값-0001";
const ECOS: &str = "시험용-이코스-키-값-0001";
const FRED: &str = "시험용-프레드-키-값-0001";
const BACKUP_TOKEN: &str = "시험용-백업-토큰-값-0001";
const VAPID: &str = "시험용-브이에이피드-비밀-값-0001";
const BROKER_ID: &str = "시험용-증권사-아이디-0001";
const BROKER_SECRET: &str = "시험용-증권사-비밀-값-0001";
const OLD_DATA: &str = "시험용-옛-자료-키-값-0001";

/// 확인기가 받은 칸과 값. 테스트가 확인기에 무엇이 넘어갔는지 본다.
type Call = (KeySlot, Vec<(String, String)>);

/// 6차 칸을 확인하는 가짜 확인기. 정한 결과를 돌려주고 받은 것을 적는다.
/// [`KeyKind`] 종류는 언제나 받아들인다(같은 보관소에 기존 종류도 등록하려고).
struct SlotVerifier {
    result: Result<(), VerifyFailure>,
    calls: Mutex<Vec<Call>>,
}

impl SlotVerifier {
    fn new(result: Result<(), VerifyFailure>) -> Self {
        Self {
            result,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

impl Verifier for SlotVerifier {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_slot(&self, slot: KeySlot, values: &Credentials) -> Result<(), VerifyFailure> {
        let seen = values
            .field_names()
            .into_iter()
            .map(|name| {
                let value = values.get(name).unwrap().expose().to_owned();
                (name.to_owned(), value)
            })
            .collect();
        self.calls.lock().unwrap().push((slot, seen));
        self.result
    }
}

/// 6차 전의 확인기: [`Verifier::verify`]만 구현한다(칸 확인은 기본 구현).
struct KindOnlyVerifier;

impl Verifier for KindOnlyVerifier {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

fn open(dir: &Path) -> SecretStore {
    SecretStore::open(dir).unwrap()
}

/// 칸이 정한 첫 칸 이름으로 값 하나를 담는다.
fn slot_values(slot: KeySlot, value: &str) -> Credentials {
    Credentials::new().with(slot.fields()[0], value)
}

fn slot_value(store: &SecretStore, slot: KeySlot) -> Option<String> {
    store
        .read_slot(slot)
        .unwrap()
        .map(|found| found.get(slot.fields()[0]).unwrap().expose().to_owned())
}

/// 각 칸의 시험 값.
fn value_of(slot: KeySlot) -> &'static str {
    match slot {
        KeySlot::Dart => DART,
        KeySlot::Ecos => ECOS,
        KeySlot::Fred => FRED,
        _ => BACKUP_TOKEN,
    }
}

/// 보관 폴더에서 `needle` 바이트가 든 파일 이름들(정렬).
fn files_containing(dir: &Path, needle: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
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

/// 앞뒤에 UTF-8이 아닌 바이트를 붙인다(SQLite 스냅샷 같은 바이트 안에 든 경우).
fn in_binary(inner: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x53, 0x51, 0x4c, 0x00, 0xff, 0xfe, 0x00];
    bytes.extend_from_slice(inner);
    bytes.extend_from_slice(&[0x00, 0xc3, 0x28, 0xff]);
    bytes
}

// 검증: D9 C3
#[tokio::test]
async fn 자료_키는_출처마다_따로_등록되고_따로_읽힌다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = SlotVerifier::new(Ok(()));

    assert_eq!(
        KeySlot::DATA_SOURCES,
        [KeySlot::Dart, KeySlot::Ecos, KeySlot::Fred]
    );
    for slot in KeySlot::DATA_SOURCES {
        let registered = store
            .register_slot(slot, slot_values(slot, value_of(slot)), &verifier)
            .await
            .unwrap();
        assert_eq!(registered, SlotRegistered { slot });
    }

    for slot in KeySlot::DATA_SOURCES {
        assert_eq!(slot_value(&store, slot).as_deref(), Some(value_of(slot)));
    }
    let expected: Vec<Call> = KeySlot::DATA_SOURCES
        .into_iter()
        .map(|slot| {
            (
                slot,
                vec![("api_key".to_owned(), value_of(slot).to_owned())],
            )
        })
        .collect();
    assert_eq!(verifier.calls(), expected, "확인기는 칸과 그 값을 받는다");
}

// 검증: D9
#[tokio::test]
async fn 한_출처만_등록하면_다른_칸과_기존_종류는_등록되지_않은_것이다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = SlotVerifier::new(Ok(()));

    store
        .register_slot(KeySlot::Dart, slot_values(KeySlot::Dart, DART), &verifier)
        .await
        .unwrap();

    let expected: Vec<SlotStatus> = KeySlot::ALL
        .into_iter()
        .map(|slot| SlotStatus {
            slot,
            registered: slot == KeySlot::Dart,
        })
        .collect();
    assert_eq!(store.slot_status().unwrap(), expected);
    assert!(store.read_slot(KeySlot::Ecos).unwrap().is_none());
    assert!(store.read_slot(KeySlot::Fred).unwrap().is_none());
    assert!(
        store
            .read_slot(KeySlot::BackupGithubToken)
            .unwrap()
            .is_none()
    );

    // 기존 종류의 상태는 그대로다: 출처 칸은 KeyKind 상태에 섞이지 않는다.
    let kinds = store.status().unwrap();
    assert_eq!(kinds.len(), KeyKind::ALL.len());
    assert!(kinds.iter().all(|status| !status.registered));
    assert!(store.read(KeyKind::Data).unwrap().is_none());
}

// 검증: D9 C16
#[tokio::test]
async fn 칸을_다시_등록하면_덮어쓰고_다른_칸과_종류는_그대로다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = SlotVerifier::new(Ok(()));
    let broker = Credentials::new()
        .with("client_id", BROKER_ID)
        .with("client_secret", BROKER_SECRET);
    store
        .register(KeyKind::Broker, broker, &verifier)
        .await
        .unwrap();
    let old_data = Credentials::new().with("api_key", OLD_DATA);
    store
        .register(KeyKind::Data, old_data, &verifier)
        .await
        .unwrap();
    store
        .register_slot(KeySlot::Dart, slot_values(KeySlot::Dart, DART), &verifier)
        .await
        .unwrap();
    store
        .register_slot(KeySlot::Fred, slot_values(KeySlot::Fred, FRED), &verifier)
        .await
        .unwrap();

    let newer = "시험용-다트-키-값-0002";
    store
        .register_slot(KeySlot::Dart, slot_values(KeySlot::Dart, newer), &verifier)
        .await
        .unwrap();

    for store in [store, open(dir.path())] {
        assert_eq!(slot_value(&store, KeySlot::Dart).as_deref(), Some(newer));
        assert_eq!(slot_value(&store, KeySlot::Fred).as_deref(), Some(FRED));
        let broker = store.read(KeyKind::Broker).unwrap().unwrap();
        assert_eq!(broker.get("client_id").unwrap().expose(), BROKER_ID);
        assert_eq!(broker.get("client_secret").unwrap().expose(), BROKER_SECRET);
        let data = store.read(KeyKind::Data).unwrap().unwrap();
        assert_eq!(data.get("api_key").unwrap().expose(), OLD_DATA);
    }
    assert!(
        files_containing(dir.path(), DART).is_empty(),
        "덮인 값은 파일에 남지 않는다"
    );
}

// 검증: D9 D7 E4 N8
#[tokio::test]
async fn 확인에_실패하면_어느_칸도_저장하지_않고_값은_계속_가린다() {
    for failure in [VerifyFailure::Rejected, VerifyFailure::Unreachable] {
        let dir = tempdir().unwrap();
        let store = open(dir.path());
        let verifier = SlotVerifier::new(Err(failure));

        for slot in KeySlot::ALL {
            let value = value_of(slot);
            let result = store
                .register_slot(slot, slot_values(slot, value), &verifier)
                .await;
            let err = result.unwrap_err();
            assert!(matches!(err, SecretsError::VerifyFailed(f) if f == failure));
            assert!(!format!("{err}").contains(value));
            assert!(!format!("{err:?}").contains(value));

            assert!(store.read_slot(slot).unwrap().is_none());
            assert!(files_containing(dir.path(), value).is_empty());
            let redacted = store.redact(&format!("값={value}"));
            assert!(!redacted.contains(value) && redacted.contains(REDACTED));
            assert!(store.contains_registered(value.as_bytes()));
        }
        assert!(store.slot_status().unwrap().iter().all(|s| !s.registered));
    }
}

// 검증: D9 E4
#[tokio::test]
async fn 칸_확인을_모르는_확인기로는_저장되지_않는다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());

    let result = store
        .register_slot(
            KeySlot::Ecos,
            slot_values(KeySlot::Ecos, ECOS),
            &KindOnlyVerifier,
        )
        .await;

    assert!(matches!(
        result,
        Err(SecretsError::VerifyFailed(VerifyFailure::Unreachable))
    ));
    assert!(store.read_slot(KeySlot::Ecos).unwrap().is_none());
    assert!(files_containing(dir.path(), ECOS).is_empty());
}

// 검증: D9 D7 C3
#[tokio::test]
async fn 칸_검사에서_걸리면_확인기를_부르지_않고_저장하지_않는다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = SlotVerifier::new(Ok(()));

    let short = store
        .register_slot(
            KeySlot::Dart,
            slot_values(KeySlot::Dart, "짧은값"),
            &verifier,
        )
        .await;
    assert!(matches!(
        short,
        Err(SecretsError::TooShort {
            field: "api_key",
            min: 8
        })
    ));

    let newline = store
        .register_slot(
            KeySlot::Fred,
            slot_values(KeySlot::Fred, "시험용-줄\n바꿈-값-0001"),
            &verifier,
        )
        .await;
    assert!(matches!(
        newline,
        Err(SecretsError::InvalidValue { field: "api_key" })
    ));

    let missing = store
        .register_slot(KeySlot::BackupGithubToken, Credentials::new(), &verifier)
        .await;
    assert!(matches!(
        missing,
        Err(SecretsError::MissingField { field: "token" })
    ));

    let wrong_field = store
        .register_slot(
            KeySlot::BackupGithubToken,
            Credentials::new().with("api_key", BACKUP_TOKEN),
            &verifier,
        )
        .await;
    assert!(matches!(wrong_field, Err(SecretsError::UnknownField)));

    let extra_field = store
        .register_slot(
            KeySlot::Ecos,
            slot_values(KeySlot::Ecos, ECOS).with("token", BACKUP_TOKEN),
            &verifier,
        )
        .await;
    assert!(matches!(extra_field, Err(SecretsError::UnknownField)));

    assert!(
        verifier.calls().is_empty(),
        "칸 검사에서 걸리면 확인기를 부르지 않는다"
    );
    assert!(store.slot_status().unwrap().iter().all(|s| !s.registered));
}

// 검증: D9 D7 N8
#[tokio::test]
async fn 다시_연_보관소도_칸_값을_읽고_가린다() {
    let dir = tempdir().unwrap();
    let verifier = SlotVerifier::new(Ok(()));
    {
        let store = open(dir.path());
        for slot in KeySlot::ALL {
            store
                .register_slot(slot, slot_values(slot, value_of(slot)), &verifier)
                .await
                .unwrap();
        }
    }

    let reopened = open(dir.path());
    for slot in KeySlot::ALL {
        let value = value_of(slot);
        assert_eq!(slot_value(&reopened, slot).as_deref(), Some(value));
        let redacted = reopened.redact(&format!("요청 주소 ?key={value}"));
        assert!(!redacted.contains(value) && redacted.contains(REDACTED));
        assert!(reopened.contains_registered(value.as_bytes()));
    }
    assert!(reopened.slot_status().unwrap().iter().all(|s| s.registered));
}

// 검증: D9 D7 N8
#[cfg(unix)]
#[tokio::test]
async fn 칸을_등록해도_키_파일은_0600이고_백업_제외_파일에만_있다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = SlotVerifier::new(Ok(()));
    for slot in KeySlot::ALL {
        store
            .register_slot(slot, slot_values(slot, value_of(slot)), &verifier)
            .await
            .unwrap();
    }

    assert_eq!(mode(dir.path()), 0o700);
    for slot in KeySlot::ALL {
        let files = files_containing(dir.path(), value_of(slot));
        assert!(!files.is_empty(), "등록한 값은 키 파일에 있다");
        for name in files {
            assert!(BACKUP_EXCLUDED_FILES.contains(&name.as_str()), "{name}");
            assert_eq!(mode(&dir.path().join(&name)), 0o600, "{name}");
        }
    }
    for entry in fs::read_dir(dir.path()).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            BACKUP_EXCLUDED_FILES.contains(&name.as_str()),
            "임시 파일이나 다른 파일이 남지 않는다: {name}"
        );
    }
}

// 검증: D7 N8
#[tokio::test]
async fn 백업_토큰은_칸_하나이고_등록_결과와_상태에_값이_없다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    let verifier = SlotVerifier::new(Ok(()));
    assert_eq!(KeySlot::BackupGithubToken.fields(), ["token"]);

    let registered = store
        .register_slot(
            KeySlot::BackupGithubToken,
            slot_values(KeySlot::BackupGithubToken, BACKUP_TOKEN),
            &verifier,
        )
        .await
        .unwrap();

    assert_eq!(
        registered,
        SlotRegistered {
            slot: KeySlot::BackupGithubToken
        }
    );
    assert!(!format!("{registered:?}").contains(BACKUP_TOKEN));
    let status = store.slot_status().unwrap();
    assert!(!format!("{status:?}").contains(BACKUP_TOKEN));
    assert!(!format!("{store:?}").contains(BACKUP_TOKEN));
    let read = store
        .read_slot(KeySlot::BackupGithubToken)
        .unwrap()
        .unwrap();
    assert!(!format!("{read:?}").contains(BACKUP_TOKEN));
    assert_eq!(read.get("token").unwrap().expose(), BACKUP_TOKEN);
}

// 검증: E39 N8
#[test]
fn vapid_비밀키는_처음_한_번만_만들고_다시_열어도_같다() {
    let dir = tempdir().unwrap();
    let made = Cell::new(0);
    let generate = || {
        made.set(made.get() + 1);
        SecretString::new(VAPID)
    };

    let store = open(dir.path());
    assert_eq!(store.vapid_private_key(generate).unwrap().expose(), VAPID);
    assert_eq!(made.get(), 1);
    assert_eq!(store.vapid_private_key(generate).unwrap().expose(), VAPID);
    assert_eq!(made.get(), 1, "이미 있으면 다시 만들지 않는다");
    drop(store);

    let reopened = open(dir.path());
    let other = || {
        made.set(made.get() + 1);
        SecretString::new("시험용-다른-브이에이피드-값-0002")
    };
    assert_eq!(reopened.vapid_private_key(other).unwrap().expose(), VAPID);
    assert_eq!(made.get(), 1, "다시 연 보관소도 처음 만든 값을 쓴다");
}

// 검증: E39 N8
#[cfg(unix)]
#[test]
fn vapid_비밀키는_0600_파일에_두고_백업_대상에서_빠진다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());
    store
        .vapid_private_key(|| SecretString::new(VAPID))
        .unwrap();

    assert!(BACKUP_EXCLUDED_FILES.contains(&ENV_FILE));
    assert!(BACKUP_EXCLUDED_FILES.contains(&LEGACY_AUTH_FILE));
    let files = files_containing(dir.path(), VAPID);
    assert!(!files.is_empty(), "만든 값을 파일에 둔다");
    for name in files {
        assert!(BACKUP_EXCLUDED_FILES.contains(&name.as_str()), "{name}");
        assert_eq!(mode(&dir.path().join(&name)), 0o600, "{name}");
    }
    assert_eq!(mode(dir.path()), 0o700);
}

// 검증: E39 N8
#[test]
fn vapid_비밀키는_가려지고_다시_연_보관소도_가린다() {
    let dir = tempdir().unwrap();
    {
        let store = open(dir.path());
        let key = store
            .vapid_private_key(|| SecretString::new(VAPID))
            .unwrap();
        assert!(!format!("{key:?}").contains(VAPID));
        assert!(!store.redact(&format!("키={VAPID}")).contains(VAPID));
        assert!(store.contains_registered(&in_binary(VAPID.as_bytes())));
    }

    // 다시 연 보관소는 VAPID를 부르지 않아도 파일에 있는 값을 가린다(백업기가 따로 연다).
    let reopened = open(dir.path());
    let redacted = reopened.redact(&format!("키={VAPID}"));
    assert!(!redacted.contains(VAPID) && redacted.contains(REDACTED));
    assert!(reopened.contains_registered(&in_binary(VAPID.as_bytes())));
}

// 검증: E39
#[test]
fn 만든_vapid_값이_짧거나_못_쓰는_글자면_저장하지_않는다() {
    let dir = tempdir().unwrap();
    let store = open(dir.path());

    let short = store.vapid_private_key(|| SecretString::new("짧은값"));
    assert!(matches!(short, Err(SecretsError::TooShort { min: 8, .. })));

    let bad = "시험용-줄\n바꿈-브이에이피드-0001";
    let newline = store.vapid_private_key(|| SecretString::new(bad));
    let err = newline.unwrap_err();
    assert!(matches!(err, SecretsError::InvalidValue { .. }));
    assert!(!format!("{err}").contains(bad) && !format!("{err:?}").contains(bad));
    assert!(files_containing(dir.path(), "브이에이피드").is_empty());

    let made = Cell::new(0);
    let key = store
        .vapid_private_key(|| {
            made.set(made.get() + 1);
            SecretString::new(VAPID)
        })
        .unwrap();
    assert_eq!(made.get(), 1, "거부된 뒤에는 다시 만든다");
    assert_eq!(key.expose(), VAPID);
}

/// 보관소 하나에 기존 종류 · 6차 칸 · VAPID를 모두 등록한다.
async fn store_with_everything(dir: &Path) -> SecretStore {
    let store = open(dir);
    let verifier = SlotVerifier::new(Ok(()));
    let broker = Credentials::new()
        .with("client_id", BROKER_ID)
        .with("client_secret", BROKER_SECRET);
    store
        .register(KeyKind::Broker, broker, &verifier)
        .await
        .unwrap();
    store
        .register_slot(KeySlot::Dart, slot_values(KeySlot::Dart, DART), &verifier)
        .await
        .unwrap();
    store
        .register_slot(
            KeySlot::BackupGithubToken,
            slot_values(KeySlot::BackupGithubToken, BACKUP_TOKEN),
            &verifier,
        )
        .await
        .unwrap();
    store
        .vapid_private_key(|| SecretString::new(VAPID))
        .unwrap();
    store
}

// 검증: E39 N8
#[tokio::test]
async fn 바이트에_그대로_든_등록된_값을_찾는다() {
    let dir = tempdir().unwrap();
    let store = store_with_everything(dir.path()).await;

    for value in [BROKER_ID, BROKER_SECRET, DART, BACKUP_TOKEN, VAPID] {
        assert!(store.contains_registered(value.as_bytes()), "{value}");
        assert!(
            store.contains_registered(&in_binary(value.as_bytes())),
            "{value}"
        );
    }
}

// 검증: E39 N8
#[tokio::test]
async fn url_인코딩이나_base64로_든_값도_찾는다() {
    let dir = tempdir().unwrap();
    let store = store_with_everything(dir.path()).await;

    for value in [BROKER_SECRET, DART, BACKUP_TOKEN, VAPID] {
        let encoded = utf8_percent_encode(value, NON_ALPHANUMERIC).to_string();
        let shapes = [
            encoded.clone(),
            encoded.to_lowercase(),
            STANDARD.encode(value),
            URL_SAFE_NO_PAD.encode(value),
            STANDARD.encode(format!("{BROKER_ID}:{value}")),
            URL_SAFE_NO_PAD.encode(format!("x:{value}")),
        ];
        for shape in shapes {
            assert!(
                store.contains_registered(&in_binary(shape.as_bytes())),
                "{value} 모양 {shape}"
            );
        }
    }
}

// 검증: E39
#[tokio::test]
async fn 등록된_값이_없는_바이트는_찾지_않는다() {
    let dir = tempdir().unwrap();
    let store = store_with_everything(dir.path()).await;
    let cut = &DART[..DART.len() - 1];

    let clean: [Vec<u8>; 6] = [
        Vec::new(),
        in_binary(b""),
        in_binary("시험용-등록-안-한-값-0001".as_bytes()),
        in_binary(REDACTED.as_bytes()),
        in_binary(cut.as_bytes()),
        in_binary(STANDARD.encode("시험용-등록-안-한-값-0001").as_bytes()),
    ];
    for bytes in clean {
        assert!(!store.contains_registered(&bytes), "{bytes:?}");
    }

    let empty_dir = tempdir().unwrap();
    let empty = open(empty_dir.path());
    assert!(!empty.contains_registered(&in_binary(DART.as_bytes())));
}

// 검증: E39 N8
#[tokio::test]
async fn 다시_연_보관소는_파일에_있던_값도_찾는다() {
    let dir = tempdir().unwrap();
    drop(store_with_everything(dir.path()).await);

    let reopened = open(dir.path());
    for value in [BROKER_SECRET, DART, BACKUP_TOKEN, VAPID] {
        assert!(
            reopened.contains_registered(&in_binary(value.as_bytes())),
            "{value}"
        );
        let b64 = STANDARD.encode(value);
        assert!(
            reopened.contains_registered(&in_binary(b64.as_bytes())),
            "{value}"
        );
    }
}
