//! 1번 · 3번 · E56: 실행 파일 조립 값. 시연 달력 · 시연용 가짜 공시는 실행 파일의 `--demo` 조립에서만 켜고,
//! 실제(시연 아님) 조립에는 섞이지 않는다. 시연 어댑터의 기본(`DemoWiring::default`)은 꺼짐 그대로다.

use std::time::Duration;

use price_inha::{DEMO_DISCLOSURE_EVERY, DEMO_DISCLOSURE_MARK, DemoWiring, run_assembly};

use crate::common::settings;

// 검증: V54 D14 E56
#[test]
fn 시연_설정의_실행_조립은_시연_달력과_3분마다_가짜_공시를_켠다() {
    let home = tempfile::tempdir().unwrap();
    let assembly = run_assembly(&settings(home.path(), true), None);
    assert_eq!(
        assembly.demo,
        DemoWiring {
            calendar: true,
            disclosure_every: Some(DEMO_DISCLOSURE_EVERY),
        },
        "--demo 조립은 시연 달력과 가짜 공시를 켠다"
    );
    assert_eq!(
        assembly.demo,
        DemoWiring::binary(),
        "실행 파일의 시연 값이다"
    );
    assert_eq!(
        DEMO_DISCLOSURE_EVERY,
        Duration::from_secs(180),
        "가짜 공시 기본 간격은 3분이다"
    );
    assert_eq!(
        DEMO_DISCLOSURE_MARK, "시연",
        "가짜 공시에는 '시연'이 적힌다"
    );
    assert!(
        assembly.launch.team.is_some(),
        "실행 파일 조립은 투자 팀을 잇는다"
    );
}

// 검증: V54 E56 D14
#[test]
fn 실제_설정의_실행_조립에는_시연_달력도_가짜_공시도_없다() {
    let home = tempfile::tempdir().unwrap();
    let assembly = run_assembly(&settings(home.path(), false), None);
    assert_eq!(
        assembly.demo,
        DemoWiring::default(),
        "시연이 아니면 시연 값은 모두 꺼짐이다"
    );
    assert!(
        !assembly.demo.calendar && assembly.demo.disclosure_every.is_none(),
        "실제 모드에 시연 달력 · 가짜 공시가 섞이지 않는다"
    );
    assert!(
        assembly.launch.trading.calendar.is_none(),
        "실제 모드의 달력은 시장 어댑터의 것 그대로다(조립이 정한 달력 없음)"
    );
    assert!(
        assembly.launch.team.is_some(),
        "실행 파일 조립은 투자 팀을 잇는다"
    );
}

// 검증: V54 E56
#[test]
fn 시연_조립의_기본값은_꺼짐이라_기존_시연_어댑터_동작이_바뀌지_않는다() {
    let off = DemoWiring::default();
    assert!(!off.calendar, "기본은 달력 없음");
    assert_eq!(off.disclosure_every, None, "기본은 가짜 공시 없음");
    assert_ne!(off, DemoWiring::binary(), "켜는 길은 실행 파일 조립뿐이다");
}
