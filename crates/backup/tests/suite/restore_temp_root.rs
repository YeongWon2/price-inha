//! 3차: 되살리기도 검증 사본을 넘겨받은 임시 폴더 안에만 두고 시스템 임시 폴더에는 만들지 않는다(V51 · P22).
//!
//! 2차 `temp_root`와 같은 방법: 같은 시험 파일을 자식 프로세스로 다시 돌리되 `TMPDIR`를 쓰기 막은 폴더로 준다.

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use crate::common::*;
use pi_backup::{SNAPSHOT_FILE, restore_in};
use pi_store::UnixMillis;

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);
const LATER: UnixMillis = UnixMillis(NOW.0 + 3_600_000);
const WORK_VAR: &str = "PI_BACKUP_TEST_RESTORE_BASE";
const CHILD: &str = "restore_temp_root::쓰기_막힌_임시_폴더에서_되살리기_자식";

// 검증: V51, P22
#[test]
fn 되살리기_검증_중에도_시스템_임시_폴더에_평문_사본을_만들지_않는다() {
    let outer = tempfile::tempdir().unwrap();
    let base = outer.path().join("base");
    let blocked = outer.path().join("blocked");
    std::fs::create_dir(&base).unwrap();
    std::fs::create_dir(&blocked).unwrap();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o555)).unwrap();
    assert!(
        std::fs::File::create(blocked.join("probe")).is_err(),
        "쓰기 막힌 폴더여야 시험이 뜻을 가진다"
    );

    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture", "--test-threads=1"])
        .env("TMPDIR", &blocked)
        .env(WORK_VAR, &base)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.contains("1 passed"),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(names_in(&blocked), Vec::<String>::new());
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// 위 시험이 자식 프로세스로만 부른다. 변수가 없으면(보통 실행) 아무것도 하지 않는다.
#[tokio::test]
async fn 쓰기_막힌_임시_폴더에서_되살리기_자식() {
    let Some(base) = std::env::var_os(WORK_VAR) else {
        return;
    };
    let env = Env::new_in(std::path::Path::new(&base)).await;
    let report = env.backup().run(NOW).await.unwrap();
    let sealed = env.fake.file(&report.remote_path).unwrap();
    let target = env.target("restored");
    let scratch = env.target("scratch");
    std::fs::create_dir(&scratch).unwrap();

    let restored = restore_in(&sealed, &env.identity_secret(), &target, &scratch, LATER).unwrap();

    assert_eq!(restored.manifest.files[0].path, SNAPSHOT_FILE);
    assert!(target.join(SNAPSHOT_FILE).exists());
    assert!(restored.outcome.team_record.is_some());
    assert_eq!(
        names_in(&scratch),
        Vec::<String>::new(),
        "검증 사본은 지운다"
    );
}
