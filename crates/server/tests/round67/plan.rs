//! `price-inha demo [--demo-day-minutes <수>]`: 하루 길이(분)를 읽는다. 기본 20, 0이면 짧은 하루를 끈다(하루 종일 장).

use std::path::{Path, PathBuf};

use price_inha::{DEMO_DAY_MINUTES_DEFAULT, demo_plan};

use crate::common::{args, no_env};

// 검증: D1, V54
#[test]
fn 하루_길이를_주지_않으면_20분이다() {
    let (settings, plan) = demo_plan(
        &args(&["--port", "0"]),
        &no_env,
        Some(Path::new("/u")),
        true,
    )
    .expect("읽어야 한다");
    assert_eq!(plan.day_minutes, DEMO_DAY_MINUTES_DEFAULT);
    assert!(plan.request.no_open);
    assert_eq!(settings.home, PathBuf::from("/u/.price-inha-demo"));
    assert!(settings.demo);
}

// 검증: D1, V54
#[test]
fn 하루_길이를_읽고_설정_인자에서는_뺀다() {
    let given = args(&["--demo-day-minutes", "30", "--port", "0"]);
    let (settings, plan) = demo_plan(&given, &no_env, Some(Path::new("/u")), false).unwrap();
    assert_eq!(plan.day_minutes, 30);
    assert_eq!(settings.port, 0);
    let (_, off) = demo_plan(
        &args(&["--port", "0", "--demo-day-minutes", "0"]),
        &no_env,
        Some(Path::new("/u")),
        false,
    )
    .unwrap();
    assert_eq!(off.day_minutes, 0, "0이면 짧은 하루를 끈다");
}

// 검증: D1
#[test]
fn 너무_짧거나_읽을_수_없는_하루_길이는_거부한다() {
    for bad in [
        &["--demo-day-minutes", "3"][..],
        &["--demo-day-minutes", "x"],
        &["--demo-day-minutes"],
    ] {
        let mut given = args(bad);
        given.extend(args(&["--port", "0"]));
        assert!(
            demo_plan(&given, &no_env, Some(Path::new("/u")), false).is_err(),
            "{bad:?}"
        );
    }
}
