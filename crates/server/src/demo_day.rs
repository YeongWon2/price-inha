//! (결정 36) 체험판의 짧은 하루(뼈대).

use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::Market;

/// 체험 폴더 표시 · 짧은 하루 시작점을 두는 파일(체험 보관 폴더 안). 비밀이 아니다.
pub const DEMO_HOME_FILE: &str = "demo.json";
/// 짧은 하루의 기본 길이(분).
pub const DEMO_DAY_MINUTES_DEFAULT: u32 = 20;
/// 짧은 하루의 가장 짧은 길이(분).
pub const DEMO_DAY_MINUTES_MIN: u32 = 8;

/// 짧은 하루의 시작점.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemoDayOrigin {
    /// 시작한 실제 시각.
    pub real: DateTime<Utc>,
    /// 그때의 체험 날짜(시장 날짜).
    pub date: NaiveDate,
    /// 하루 길이(분). 0이면 짧은 하루를 끈다.
    pub minutes: u32,
}

/// 실제 시각의 체험 시각.
pub fn demo_day_time(
    _origin: &DemoDayOrigin,
    _market: Market,
    _real: DateTime<Utc>,
) -> DateTime<Utc> {
    todo!()
}

/// 체험 폴더의 시작점을 읽는다(없으면 새로, 길이가 다르면 다음 거래일부터 새 길이로). 쓰지 않는다.
pub fn demo_day_origin(
    _home: &Path,
    _minutes: u32,
    _market: Market,
    _now: DateTime<Utc>,
) -> DemoDayOrigin {
    todo!()
}

/// 시작점을 체험 폴더에 남긴다.
pub fn save_demo_day(_home: &Path, _origin: &DemoDayOrigin) -> std::io::Result<()> {
    todo!()
}

/// `price-inha demo`가 만든 체험 폴더인가.
pub fn is_demo_home(_home: &Path) -> bool {
    todo!()
}
