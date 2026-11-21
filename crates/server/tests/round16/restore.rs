//! 터미널의 되살리기: 빈 폴더가 아니면 거부, 되살린 폴더는 팀 멈춤 · 승인 만료(V51 · M5),
//! 비밀키 파일 글자는 어디에도 남지 않는다.

use std::path::Path;

use pi_backup::{Bundle, BundleFile, MANIFEST_VERSION, Manifest, ManifestFile, SNAPSHOT_FILE};
use pi_store::{ApprovalId, ApprovalStatus, NewApprovalRequest, Store, TeamState, UnixMillis};
use price_inha::{CliError, DB_FILE, RestoreRequest, restore};

use crate::common::files_containing;

/// 시험 안에서 만든 age 키 한 쌍. 비밀키는 `age-keygen` 출력 모양의 파일로 둔다.
struct AgeKeys {
    recipient: String,
    secret: String,
}

impl AgeKeys {
    fn generate() -> AgeKeys {
        let identity = age::x25519::Identity::generate();
        let recipient = identity.to_public().to_string();
        let secret = identity.to_string();
        let secret = age::secrecy::ExposeSecret::expose_secret(&secret).to_string();
        AgeKeys { recipient, secret }
    }

    /// `dir/identity.txt`에 주석 두 줄과 비밀키 한 줄을 쓴다.
    fn write_identity(&self, dir: &Path) -> std::path::PathBuf {
        let path = dir.join("identity.txt");
        let text = format!(
            "# created: 2026-11-21T09:00:00+09:00\n# public key: {}\n{}\n",
            self.recipient, self.secret
        );
        std::fs::write(&path, text).unwrap();
        path
    }
}

/// 기다리는 승인 하나가 든 저장소를 묶어 `recipient`로 암호화한다.
fn backup_with_pending(work: &Path, recipient: &str) -> (Vec<u8>, ApprovalId) {
    let store = Store::open(work.join("source.sqlite")).unwrap();
    let pending = store
        .create_approval(NewApprovalRequest {
            fingerprint: "restore-cli-fingerprint".to_string(),
            summary: serde_json::json!({"symbol": "005930", "side": "buy"}),
            created_at: UnixMillis(1_000),
            expires_at: UnixMillis(4_000_000_000_000),
        })
        .unwrap();
    let snapshot = work.join("snapshot.sqlite");
    store.snapshot_to(&snapshot).unwrap();
    drop(store);
    let bytes = std::fs::read(&snapshot).unwrap();
    let bundle = Bundle {
        manifest: Manifest {
            version: MANIFEST_VERSION,
            created_at: UnixMillis(2_000),
            files: vec![ManifestFile {
                path: SNAPSHOT_FILE.to_string(),
                size: bytes.len() as u64,
                sha256: pi_backup::sha256_hex(&bytes),
            }],
        },
        files: vec![BundleFile {
            path: SNAPSHOT_FILE.to_string(),
            bytes,
        }],
    };
    (pi_backup::seal(&bundle, recipient).unwrap(), pending.id)
}

/// 백업 파일과 비밀키 파일을 `keys` 폴더에 둔다.
fn prepared(keys: &Path) -> (AgeKeys, std::path::PathBuf, std::path::PathBuf, ApprovalId) {
    let age = AgeKeys::generate();
    let work = tempfile::tempdir().unwrap();
    let (encrypted, approval) = backup_with_pending(work.path(), &age.recipient);
    let backup = keys.join("price-inha.age");
    std::fs::write(&backup, encrypted).unwrap();
    let identity = age.write_identity(keys);
    (age, backup, identity, approval)
}

// 검증: V51
#[test]
fn 빈_폴더가_아니면_거부하고_그대로_둔다() {
    let keys = tempfile::tempdir().unwrap();
    let (_age, backup, identity, _) = prepared(keys.path());
    let into = tempfile::tempdir().unwrap();
    std::fs::write(into.path().join("existing.txt"), b"keep me").unwrap();

    let refused = restore(
        &RestoreRequest {
            backup,
            identity,
            into: into.path().to_path_buf(),
        },
        UnixMillis(3_000),
    );
    assert!(
        matches!(refused, Err(CliError::NotEmpty { .. })),
        "{refused:?}"
    );
    let names: Vec<String> = std::fs::read_dir(into.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["existing.txt".to_string()],
        "아무것도 바꾸지 않는다"
    );
    assert_eq!(
        std::fs::read(into.path().join("existing.txt")).unwrap(),
        b"keep me"
    );
}

// 검증: V51 C8 E9
#[test]
fn 되살린_폴더는_팀이_멈추고_승인은_만료다() {
    let keys = tempfile::tempdir().unwrap();
    let (_age, backup, identity, approval) = prepared(keys.path());
    let parent = tempfile::tempdir().unwrap();
    let into = parent.path().join("home");
    std::fs::create_dir(&into).unwrap();

    let summary = restore(
        &RestoreRequest {
            backup,
            identity,
            into: into.clone(),
        },
        UnixMillis(3_000),
    )
    .expect("빈 폴더에 되살린다");
    assert!(
        !summary.report.outcome.expired_approvals.is_empty(),
        "기다리던 승인을 만료시켰다"
    );
    let store = Store::open(into.join(DB_FILE)).unwrap();
    assert_eq!(
        store.team_state().unwrap(),
        TeamState::FullStop,
        "되살리면 팀은 멈춘다"
    );
    assert_eq!(
        store.get_approval(approval).unwrap().status,
        ApprovalStatus::Expired,
        "그 승인은 만료다(M5)"
    );
    assert!(summary.text.contains("멈춤"), "{}", summary.text);
    assert!(summary.text.contains("만료"), "{}", summary.text);
    assert!(
        summary.text.contains("run --home") && summary.text.contains(into.to_str().unwrap()),
        "다음에 띄울 명령: {}",
        summary.text
    );
}

// 검증: N8 V51
#[test]
fn 비밀키_파일_글자는_어디에도_남지_않는다() {
    let keys = tempfile::tempdir().unwrap();
    let (age, backup, identity, _) = prepared(keys.path());
    let parent = tempfile::tempdir().unwrap();
    let into = parent.path().join("home");

    let summary = restore(
        &RestoreRequest {
            backup,
            identity,
            into: into.clone(),
        },
        UnixMillis(3_000),
    )
    .expect("없는 폴더에도 되살린다");
    assert!(!summary.text.contains(&age.secret), "안내에 비밀키가 없다");
    assert!(
        !format!("{summary:?}").contains(&age.secret),
        "Debug에 비밀키가 없다"
    );
    assert!(
        files_containing(parent.path(), &age.secret).is_empty(),
        "되살린 폴더 · 기록에 비밀키 글자가 없다"
    );
}

// 검증: V51
#[test]
fn 비밀키_파일이_틀리면_되살리지_않는다() {
    let keys = tempfile::tempdir().unwrap();
    let (_age, backup, _identity, _) = prepared(keys.path());
    let wrong = keys.path().join("wrong.txt");
    std::fs::write(&wrong, "# no key in this file\n").unwrap();
    let parent = tempfile::tempdir().unwrap();
    let into = parent.path().join("home");
    let refused = restore(
        &RestoreRequest {
            backup,
            identity: wrong,
            into: into.clone(),
        },
        UnixMillis(3_000),
    );
    assert!(refused.is_err(), "비밀키가 없으면 거부");
    assert!(!into.join(DB_FILE).exists(), "되살린 것이 없다");
}
