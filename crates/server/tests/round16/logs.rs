//! (9차 보강 3) 기록 자리: macOS는 정의의 두 경로, Windows는 `run`의 `--log-file` 인자(예약 작업은 표준 출력을
//! 파일로 돌리지 못한다), Linux는 없음(journal). `run --log-file <경로>`가 설정으로 읽힌다.

use std::path::Path;

use price_inha::{Command, Os, SERVER_ERROR_LOG, SERVER_LOG, SERVER_LOGS_DIR, Service, Settings};

use crate::common::{FakeRunner, setup, squash};

/// `--log-file` 인자 이름.
const LOG_FILE_ARG: &str = "--log-file";

fn contents(os: Os, home: &Path, definitions: &Path) -> String {
    let runner = FakeRunner::ok();
    let definition = Service::new(setup(os, home, definitions), &runner).definition();
    assert!(
        runner.commands().is_empty(),
        "정의를 만들 때는 부르지 않는다"
    );
    definition.contents
}

// 검증: D1
#[test]
fn 운영체제마다_정의가_기록_자리를_가리킨다() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    let h = home.path().to_str().unwrap();

    let plist = squash(&contents(Os::MacOs, home.path(), defs.path()));
    let logs = home.path().join(SERVER_LOGS_DIR);
    let wanted = format!(
        "<key>StandardErrorPath</key><string>{}</string>",
        logs.join(SERVER_ERROR_LOG).to_str().unwrap()
    );
    assert!(plist.contains(&wanted), "{wanted} 없음:\n{plist}");
    // (68차, 백로그 U 5번) 표준 출력은 버리고 서버가 `--log-file`로 돌려 쓴다.
    assert!(plist.contains("<key>StandardOutPath</key><string>/dev/null</string>"));
    let wanted = format!(
        "<string>{LOG_FILE_ARG}</string><string>{}</string>",
        logs.join(SERVER_LOG).to_str().unwrap()
    );
    assert!(
        plist.contains(&wanted),
        "macOS는 인자로 기록 파일을 싣는다:\n{plist}"
    );

    let unit = contents(Os::Linux, home.path(), defs.path());
    for absent in [
        LOG_FILE_ARG,
        SERVER_LOG,
        "StandardOutput=",
        "StandardError=",
    ] {
        assert!(
            !unit.contains(absent),
            "Linux는 journal이다({absent} 없음):\n{unit}"
        );
    }

    let xml = squash(&contents(Os::Windows, home.path(), defs.path()));
    let arguments = &xml[xml.find("<Arguments>").expect("인자")..];
    let arguments = arguments[..arguments.find("</Arguments>").unwrap()].replace('"', "");
    let wanted = format!(r"{LOG_FILE_ARG} {h}\{SERVER_LOGS_DIR}\{SERVER_LOG}");
    assert!(
        arguments.contains(&wanted),
        "Windows는 인자로 기록 파일을 싣는다({wanted}): {arguments}"
    );
}

// 검증: D1
#[test]
fn run의_log_file_인자가_읽힌다() {
    let home = tempfile::tempdir().unwrap();
    let log = home.path().join("logs").join("server.log");
    let args: Vec<String> = [
        "run",
        "--home",
        home.path().to_str().unwrap(),
        LOG_FILE_ARG,
        log.to_str().unwrap(),
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    let Command::Run { args: rest } = Command::parse(&args).unwrap() else {
        panic!("run으로 읽혀야 한다");
    };
    assert_eq!(rest, args[1..], "나머지 인자는 그대로 설정으로 간다");

    let settings = Settings::parse(&rest, &|_| None, None).expect("--log-file을 안다");
    assert_eq!(settings.log_file.as_deref(), Some(log.as_path()));
    assert!(!log.exists(), "읽기만으로는 파일을 만들지 않는다");

    let plain = Settings::parse(&rest[..2], &|_| None, None).unwrap();
    assert_eq!(plain.log_file, None, "주지 않으면 표준 오류 그대로");
}
