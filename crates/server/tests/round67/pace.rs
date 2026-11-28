//! (결정 36) 짧은 하루에서는 가짜 시세를 더 자주(기본 250ms마다) 받아, 장중 15분 안에 익절 · 손절까지 닿을 만큼
//! 값이 움직이게 한다. 시세 간격을 직접 줬거나 짧은 하루를 껐으면 그대로다.

use std::path::Path;
use std::time::Duration;

use price_inha::{DEFAULT_QUOTE_INTERVAL, DEMO_DAY_QUOTE_INTERVAL, demo_plan};

use crate::common::{args, no_env};

// 검증: V54
#[test]
fn 짧은_하루에서는_가짜_시세를_더_자주_받는다() {
    assert_eq!(DEMO_DAY_QUOTE_INTERVAL, Duration::from_millis(250));
    let (settings, _) = demo_plan(
        &args(&["--port", "0"]),
        &no_env,
        Some(Path::new("/u")),
        true,
    )
    .unwrap();
    assert_eq!(settings.quote_interval, DEMO_DAY_QUOTE_INTERVAL);
}

// 검증: V54
#[test]
fn 시세_간격을_줬거나_짧은_하루를_껐으면_그대로다() {
    let given = args(&["--port", "0", "--interval-ms", "700"]);
    let (settings, _) = demo_plan(&given, &no_env, Some(Path::new("/u")), true).unwrap();
    assert_eq!(settings.quote_interval, Duration::from_millis(700));
    let env = |name: &str| (name == "PRICE_INHA_QUOTE_INTERVAL_MS").then(|| "900".to_string());
    let (settings, _) =
        demo_plan(&args(&["--port", "0"]), &env, Some(Path::new("/u")), true).unwrap();
    assert_eq!(settings.quote_interval, Duration::from_millis(900));
    let off = args(&["--port", "0", "--demo-day-minutes", "0"]);
    let (settings, _) = demo_plan(&off, &no_env, Some(Path::new("/u")), true).unwrap();
    assert_eq!(settings.quote_interval, DEFAULT_QUOTE_INTERVAL);
}
