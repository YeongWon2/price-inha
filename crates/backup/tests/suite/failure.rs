//! 실패: 기록하고, 다음 마감에 다시 하고, 알림 요구를 낸다(E15). 실패하면 오래된 것을 지우지 않는다.

use crate::common::*;
use pi_backup::{BackupError, FailureKind, LastFailure, REMOTE_DIR, backup_file_name};
use pi_store::UnixMillis;

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);
const NEXT: UnixMillis = UnixMillis(NOW.0 + 86_400_000);

fn assert_failed_state(env: &Env, kind: FailureKind) {
    let status = env.backup().status().unwrap();
    assert!(status.retry_pending, "다음 마감에 다시 올려야 한다");
    assert_eq!(status.last_failure, Some(LastFailure { at: NOW, kind }));
    assert_eq!(status.last_success, None);
    assert_eq!(names_in(&env.settings.temp_root), Vec::<String>::new());
}

// 검증: E15, C13
#[tokio::test]
async fn 토큰이_없으면_요청_없이_실패하고_알림을_낸다() {
    let env = Env::without_token().await;

    let failure = env.backup().run(NOW).await.unwrap_err();

    assert!(
        matches!(failure.error, BackupError::TokenMissing),
        "{:?}",
        failure.error
    );
    assert_eq!(failure.notice.kind, FailureKind::TokenMissing);
    assert_eq!(failure.notice.at, NOW);
    assert!(env.fake.requests().await.is_empty());
    assert_failed_state(&env, FailureKind::TokenMissing);
}

// 검증: E15, C13
#[tokio::test]
async fn 원격이_실패하면_기록하고_다음_마감에_다시_올린다() {
    let env = Env::new().await;
    env.fake.fail_puts(500);

    let failure = env.backup().run(NOW).await.unwrap_err();

    assert!(
        matches!(failure.error, BackupError::Remote { status: Some(500) }),
        "{:?}",
        failure.error
    );
    assert_eq!(failure.error.kind(), FailureKind::Upload);
    assert_eq!(failure.notice.kind, FailureKind::Upload);
    assert_failed_state(&env, FailureKind::Upload);

    env.fake.heal();
    let report = env.backup().run(NEXT).await.unwrap();

    assert_eq!(
        env.fake.paths(),
        vec![format!("{REMOTE_DIR}/{}", backup_file_name(NEXT))]
    );
    assert_eq!(report.at, NEXT);
    let status = env.backup().status().unwrap();
    assert!(!status.retry_pending);
    assert_eq!(status.last_success, Some(NEXT));
}

// 검증: E15, C13
#[tokio::test]
async fn 원격이_알려_준_크기가_다르면_실패다() {
    let env = Env::new().await;
    env.fake.lie_about_size();

    let failure = env.backup().run(NOW).await.unwrap_err();

    assert!(
        matches!(failure.error, BackupError::SizeMismatch { .. }),
        "{:?}",
        failure.error
    );
    assert_eq!(failure.notice.kind, FailureKind::SizeMismatch);
    assert_failed_state(&env, FailureKind::SizeMismatch);
}

// 검증: E15, D7
#[tokio::test]
async fn 수신자_공개키가_잘못되면_암호화_실패로_기록하고_올리지_않는다() {
    let mut env = Env::new().await;
    env.settings.recipient = "not-an-age-recipient".to_owned();

    let failure = env.backup().run(NOW).await.unwrap_err();

    assert!(
        matches!(failure.error, BackupError::Encrypt),
        "{:?}",
        failure.error
    );
    assert_eq!(failure.notice.kind, FailureKind::Encrypt);
    assert_eq!(env.fake.count("PUT").await, 0);
    assert_failed_state(&env, FailureKind::Encrypt);
}

// 검증: E15
#[tokio::test]
async fn 실패하면_오래된_백업을_로컬에서도_원격에서도_지우지_않는다() {
    let env = Env::new().await;
    std::fs::create_dir_all(&env.settings.local_dir).unwrap();
    let mut seeded = Vec::new();
    for back in 1..=40 {
        let name = backup_file_name(plus_days(NOW, -back));
        env.fake.put_file(&format!("{REMOTE_DIR}/{name}"), b"old");
        std::fs::write(env.settings.local_dir.join(&name), b"old").unwrap();
        seeded.push(name);
    }
    env.fake.fail_puts(503);

    env.backup().run(NOW).await.unwrap_err();

    assert_eq!(env.fake.count("DELETE").await, 0);
    assert_eq!(env.fake.paths().len(), 40);
    for name in &seeded {
        assert!(env.settings.local_dir.join(name).exists(), "{name}");
    }
}
