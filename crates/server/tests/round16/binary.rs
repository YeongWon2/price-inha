//! 실행 파일 자체: `--version`은 보관 폴더 없이 답하고, `run`을 붙여도 지금 동작과 같다.
//! 서버를 띄우지 않는 경우만 부른다(곧 끝난다).

use std::path::Path;
use std::process::{Command, Output};

/// 보관 폴더 · 사용자 홈을 임시 폴더 안 없는 곳으로 두고 실행 파일을 부른다.
fn price_inha(args: &[&str], root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_price-inha"))
        .args(args)
        .env_clear()
        .env(price_inha::ENV_HOME, root.join("pi-home"))
        .env("HOME", root.join("user"))
        .env("USERPROFILE", root.join("user"))
        .output()
        .unwrap()
}

// 검증: D1
#[test]
fn 판_번호는_보관_폴더_없이도_답한다() {
    let root = tempfile::tempdir().unwrap();
    let out = price_inha(&["--version"], root.path());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "판 번호를 보여 준다: {stdout}"
    );
    assert!(
        !root.path().join("pi-home").exists(),
        "보관 폴더를 만들지 않는다"
    );
    assert!(
        !root.path().join("user").exists(),
        "사용자 홈도 건드리지 않는다"
    );
}

// 검증: D1
#[test]
fn run을_붙여도_설정_오류는_지금과_같다() {
    let root = tempfile::tempdir().unwrap();
    let bare = price_inha(&["--port", "not-a-port"], root.path());
    let run = price_inha(&["run", "--port", "not-a-port"], root.path());
    assert_eq!(bare.status.code(), Some(2), "설정 치명 오류는 종료 코드 2");
    assert_eq!(run.status.code(), bare.status.code(), "같은 종료 코드");
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&bare.stderr),
        "같은 안내"
    );
}
