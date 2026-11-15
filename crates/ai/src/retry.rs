//! 다시 시도할지와 얼마나 기다릴지를 정한다(A15 · A17 · A18).
//!
//! 원본(Hermes)의 동작을 따른다: 다시 시도할 수 있는 오류는 한도 초과 · 장애 · 시간 초과 ·
//! 연결 실패이고, 간격은 2의 거듭제곱으로 늘며 균등 jitter를 더한다. `Retry-After`가 있으면
//! 그 값이 우선이고, 600초에서 자른다.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::AiError;

/// 늘어나는 다시 시도 간격의 위 끝 기본값(A17, Hermes `max_delay=60`).
pub const RETRY_MAX_DELAY: Duration = Duration::from_secs(60);

/// `Retry-After`로 기다리는 시간의 위 끝(A18, Hermes `RETRY_AFTER_CAP_S`).
/// 이보다 길게 기다리라고 하면 이만큼만 기다린다.
pub const RETRY_AFTER_CAP: Duration = Duration::from_secs(600);

/// 다시 보내면 나을 수 있는 오류인가(A15).
///
/// 한도 초과 · 공급자 장애 · 시간 초과 · 연결 실패만 다시 시도한다. 인증 실패 · 잘못된 요청 ·
/// 읽지 못한 응답 · 자격 문제는 다시 보내도 같은 답이다.
pub(crate) fn is_retryable(err: &AiError) -> bool {
    matches!(
        err,
        AiError::RateLimited { .. }
            | AiError::ProviderUnavailable { .. }
            | AiError::Timeout
            | AiError::Connection
    )
}

/// 다시 시도 간격 계산기(A17).
///
/// `n`번째(1부터) 다시 시도 전 간격은 `d = min(base × 2^(n-1), max)`에
/// 균등 jitter `[0, d/2]`를 더한 값이다. 무작위는 씨앗으로 정해지므로
/// 같은 씨앗이면 같은 차례의 간격이 나온다.
#[derive(Clone, Debug)]
pub struct RetryBackoff {
    base: Duration,
    max: Duration,
    state: u64,
}

impl RetryBackoff {
    /// 첫 간격 `base`, 위 끝 `max`, 무작위 씨앗 `seed`로 만든다.
    pub fn new(base: Duration, max: Duration, seed: u64) -> Self {
        Self {
            base,
            max,
            state: seed,
        }
    }

    /// `retry`번째(1부터) 다시 시도 전에 기다릴 시간. 부를 때마다 무작위가 한 걸음 나아간다.
    pub fn delay(&mut self, retry: u32) -> Duration {
        let exponent = retry.saturating_sub(1).min(31);
        let d = self.base.saturating_mul(1u32 << exponent).min(self.max);
        d.saturating_add((d / 2).mul_f64(self.next_unit()))
    }

    /// `[0, 1)` 안의 고른 수(splitmix64).
    fn next_unit(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // 위 53비트만 쓰면 f64로 정확히 옮겨진다.
        let top = u32::try_from(z >> 43).unwrap_or(0);
        let rest = u32::try_from((z >> 11) & 0xFFFF_FFFF).unwrap_or(0);
        (f64::from(top) * 4_294_967_296.0 + f64::from(rest)) / 9_007_199_254_740_992.0
    }
}

/// `Retry-After` 값을 기다릴 시간으로 읽는다(A18).
///
/// - 초 숫자(소수 허용)와 HTTP 날짜(`Wed, 21 Oct 2015 07:28:00 GMT`) 둘 다 읽는다.
///   날짜는 `now`에서 그 시각까지 남은 시간이다.
/// - [`RETRY_AFTER_CAP`]보다 길면 [`RETRY_AFTER_CAP`]이다.
/// - 읽을 수 없는 값, 0, 이미 지난 시각은 `None`(기다릴 시간이 없는 것으로 본다).
pub fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(secs) = value.parse::<f64>() {
        return seconds_to_wait(secs);
    }
    let at = parse_http_date(value)?;
    let left = at.duration_since(now).ok()?;
    (!left.is_zero()).then(|| left.min(RETRY_AFTER_CAP))
}

/// 초 숫자를 기다릴 시간으로. 0 이하 · 무한 · 숫자 아님은 `None`.
pub(crate) fn seconds_to_wait(secs: f64) -> Option<Duration> {
    if !secs.is_finite() || secs <= 0.0 {
        return None;
    }
    let cap = RETRY_AFTER_CAP.as_secs_f64();
    Some(Duration::from_secs_f64(secs.min(cap)))
}

