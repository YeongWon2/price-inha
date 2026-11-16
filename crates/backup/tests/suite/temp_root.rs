//! 2차: 올릴 때 평문 스냅샷 사본은 설정한 `temp_root` 안에만 두고 시스템 임시 폴더에는 만들지 않는다(P22 · E39).
//!
//! 시스템 임시 폴더(`TMPDIR`)는 프로세스 전체 값이라 안에서 바꿀 수 없다. 그래서 같은 시험 파일을 자식
//! 프로세스로 다시 돌리되 `TMPDIR`를 쓰기 막은 폴더로 준다. 시스템 임시 폴더에 무언가 만들려 하면 실패한다.

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use crate::common::*;
use pi_store::UnixMillis;

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);
const WORK_VAR: &str = "PI_BACKUP_TEST_CHILD_BASE";
const CHILD: &str = "temp_root::쓰기_막힌_임시_폴더에서_올리기_자식";

// 검증: P22, E39
#[test]
fn 검증_중에도_시스템_임시_폴더에_평문_사본을_만들지_않는다() {
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
async fn 쓰기_막힌_임시_폴더에서_올리기_자식() {
    let Some(base) = std::env::var_os(WORK_VAR) else {
        return;
    };
    let env = Env::new_in(std::path::Path::new(&base)).await;

    let report = env.backup().run(NOW).await.unwrap();

    assert_eq!(report.at, NOW);
    assert_eq!(env.fake.paths().len(), 1);
    assert_eq!(names_in(&env.settings.temp_root), Vec::<String>::new());
}
