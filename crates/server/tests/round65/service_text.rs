//! 서비스 정의의 글자(지시서 9차 보강 4 "다음 차수 후보"):
//! - systemd: `ExecStart`의 `$`는 `$$`(환경 변수 풀이를 막는다), `%`는 `%%`(지정자 풀이를 막는다).
//!   `WorkingDirectory`의 `%`도 `%%`. 정의를 다시 읽을 때(다른 홈 거부)는 되돌려 읽는다.
//! - Windows: 인자가 `\`로 끝나고 따옴표로 감쌀 때 끝의 `\`를 겹쳐 닫는 따옴표가 글자로 읽히지 않게 한다
//!   (`CommandLineToArgvW` 규칙). 따옴표 앞의 `\`도 겹친다.

use price_inha::{InstallOptions, Os, Service, ServiceError};

use crate::common::{ScriptedRunner, setup};

fn exec_line(unit: &str) -> &str {
    unit.lines()
        .find(|line| line.starts_with("ExecStart="))
        .expect("ExecStart 줄")
}

// 검증: D1
#[test]
fn systemd_정의는_달러와_퍼센트를_풀이되지_않게_적는다() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("홈 $HOME 100%");
    std::fs::create_dir_all(&home).unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = ScriptedRunner::ok();
    let service = Service::new(
        setup(Os::Linux, &home, defs.path(), &["--symbols", "A$B"]),
        &runner,
    );
    let unit = service.definition().contents;
    let exec = exec_line(&unit);
    assert!(exec.contains("$$HOME"), "{exec}");
    assert!(exec.contains("100%%"), "{exec}");
    assert!(exec.contains("A$$B"), "{exec}");
    assert!(
        !exec.replace("$$", "").contains('$'),
        "홀로 선 $가 없다: {exec}"
    );
    let working = unit
        .lines()
        .find(|line| line.starts_with("WorkingDirectory="))
        .unwrap();
    assert!(working.ends_with("100%%"), "{working}");

    service
        .install(InstallOptions {
            start_now: false,
            dry_run: false,
        })
        .unwrap();
    assert_eq!(
        service.installed_home().unwrap().as_deref(),
        Some(home.as_path()),
        "다시 읽으면 원래 홈이다"
    );
    // 같은 홈이면 다시 쓸 수 있다(다른 홈으로 잘못 읽어 거부하지 않는다).
    let again = service.install(InstallOptions {
        start_now: false,
        dry_run: false,
    });
    assert!(
        !matches!(again, Err(ServiceError::OtherHome { .. })),
        "{again:?}"
    );
}

// 검증: D1
#[test]
fn windows_인자_끝의_역빗금은_따옴표를_닫게_겹친다() {
    let defs = tempfile::tempdir().unwrap();
    let home = std::path::PathBuf::from(r"C:\Users\me\price inha\");
    let runner = ScriptedRunner::ok();
    let service = Service::new(
        setup(
            Os::Windows,
            &home,
            defs.path(),
            &["--symbols", r#"a"b\"#, "--log-file", r"D:\logs dir\"],
        ),
        &runner,
    );
    let xml = service.definition().contents;
    let arguments = xml
        .split("<Arguments>")
        .nth(1)
        .and_then(|rest| rest.split("</Arguments>").next())
        .expect("Arguments 칸");
    assert!(
        arguments.contains(r#""C:\Users\me\price inha\\""#),
        "끝의 \\는 겹친다: {arguments}"
    );
    assert!(arguments.contains(r#""D:\logs dir\\""#), "{arguments}");
    assert!(
        arguments.contains(r#""a\"b\\""#),
        "따옴표는 \\\" · 끝 \\는 겹친다: {arguments}"
    );
    assert!(
        arguments.contains(r"--symbols "),
        "따옴표가 필요 없는 인자는 그대로: {arguments}"
    );
}