/// `Retry-After`(이미 [`RETRY_AFTER_CAP`]에서 잘린 값)를 실제로 기다릴 시간으로 늘린다.
/// 곱이 [`Duration`]으로 나타낼 수 없을 만큼 크면 공황하지 않고 [`Duration::MAX`]에서 멈춘다
/// (ai.md 4차 5). `tokio::time::sleep`은 그 값을 먼 미래로 다룬다.
pub(crate) fn scale_wait(unit: Duration, after: Duration) -> Duration {
    Duration::try_from_secs_f64(unit.as_secs_f64() * after.as_secs_f64()).unwrap_or(Duration::MAX)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// HTTP 날짜(IMF-fixdate, RFC 9110 5.6.7)를 읽는다. 모양이 다르면 `None`.
fn parse_http_date(value: &str) -> Option<SystemTime> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    let [weekday, day, month, year, time, zone] = parts.as_slice() else {
        return None;
    };
    let weekday = weekday.strip_suffix(',')?;
    if !WEEKDAYS.contains(&weekday) || *zone != "GMT" || day.len() != 2 || year.len() != 4 {
        return None;
    }
    let day: u32 = day.parse().ok()?;
    let month = u32::try_from(MONTHS.iter().position(|m| m == month)? + 1).ok()?;
    let year: i64 = year.parse().ok()?;
    let mut clock = time.split(':').map(|p| {
        if p.len() == 2 {
            p.parse::<u64>().ok()
        } else {
            None
        }
    });
    let (Some(Some(h)), Some(Some(m)), Some(Some(s)), None) =
        (clock.next(), clock.next(), clock.next(), clock.next())
    else {
        return None;
    };
    if !(1..=31).contains(&day) || h > 23 || m > 59 || s > 60 {
        return None;
    }
    let days = u64::try_from(days_from_civil(year, month, day)).ok()?;
    let secs = days * 86_400 + h * 3600 + m * 60 + s;
    UNIX_EPOCH.checked_add(Duration::from_secs(secs))
}

/// 그레고리력 날짜를 1970-01-01부터 센 날로 옮긴다(Howard Hinnant의 `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 한도_초과_장애_시간_초과_연결_실패만_다시_시도한다() {
        let no_retry = [
            AiError::NotConfigured,
            AiError::Auth { status: 401 },
            AiError::InvalidRequest {
                message: "no".to_string(),
            },
            AiError::InvalidResponse,
            AiError::InvalidCredential,
            AiError::InsecureBaseUrl,
        ];
        for err in no_retry {
            assert!(!is_retryable(&err), "{err:?}");
        }
        let retry = [
            AiError::RateLimited { retry_after: None },
            AiError::ProviderUnavailable { status: 529 },
            AiError::Timeout,
            AiError::Connection,
        ];
        for err in retry {
            assert!(is_retryable(&err), "{err:?}");
        }
    }

    #[test]
    fn 간격은_넘치지_않는다() {
        let mut backoff = RetryBackoff::new(Duration::MAX, Duration::MAX, 1);
        assert!(backoff.delay(u32::MAX) >= Duration::MAX / 2);
        let mut zero = RetryBackoff::new(Duration::ZERO, RETRY_MAX_DELAY, 1);
        assert_eq!(zero.delay(3), Duration::ZERO);
    }

    #[test]
    fn 무작위는_0_이상_1_미만이다() {
        let mut backoff = RetryBackoff::new(Duration::ZERO, Duration::ZERO, u64::MAX);
        for _ in 0..10_000 {
            let x = backoff.next_unit();
            assert!((0.0..1.0).contains(&x), "{x}");
        }
    }

    #[test]
    fn 날짜는_윤년과_세기를_맞게_센다() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2015, 10, 21), 16_729);
        let at = parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(at, Some(UNIX_EPOCH));
    }

    #[test]
    fn 날짜_모양이_다르면_읽지_않는다() {
        for bad in [
            "Wed, 21 Oct 2015 07:28:00 UTC",
            "Wed 21 Oct 2015 07:28:00 GMT",
            "Wed, 21 Oct 2015 7:28:00 GMT",
            "Wed, 21 Oct 2015 07:28 GMT",
            "Wed, 32 Oct 2015 07:28:00 GMT",
            "Wed, 21 Oct 2015 24:00:00 GMT",
        ] {
            assert_eq!(parse_http_date(bad), None, "{bad}");
        }
    }

    #[test]
    fn 아주_큰_단위도_공황하지_않고_끝에서_멈춘다() {
        let after = Duration::from_secs(600);
        assert_eq!(scale_wait(Duration::MAX, after), Duration::MAX);
        assert_eq!(
            scale_wait(Duration::from_secs(u64::MAX / 4), after),
            Duration::MAX
        );
        assert_eq!(
            scale_wait(Duration::from_secs(1), Duration::from_secs(2)),
            Duration::from_secs(2)
        );
    }

    #[test]
    fn 숫자가_아닌_초는_기다리지_않는다() {
        let now = SystemTime::now();
        for bad in ["-1", "inf", "NaN", "-0.5"] {
            assert_eq!(parse_retry_after(bad, now), None, "{bad}");
        }
    }
}
