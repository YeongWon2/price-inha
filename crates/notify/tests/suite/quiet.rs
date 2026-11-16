//! 조용한 시간 계산. 기본값은 없다.

use pi_notify::{Error, QuietHours};
use pi_store::UnixMillis;

use crate::common::{DAY0, SEC, kst, night_quiet};

// 검증: V10 V11
#[test]
fn 자정을_넘는_시간대는_시작을_포함하고_끝을_제외한다() {
    let quiet = night_quiet();
    for (hour, minute, inside) in [
        (21, 59, false),
        (22, 0, true),
        (23, 30, true),
        (0, 0, true),
        (3, 15, true),
        (6, 59, true),
        (7, 0, false),
        (12, 0, false),
    ] {
        let at = UnixMillis(kst(hour, minute));
        assert_eq!(quiet.contains(at), inside, "{hour}:{minute}");
    }
}

// 검증: V10 V11
#[test]
fn 같은_날_안의_시간대도_센다() {
    let lunch = QuietHours::new(12 * 60, 13 * 60, 540).unwrap();
    assert!(!lunch.contains(UnixMillis(kst(11, 59))));
    assert!(lunch.contains(UnixMillis(kst(12, 0))));
    assert!(lunch.contains(UnixMillis(kst(12, 59))));
    assert!(!lunch.contains(UnixMillis(kst(13, 0))));
}

// 검증: V10 V11
#[test]
fn 시차가_다르면_같은_시각도_다르게_센다() {
    // 0시~1시. 시차 0이면 UTC 00:30이 안, 시차 +9시간이면 같은 시각이 한국 09:30이라 밖이다.
    let utc_zero = QuietHours::new(0, 60, 0).unwrap();
    let korea = QuietHours::new(0, 60, 540).unwrap();
    let at = UnixMillis(DAY0 + 30 * 60 * SEC);
    assert!(utc_zero.contains(at));
    assert!(!korea.contains(at));
}

// 검증: V10
#[test]
fn 값이_틀리면_거절한다() {
    for (start, end) in [(60, 60), (0, 0), (1440, 60), (60, 1440), (2000, 3000)] {
        assert!(
            matches!(
                QuietHours::new(start, end, 0),
                Err(Error::InvalidQuietHours)
            ),
            "{start}~{end}"
        );
    }
    assert!(QuietHours::new(0, 1439, 0).is_ok());
}
