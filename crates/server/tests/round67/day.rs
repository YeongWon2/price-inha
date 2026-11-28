//! (결정 36) 체험판의 짧은 하루: 실제 20분이 시연 달력의 한 거래일이다.
//!
//! 한 바퀴(기본 20분) = 장 시작 1분 전(시장 시각 00:04) → 장중 15분(00:05 ~ 00:20) → 장 마감(23:55로 건너뜀) →
//! 마감 뒤 4분(23:55 ~ 23:59) → 다음 거래일 00:04. 시연 달력(N19, 00:05 ~ 23:55)은 그대로다.
//! 시작점은 체험 보관 폴더의 [`DEMO_HOME_FILE`]에 남아, 다시 켜도 시각이 뒤로 가지 않는다.

use chrono::{Duration, NaiveDate};
use pi_market::{Market, SessionKind};
use price_inha::{
    DEMO_DAY_MINUTES_DEFAULT, DEMO_HOME_FILE, DemoDayOrigin, demo_calendar, demo_day_origin,
    demo_day_time, is_demo_home, save_demo_day,
};

use crate::clock::{kst, plus, tuesday};

fn origin(minutes: u32) -> DemoDayOrigin {
    DemoDayOrigin {
        real: kst(tuesday(), 10, 0, 0),
        date: tuesday(),
        minutes,
    }
}

fn day(offset: i64) -> NaiveDate {
    tuesday() + Duration::days(offset)
}

fn in_session(at: chrono::DateTime<chrono::Utc>) -> bool {
    let calendar = demo_calendar(Market::Kr, at.with_timezone(&kst_zone()).date_naive());
    calendar
        .session_at(at)
        .is_some_and(|session| session.kind == SessionKind::Regular)
}

fn kst_zone() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(9 * 3600).unwrap()
}

// 검증: V54, C33
#[test]
fn 기본_하루는_20분이고_장_시작_1분_전에서_시작한다() {
    assert_eq!(DEMO_DAY_MINUTES_DEFAULT, 20);
    let o = origin(20);
    assert_eq!(
        demo_day_time(&o, Market::Kr, o.real),
        kst(tuesday(), 0, 4, 0)
    );
    assert!(
        !in_session(demo_day_time(&o, Market::Kr, o.real)),
        "아직 장 전"
    );
}

// 검증: V54, C33
#[test]
fn 일분_뒤에_장이_열리고_15분_동안_장중이다() {
    let o = origin(20);
    let open = demo_day_time(&o, Market::Kr, plus(o.real, 60));
    assert_eq!(open, kst(tuesday(), 0, 5, 0));
    assert!(in_session(open));
    let last = demo_day_time(&o, Market::Kr, plus(o.real, 16 * 60 - 1));
    assert_eq!(last, kst(tuesday(), 0, 19, 59));
    assert!(in_session(last));
}

// 검증: V54, C33
#[test]
fn 십육분에_장이_닫히고_마감_뒤_4분이_흐른다() {
    let o = origin(20);
    let close = demo_day_time(&o, Market::Kr, plus(o.real, 16 * 60));
    assert_eq!(close, kst(tuesday(), 23, 55, 0));
    assert!(!in_session(close), "장 마감 뒤");
    let late = demo_day_time(&o, Market::Kr, plus(o.real, 20 * 60 - 1));
    assert_eq!(late, kst(tuesday(), 23, 58, 59));
}

// 검증: V54, C33
#[test]
fn 이십분이면_다음_거래일이고_두_시간이면_엿새_뒤다() {
    let o = origin(20);
    assert_eq!(
        demo_day_time(&o, Market::Kr, plus(o.real, 20 * 60)),
        kst(day(1), 0, 4, 0)
    );
    assert_eq!(
        demo_day_time(&o, Market::Kr, plus(o.real, 120 * 60)),
        kst(day(6), 0, 4, 0)
    );
}

// 검증: V54
#[test]
fn 체험_시각은_뒤로_가지_않는다() {
    let o = origin(20);
    let mut before = demo_day_time(&o, Market::Kr, o.real);
    for step in 1..(3 * 20 * 60 / 7) {
        let now = demo_day_time(&o, Market::Kr, plus(o.real, step * 7));
        assert!(now >= before, "{step}: {now} < {before}");
        before = now;
    }
    // 시작점보다 앞선 실제 시각도 시작점으로 본다.
    assert_eq!(
        demo_day_time(&o, Market::Kr, plus(o.real, -600)),
        kst(tuesday(), 0, 4, 0)
    );
}

// 검증: V54, E56
#[test]
fn 시작점은_체험_폴더에_남고_다시_읽으면_같다() {
    let dir = tempfile::tempdir().unwrap();
    let now = kst(tuesday(), 10, 0, 0);
    let first = demo_day_origin(dir.path(), 20, Market::Kr, now);
    assert_eq!(first, origin(20), "처음이면 지금 · 오늘 · 그 길이");
    assert!(
        !is_demo_home(dir.path()),
        "남기기 전에는 체험 폴더가 아니다"
    );
    save_demo_day(dir.path(), &first).unwrap();
    assert!(dir.path().join(DEMO_HOME_FILE).is_file());
    assert!(is_demo_home(dir.path()));
    let again = demo_day_origin(dir.path(), 20, Market::Kr, plus(now, 300));
    assert_eq!(again, first, "다시 켜도 같은 시작점");
}

// 검증: V54
#[test]
fn 하루_길이를_바꾸면_다음_거래일부터_새_길이로_이어진다() {
    let dir = tempfile::tempdir().unwrap();
    let now = kst(tuesday(), 10, 0, 0);
    let first = demo_day_origin(dir.path(), 20, Market::Kr, now);
    save_demo_day(dir.path(), &first).unwrap();
    let later = plus(now, 25 * 60);
    let old_time = demo_day_time(&first, Market::Kr, later);
    let changed = demo_day_origin(dir.path(), 30, Market::Kr, later);
    assert_eq!(changed.minutes, 30);
    assert_eq!(changed.real, later);
    assert_eq!(
        changed.date,
        day(2),
        "지금 체험 날짜(다음 날)의 다음 거래일"
    );
    assert!(demo_day_time(&changed, Market::Kr, later) >= old_time);
}

// 검증: V54
#[test]
fn 하루_길이_0은_짧은_하루를_끄고_체험_폴더_표시만_남긴다() {
    let dir = tempfile::tempdir().unwrap();
    let now = kst(tuesday(), 10, 0, 0);
    let off = demo_day_origin(dir.path(), 0, Market::Kr, now);
    assert_eq!(off.minutes, 0);
    save_demo_day(dir.path(), &off).unwrap();
    assert!(is_demo_home(dir.path()));
}
