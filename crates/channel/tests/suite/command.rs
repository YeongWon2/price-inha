//! 명령 읽기(V90): 상태 · 멈춤 · 도움말 · 승인 목록 · 거절. 승인은 메신저 명령으로 받지 않는다.

use pi_channel::{Command, parse_command};

use crate::common::BOT;

fn read(text: &str) -> Command {
    parse_command(text, Some(BOT))
}

fn reject(id: &str) -> Command {
    Command::Reject { id: id.to_string() }
}

// 검증: V90
#[test]
fn 각_명령을_읽는다() {
    let table = [
        ("/status", Command::Status),
        ("/stop", Command::Stop),
        ("/help", Command::Help),
        ("/approvals", Command::Approvals),
        ("/reject ab12", reject("ab12")),
    ];
    for (text, want) in table {
        assert_eq!(read(text), want, "{text}");
    }
}

// 검증: V90
#[test]
fn 대소문자와_앞뒤_공백을_가리지_않는다() {
    let table = [
        ("  /STATUS  ", Command::Status),
        ("\n/Stop\n", Command::Stop),
        ("/HELP", Command::Help),
        ("/Approvals ", Command::Approvals),
        ("/Reject AB12", reject("AB12")),
        ("  /reject   ab12  ", reject("ab12")),
    ];
    for (text, want) in table {
        assert_eq!(read(text), want, "{text:?}");
    }
}

// 검증: V90
#[test]
fn 텔레그램의_봇_이름_붙은_꼴을_받는다() {
    let table = [
        ("/status@siheom_bot", Command::Status),
        ("/status@SIHEOM_BOT", Command::Status),
        ("/approvals@siheom_bot", Command::Approvals),
        ("/reject@siheom_bot ab12", reject("ab12")),
    ];
    for (text, want) in table {
        assert_eq!(read(text), want, "{text}");
    }
}

// 검증: V90
#[test]
fn 다른_봇에게_보낸_명령은_모르는_글이다() {
    assert!(matches!(read("/status@other_bot"), Command::Unknown(_)));
    assert!(matches!(
        parse_command("/status@siheom_bot", None),
        Command::Unknown(_)
    ));
}

// 검증: V90
#[test]
fn 승인은_메신저_명령으로_받지_않는다() {
    for text in [
        "/approve",
        "/approve ab12",
        "/APPROVE ab12 allow-once",
        "/approve@siheom_bot ab12",
    ] {
        assert_eq!(read(text), Command::ApproveNotHere, "{text}");
    }
    assert_eq!(
        read("/approvals"),
        Command::Approvals,
        "승인 목록은 다른 명령"
    );
}

// 검증: V90
#[test]
fn 그_밖의_글은_모르는_글이다() {
    assert_eq!(read("  안녕  "), Command::Unknown("안녕".to_string()));
    for text in ["/statusx", "/reject", "/unknown", "status", ""] {
        assert!(matches!(read(text), Command::Unknown(_)), "{text:?}");
    }
}
