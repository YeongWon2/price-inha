//! 올리기: 스냅샷 → manifest → 암호화 → contents API → 크기 확인 → 임시 지우기 → 상태 기록.

use crate::common::*;
use pi_backup::{
    Backup, BackupError, MANIFEST_VERSION, REMOTE_DIR, SNAPSHOT_FILE, backup_file_name,
};
use pi_store::{ApprovalStatus, Store, UnixMillis};

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);

fn remote_path(at: UnixMillis) -> String {
    format!("{REMOTE_DIR}/{}", backup_file_name(at))
}

// 검증: D7, C13, V48
#[tokio::test]
async fn 올리면_원격_백업_폴더에_암호문_하나가_생기고_크기가_맞는다() {
    let env = Env::new().await;

    let report = env.backup().run(NOW).await.unwrap();

    assert_eq!(env.fake.paths(), vec![remote_path(NOW)]);
    assert_eq!(report.remote_path, remote_path(NOW));
    assert_eq!(report.at, NOW);
    let stored = env.fake.file(&remote_path(NOW)).unwrap();
    assert_eq!(report.size, stored.len() as u64);
    // age 형식의 머리말로 시작하고 평문 SQLite가 아니다.
    assert!(stored.starts_with(b"age-encryption.org/v1\n"));
    assert!(!contains(&stored, b"SQLite format 3"));
}

// 검증: D7, C13
#[tokio::test]
async fn 올린_암호문을_identity로_풀면_manifest와_스냅샷이_맞다() {
    let env = Env::new().await;

    let report = env.backup().run(NOW).await.unwrap();

    let stored = env.fake.file(&report.remote_path).unwrap();
    let bundle = pi_backup::open(&stored, &env.identity_secret()).unwrap();
    assert_eq!(bundle.manifest.version, MANIFEST_VERSION);
    assert_eq!(bundle.manifest.created_at, NOW);
    assert_eq!(bundle.manifest, report.manifest);
    assert_eq!(bundle.files.len(), 1);
    let snap = &bundle.files[0];
    assert_eq!(snap.path, SNAPSHOT_FILE);
    let listed = &bundle.manifest.files[0];
    assert_eq!(listed.path, SNAPSHOT_FILE);
    assert_eq!(listed.size, snap.bytes.len() as u64);
    assert_eq!(listed.sha256, sha_hex(&snap.bytes));
    assert_eq!(pi_backup::sha256_hex(&snap.bytes), sha_hex(&snap.bytes));
    pi_backup::verify(&bundle).unwrap();
}

// 검증: D7, C13
#[tokio::test]
async fn 스냅샷을_열면_백업_전에_쓴_자료가_들어_있다() {
    let env = Env::new().await;
    let id = pending_approval(&env.store, "백업 전에 쓴 승인", UnixMillis(NOW.0 - 1000));

    let report = env.backup().run(NOW).await.unwrap();

    let stored = env.fake.file(&report.remote_path).unwrap();
    let bundle = pi_backup::open(&stored, &env.identity_secret()).unwrap();
    let copy = env.target("copy.sqlite");
    std::fs::write(&copy, &bundle.files[0].bytes).unwrap();
    let reopened = Store::open(&copy).unwrap();
    let pending = reopened
        .approvals_in_status(ApprovalStatus::Pending)
        .unwrap();
    assert_eq!(pending.iter().map(|a| a.id).collect::<Vec<_>>(), vec![id]);
}

// 검증: D7
#[tokio::test]
async fn 다른_identity로는_풀리지_않는다() {
    let env = Env::new().await;
    let report = env.backup().run(NOW).await.unwrap();
    let stored = env.fake.file(&report.remote_path).unwrap();
    let other = age::x25519::Identity::generate();
    let other = pi_secrets::SecretString::new(age::secrecy::ExposeSecret::expose_secret(
        &other.to_string(),
    ));

    let err = pi_backup::open(&stored, &other).unwrap_err();

    assert!(matches!(err, BackupError::Decrypt), "{err:?}");
}

// 검증: C13, N8
#[tokio::test]
async fn 요청마다_토큰이_실리고_끝나면_임시_폴더가_비어_있다() {
    let env = Env::new().await;

    env.backup().run(NOW).await.unwrap();

    let requests = env.fake.requests().await;
    assert!(!requests.is_empty());
    for request in &requests {
        let auth = request
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(auth.ends_with(TOKEN), "토큰이 실려야 한다");
    }
    assert_eq!(names_in(&env.settings.temp_root), Vec::<String>::new());
}

// 검증: C13
#[tokio::test]
async fn 로컬_사본도_같은_이름_같은_내용으로_남는다() {
    let env = Env::new().await;

    let report = env.backup().run(NOW).await.unwrap();

    let local = std::fs::read(env.settings.local_dir.join(backup_file_name(NOW))).unwrap();
    assert_eq!(Some(local), env.fake.file(&report.remote_path));
}

// 검증: C13, E15
#[tokio::test]
async fn 마지막_성공_시각이_기록되고_다시_열어도_같다() {
    let env = Env::new().await;
    let backup = env.backup();
    assert_eq!(backup.status().unwrap().last_success, None);

    backup.run(NOW).await.unwrap();

    let status = backup.status().unwrap();
    assert_eq!(status.last_success, Some(NOW));
    assert!(!status.retry_pending);
    assert_eq!(status.last_failure, None);
    drop(backup);
    assert_eq!(env.backup().status().unwrap(), status);
}

// 검증: C13
#[tokio::test]
async fn 저장소_이름이_소유자_이름_모양이_아니면_만들지_않는다() {
    let env = Env::new().await;
    for repo in ["repo", "/repo", "owner/", "owner/repo/extra", ""] {
        let mut settings = env.settings.clone();
        settings.repo = repo.to_owned();

        let made = Backup::new(settings, env.store.clone(), env.secrets.clone());

        assert!(matches!(made, Err(BackupError::InvalidRepo)), "{repo:?}");
    }
    assert!(env.fake.requests().await.is_empty());
}
