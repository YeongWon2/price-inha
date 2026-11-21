//! 하위 명령 읽기: 인자 없이 띄우는 지금 동작은 `run`과 같고, 새 하위 명령은 제 모양으로 읽힌다.

use std::path::PathBuf;

use price_inha::{CliError, Command, ServiceAction};

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| (*arg).to_string()).collect()
}

// 검증: D1
#[test]
fn 인자_없이_띄우기와_run은_같은_명령이다() {
    assert_eq!(
        Command::parse(&[]).unwrap(),
        Command::Run { args: Vec::new() }
    );
    assert_eq!(
        Command::parse(&args(&["run"])).unwrap(),
        Command::Run { args: Vec::new() }
    );
    let given = args(&["--home", "/srv/pi", "--port", "8123", "--demo"]);
    let bare = Command::parse(&given).unwrap();
    let mut with_run = args(&["run"]);
    with_run.extend(given.clone());
    assert_eq!(bare, Command::Run { args: given });
    assert_eq!(
        Command::parse(&with_run).unwrap(),
        bare,
        "run을 붙여도 같다"
    );
}

// 검증: D1
#[test]
fn 판_번호_명령을_읽는다() {
    assert_eq!(
        Command::parse(&args(&["--version"])).unwrap(),
        Command::Version
    );
    assert!(!price_inha::VERSION.is_empty());
}

// 검증: D1
#[test]
fn service_하위_명령을_읽는다() {
    let install = Command::parse(&args(&[
        "service",
        "install",
        "--start-now",
        "--dry-run",
        "--port",
        "8123",
    ]))
    .unwrap();
    assert_eq!(
        install,
        Command::Service {
            action: ServiceAction::Install {
                start_now: true,
                dry_run: true
            },
            args: args(&["--port", "8123"]),
        }
    );
    let plain = Command::parse(&args(&["service", "install"])).unwrap();
    assert_eq!(
        plain,
        Command::Service {
            action: ServiceAction::Install {
                start_now: false,
                dry_run: false
            },
            args: Vec::new(),
        }
    );
    let uninstall = Command::parse(&args(&["service", "uninstall", "--dry-run"])).unwrap();
    assert_eq!(
        uninstall,
        Command::Service {
            action: ServiceAction::Uninstall { dry_run: true },
            args: Vec::new(),
        }
    );
    for (word, action) in [
        ("start", ServiceAction::Start),
        ("stop", ServiceAction::Stop),
        ("restart", ServiceAction::Restart),
        ("status", ServiceAction::Status),
    ] {
        assert_eq!(
            Command::parse(&args(&["service", word, "--home", "/srv/pi"])).unwrap(),
            Command::Service {
                action,
                args: args(&["--home", "/srv/pi"]),
            },
            "{word}"
        );
    }
}

// 검증: D1
#[test]
fn 모르는_service_일은_거부한다() {
    assert!(Command::parse(&args(&["service", "explode"])).is_err());
    assert!(Command::parse(&args(&["service"])).is_err());
}

// 검증: D1 N8
#[test]
fn doctor와_access_key를_읽는다() {
    assert_eq!(
        Command::parse(&args(&["doctor", "--json", "--home", "/srv/pi"])).unwrap(),
        Command::Doctor {
            json: true,
            args: args(&["--home", "/srv/pi"]),
        }
    );
    assert_eq!(
        Command::parse(&args(&["doctor"])).unwrap(),
        Command::Doctor {
            json: false,
            args: Vec::new(),
        }
    );
    assert_eq!(
        Command::parse(&args(&["access-key", "--rotate"])).unwrap(),
        Command::AccessKey {
            rotate: true,
            args: Vec::new(),
        }
    );
    assert_eq!(
        Command::parse(&args(&["access-key"])).unwrap(),
        Command::AccessKey {
            rotate: false,
            args: Vec::new(),
        }
    );
}

// 검증: V51
#[test]
fn restore를_읽고_빠진_인자는_거부한다() {
    let parsed = Command::parse(&args(&[
        "restore",
        "/backups/pi.age",
        "--identity",
        "/keys/pi.txt",
        "--into",
        "/srv/pi-restored",
    ]))
    .unwrap();
    assert_eq!(
        parsed,
        Command::Restore {
            backup: PathBuf::from("/backups/pi.age"),
            identity: PathBuf::from("/keys/pi.txt"),
            into: PathBuf::from("/srv/pi-restored"),
        }
    );
    let no_identity = Command::parse(&args(&["restore", "/backups/pi.age", "--into", "/srv/x"]));
    assert!(
        matches!(no_identity, Err(CliError::MissingArgument { .. })),
        "{no_identity:?}"
    );
    let no_into = Command::parse(&args(&["restore", "/backups/pi.age", "--identity", "/k"]));
    assert!(
        matches!(no_into, Err(CliError::MissingArgument { .. })),
        "{no_into:?}"
    );
    let no_file = Command::parse(&args(&["restore", "--identity", "/k", "--into", "/srv/x"]));
    assert!(no_file.is_err(), "백업 파일이 없으면 거부");
}

// 검증: D1
#[test]
fn 모르는_하위_명령은_거부한다() {
    let unknown = Command::parse(&args(&["launch-rockets"]));
    assert!(
        matches!(unknown, Err(CliError::UnknownCommand(_))),
        "{unknown:?}"
    );
}
