//! backup 7차 시험(백로그 U 2번 "doctor 읽기 전용 손잡이"): 백업기([`Backup`])를 만들지 않고(저장소를 열지 않고)
//! 상태 파일만 읽는 [`read_status`]. 백업기가 남긴 상태와 같은 값을 주고, 읽기만 한다(폴더 · 파일을 만들지 않는다).
//!
//! 잠긴 `tests/suite` 묶음을 고치지 않으려고 따로 둔 시험 바이너리다. 바깥 GitHub를 부르지 않는다(토큰이 없어
//! 요청 전에 실패한다).

use std::sync::Arc;

use pi_backup::{Backup, BackupSettings, BackupStatus, FailureKind, read_status};
use pi_secrets::SecretStore;
use pi_store::{Store, UnixMillis};

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);

// 검증: E15
#[tokio::test]
async fn 백업기가_남긴_상태를_백업기_없이_같게_읽는다() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path().join("price-inha.sqlite")).unwrap();
    let secrets = Arc::new(SecretStore::open(home.path().to_path_buf()).unwrap());
    let local_dir = home.path().join("backups");
    let settings = BackupSettings {
        api_base: "http://127.0.0.1:9".to_string(),
        repo: "owner/name".to_string(),
        recipient: "age1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq".to_string(),
        local_dir: local_dir.clone(),
        temp_root: home.path().join("tmp"),
    };
    let backup = Backup::new(settings, store, secrets).unwrap();
    let failed = backup.run(NOW).await.expect_err("토큰이 없으면 실패한다");
    assert_eq!(failed.notice.kind, FailureKind::TokenMissing);

    let read = read_status(&local_dir).expect("상태 파일을 읽는다");
    assert_eq!(read, backup.status().unwrap(), "백업기가 보는 것과 같다");
    assert!(read.retry_pending);
    assert_eq!(read.last_failure.map(|f| f.at), Some(NOW));
}

// 검증: E15
#[test]
fn 상태_파일이_없으면_기본값이고_폴더도_만들지_않는다() {
    let home = tempfile::tempdir().unwrap();
    let local_dir = home.path().join("backups");
    assert_eq!(read_status(&local_dir).unwrap(), BackupStatus::default());
    assert!(!local_dir.exists(), "읽기만 한다");
}
