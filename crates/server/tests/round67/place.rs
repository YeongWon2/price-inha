//! 시연의 자리: 보관 폴더는 실제 것과 따로, 기본 포트가 차 있으면 다른 빈 포트.

use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};

use price_inha::{DEMO_HOME_SUFFIX, demo_port, demo_settings};

use crate::common::{args, no_env};

const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

// 검증: V54, E56
#[test]
fn 보관_폴더를_주지_않으면_기본_폴더_옆의_demo_폴더다() {
    let settings = demo_settings(&args(&["--port", "0"]), &no_env, Some(Path::new("/u")))
        .expect("설정을 읽어야 한다");
    assert_eq!(DEMO_HOME_SUFFIX, "-demo");
    assert_eq!(settings.home, PathBuf::from("/u/.price-inha-demo"));
    assert!(settings.demo, "시연 모드여야 한다");
}

// 검증: V54, E56
#[test]
fn 환경_변수의_보관_폴더도_demo_폴더로_옮긴다() {
    let env = |name: &str| (name == "PRICE_INHA_HOME").then(|| "/srv/pi".to_string());
    let settings = demo_settings(&args(&["--port", "0"]), &env, Some(Path::new("/u")))
        .expect("설정을 읽어야 한다");
    assert_eq!(settings.home, PathBuf::from("/srv/pi-demo"));
}

// 검증: V54
#[test]
fn home을_주면_그_폴더를_그대로_쓴다() {
    let settings = demo_settings(
        &args(&["--home", "/tmp/pi-demo-x", "--port", "0"]),
        &no_env,
        Some(Path::new("/u")),
    )
    .expect("설정을 읽어야 한다");
    assert_eq!(settings.home, PathBuf::from("/tmp/pi-demo-x"));
    assert!(settings.demo);
}

// 검증: V54
#[test]
fn 직접_준_포트는_그대로_쓴다() {
    let settings = demo_settings(&args(&["--port", "7911"]), &no_env, Some(Path::new("/u")))
        .expect("설정을 읽어야 한다");
    assert_eq!(settings.port, 7911);
    let busy = TcpListener::bind((LOOPBACK, 0)).unwrap();
    let port = busy.local_addr().unwrap().port();
    assert_eq!(
        demo_port(LOOPBACK, port, true),
        port,
        "직접 준 포트는 바꾸지 않는다"
    );
}

// 검증: V54
#[test]
fn 기본_포트가_비어_있으면_그_포트다() {
    let probe = TcpListener::bind((LOOPBACK, 0)).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    assert_eq!(demo_port(LOOPBACK, port, false), port);
}

// 검증: V54
#[test]
fn 기본_포트가_차_있으면_다른_빈_포트를_고른다() {
    let busy = TcpListener::bind((LOOPBACK, 0)).unwrap();
    let port = busy.local_addr().unwrap().port();
    let chosen = demo_port(LOOPBACK, port, false);
    assert_ne!(chosen, port, "차 있는 포트를 고르면 안 된다");
    assert_ne!(chosen, 0, "실제 포트 번호여야 한다");
    TcpListener::bind((LOOPBACK, chosen)).expect("고른 포트는 비어 있어야 한다");
}
