//! 3차: 보존 지우기가 중간에 실패해도 그때까지 지운 백업은 `removed`에 담는다(E15 · C13).

use crate::common::*;
use pi_backup::{FailureKind, REMOTE_DIR, backup_file_name};
use pi_store::UnixMillis;

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);

// 검증: E15, C13
#[tokio::test]
async fn 보존_지우기가_중간에_실패해도_이미_지운_백업은_removed에_담긴다() {
    let env = Env::new().await;
    std::fs::create_dir_all(&env.settings.local_dir).unwrap();
    let olds: Vec<UnixMillis> = (1..=40).map(|back| plus_days(NOW, -back)).collect();
    for &at in &olds {
        let name = backup_file_name(at);
        env.fake.put_file(&format!("{REMOTE_DIR}/{name}"), b"old");
        std::fs::write(env.settings.local_dir.join(&name), b"old").unwrap();
    }
    env.fake.fail_deletes_after(3, 500);

    let report = env.backup().run(NOW).await.unwrap();

    assert_eq!(report.removed.len(), 3, "{:?}", report.removed);
    let remote = env.fake.paths();
    for &at in &olds {
        let name = backup_file_name(at);
        let on_remote = remote.contains(&format!("{REMOTE_DIR}/{name}"));
        let on_local = env.settings.local_dir.join(&name).exists();
        if report.removed.contains(&at) {
            assert!(
                !on_remote && !on_local,
                "지웠다고 한 것은 양쪽에서 없다: {name}"
            );
        } else {
            assert!(
                on_remote && on_local,
                "지우지 않은 것은 양쪽에 남는다: {name}"
            );
        }
    }
    let status = env.backup().status().unwrap();
    assert_eq!(status.last_success, Some(NOW));
    assert_eq!(
        status.last_retention_failure.map(|f| f.kind),
        Some(FailureKind::Retention)
    );
}
