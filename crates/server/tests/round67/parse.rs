//! `price-inha demo [--no-open] [설정 인자들]`을 읽는다.

use price_inha::Command;

use crate::common::args;

// 검증: D1
#[test]
fn demo는_인자_없이도_읽는다() {
    let command = Command::parse(&args(&["demo"])).expect("demo를 읽어야 한다");
    assert_eq!(
        command,
        Command::Demo {
            no_open: false,
            args: Vec::new(),
        }
    );
}

// 검증: D1
#[test]
fn demo는_no_open과_설정_인자를_읽는다() {
    let command = Command::parse(&args(&[
        "demo",
        "--home",
        "/tmp/pi-demo-x",
        "--no-open",
        "--port",
        "7911",
    ]))
    .expect("demo를 읽어야 한다");
    assert_eq!(
        command,
        Command::Demo {
            no_open: true,
            args: args(&["--home", "/tmp/pi-demo-x", "--port", "7911"]),
        }
    );
}

// 검증: D1
#[test]
fn demo_안의_version도_먼저_답하지_않고_설정_인자로_넘긴다() {
    // `--version`은 첫 인자일 때만 판 번호다. demo 뒤의 모르는 인자는 설정 읽기가 거부한다.
    let command = Command::parse(&args(&["demo", "--bind", "127.0.0.1"])).unwrap();
    assert_eq!(
        command,
        Command::Demo {
            no_open: false,
            args: args(&["--bind", "127.0.0.1"]),
        }
    );
}
