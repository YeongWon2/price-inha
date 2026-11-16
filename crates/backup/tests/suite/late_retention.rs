//! 2차: 보존 지우기 실패는 올리기 성공과 따로 기록하고, 시계가 뒤로 가도 이번 백업은 지우지 않는다(E15).

use crate::common::*;
use pi_backup::{FailureKind, LastFailure, REMOTE_DIR, backup_file_name};
use pi_store::UnixMillis;

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);

// 검증: E15, C13
#[tokio::test]
async fn 올리기가_성공했으면_보존_삭제가_실패해도_성공으로_기록하고_다시_올리지_않는다() {
    let env = Env::new().await;
    std::fs::create_dir_all(&env.settings.local_dir).unwrap();
    for back in 1..=40 {
        let name = backup_file_name(plus_days(NOW, -back));
        env.fake.put_file(&format!("{REMOTE_DIR}/{name}"), b"old");
        std::fs::write(env.settings.local_dir.join(&name), b"old").unwrap();
    }
    env.fake.fail_deletes(500);

    let report = env.backup().run(NOW).await.unwrap();

    assert_eq!(report.at, NOW);
    let uploaded = format!("{REMOTE_DIR}/{}", backup_file_name(NOW));
    assert!(env.fake.paths().contains(&uploaded));
    assert!(env.settings.local_dir.join(backup_file_name(NOW)).exists());
    assert_eq!(env.fake.count("PUT").await, 1, "다시 올리지 않는다");
    let status = env.backup().status().unwrap();
    assert_eq!(status.last_success, Some(NOW));
    assert!(!status.retry_pending);
    assert_eq!(status.last_failure, None);
    assert_eq!(
        status.last_retention_failure,
        Some(LastFailure {
            at: NOW,
            kind: FailureKind::Retention
        })
    );
}

// 검증: E15, C13
#[tokio::test]
async fn 시계가_뒤로_가_새_이름의_원격_파일이_많아도_이번_백업은_지우지_않는다() {
    let env = Env::new().await;
    for ahead in 1..=8 {
        let name = backup_file_name(plus_days(NOW, ahead));
        env.fake
            .put_file(&format!("{REMOTE_DIR}/{name}"), b"future");
    }

    let report = env.backup().run(NOW).await.unwrap();

    assert!(!report.removed.contains(&NOW));
    let uploaded = format!("{REMOTE_DIR}/{}", backup_file_name(NOW));
    assert!(env.fake.paths().contains(&uploaded));
    assert!(env.settings.local_dir.join(backup_file_name(NOW)).exists());
    assert_eq!(env.backup().status().unwrap().last_success, Some(NOW));
}
