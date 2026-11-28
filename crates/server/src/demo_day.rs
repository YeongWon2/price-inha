//! (결정 36) 체험판의 짧은 하루: 실제 몇 분(기본 20분)이 시연 달력([`crate::demo_calendar`], N19)의 한 거래일이다.
//!
//! 시연 달력은 그대로 두고 **체험 시계**([`DemoDayClock`])가 시각을 옮긴다. 한 바퀴(`minutes`분)는
//!
//! | 실제 | 체험 시각(시장 시각) | 일 |
//! |---|---|---|
//! | 0 ~ 1분 | 00:04 ~ 00:05 | 장 시작 전 |
//! | 1 ~ (`minutes` − 4)분 | 00:05 ~ | 장중(기본 15분). 실제와 같은 빠르기로 흐른다 |
//! | (`minutes` − 4)분 | 23:55로 건너뜀 | 장 마감 |
//! | 마지막 4분 | 23:55 ~ 23:59 | 마감 뒤 일(하루 정리 · 복기 · 결정 메모 정산 · 팀 지침) |
//! | `minutes`분 | 다음 날 00:04 | 다음 거래일 |
//!
//! 장중과 마감 뒤는 실제와 같은 빠르기라 주문 · 승인 기다림 · 체결이 실제판과 같은 시간으로 돈다. 건너뛰는 것은
//! 장중 뒷부분과 밤뿐이다. 시작점은 체험 보관 폴더의 [`DEMO_HOME_FILE`]에 남겨, 다시 켜도 시각이 뒤로 가지 않는다
//! (꺼 둔 동안에도 날이 흐른다). 이 파일이 있는 폴더가 `price-inha demo`의 체험 폴더다([`is_demo_home`]).
//! 실제판(시연 아님)과 `run --demo`는 이 시계를 쓰지 않는다.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{Clock, Market};
use serde_json::{Value, json};

/// 체험 폴더 표시 · 짧은 하루 시작점을 두는 파일(체험 보관 폴더 안). 비밀이 아니다.
pub const DEMO_HOME_FILE: &str = "demo.json";
/// 짧은 하루의 기본 길이(분).
pub const DEMO_DAY_MINUTES_DEFAULT: u32 = 20;
/// 짧은 하루의 가장 짧은 길이(분): 장 전 1분 · 장중 3분 · 마감 뒤 4분.
pub const DEMO_DAY_MINUTES_MIN: u32 = 8;

/// 한 바퀴의 마감 뒤(분).
const AFTER_MINUTES: i64 = 4;
/// 체험 하루가 시작하는 시장 시각(00:04, 자정부터 센 분).
const DAY_START_MINUTE: u32 = 4;
/// 장 마감 시장 시각(23:55, 시연 달력의 끝).
const CLOSE_MINUTE: u32 = 23 * 60 + 55;
/// 체험 시계가 한 번에 자는 가장 긴 시간(건너뛰기를 놓치지 않게).
const SLEEP_SLICE: Duration = Duration::from_millis(500);

/// 짧은 하루의 시작점.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemoDayOrigin {
    /// 시작한 실제 시각.
    pub real: DateTime<Utc>,
    /// 그때의 체험 날짜(시장 날짜).
    pub date: NaiveDate,
    /// 하루 길이(분). 0이면 짧은 하루를 끈다(체험 폴더 표시만).
    pub minutes: u32,
}

impl DemoDayOrigin {
    fn to_json(self) -> Value {
        json!({
            "real_ms": self.real.timestamp_millis(),
            "date": self.date.format("%Y-%m-%d").to_string(),
            "minutes": self.minutes,
        })
    }

    fn from_json(value: &Value) -> Option<DemoDayOrigin> {
        let real = Utc
            .timestamp_millis_opt(value["real_ms"].as_i64()?)
            .single()?;
        let date = NaiveDate::parse_from_str(value["date"].as_str()?, "%Y-%m-%d").ok()?;
        let minutes = u32::try_from(value["minutes"].as_u64()?).ok()?;
        Some(DemoDayOrigin {
            real,
            date,
            minutes,
        })
    }
}

/// 실제 시각의 체험 시각(모듈 문서의 표). 짧은 하루가 꺼져 있으면(`minutes` 0) 실제 시각 그대로다.
/// 시작점보다 앞선 실제 시각은 시작점으로 본다.
pub fn demo_day_time(origin: &DemoDayOrigin, market: Market, real: DateTime<Utc>) -> DateTime<Utc> {
    if origin.minutes == 0 {
        return real;
    }
    let cycle = i64::from(origin.minutes) * 60_000;
    let elapsed = (real - origin.real).num_milliseconds().max(0);
    let day = origin.date + chrono::Duration::days(elapsed / cycle);
    let into = elapsed % cycle;
    let open_part = cycle - AFTER_MINUTES * 60_000;
    let (base, offset) = if into < open_part {
        (DAY_START_MINUTE, into)
    } else {
        (CLOSE_MINUTE, into - open_part)
    };
    crate::demo::market_time(market, day, base)
        .map_or(real, |at| at + chrono::Duration::milliseconds(offset))
}

/// 체험 폴더의 시작점을 읽는다. 없거나 읽을 수 없으면 새로(지금 · 오늘), 길이가 다르면 지금 체험 날짜의 다음
/// 거래일부터 새 길이로 잇는다(시각이 뒤로 가지 않게). 쓰지 않는다([`save_demo_day`]).
pub fn demo_day_origin(
    home: &Path,
    minutes: u32,
    market: Market,
    now: DateTime<Utc>,
) -> DemoDayOrigin {
    let saved = std::fs::read(home.join(DEMO_HOME_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| DemoDayOrigin::from_json(&value));
    match saved {
        Some(saved) if saved.minutes == minutes => saved,
        Some(saved) => {
            let at = demo_day_time(&saved, market, now);
            let today = crate::feed::market_date(market, at);
            let date = if saved.minutes == 0 {
                today
            } else {
                today.succ_opt().unwrap_or(today)
            };
            DemoDayOrigin {
                real: now,
                date,
                minutes,
            }
        }
        None => DemoDayOrigin {
            real: now,
            date: crate::feed::market_date(market, now),
            minutes,
        },
    }
}

/// 시작점을 체험 폴더에 남긴다(폴더가 없으면 만든다).
pub fn save_demo_day(home: &Path, origin: &DemoDayOrigin) -> std::io::Result<()> {
    std::fs::create_dir_all(home)?;
    std::fs::write(home.join(DEMO_HOME_FILE), origin.to_json().to_string())
}

/// `price-inha demo`가 만든 체험 폴더인가([`DEMO_HOME_FILE`]이 있는가).
pub fn is_demo_home(home: &Path) -> bool {
    home.join(DEMO_HOME_FILE).is_file()
}

/// 체험 시계: 안쪽 시계(실제 시각)를 [`demo_day_time`]으로 옮긴다. 자는 동안 시각이 건너뛰면 그때 깬다.
pub(crate) struct DemoDayClock {
    inner: Arc<dyn Clock>,
    origin: DemoDayOrigin,
    market: Market,
}

impl DemoDayClock {
    pub(crate) fn new(inner: Arc<dyn Clock>, origin: DemoDayOrigin, market: Market) -> Self {
        Self {
            inner,
            origin,
            market,
        }
    }
}

#[async_trait]
impl Clock for DemoDayClock {
    fn now(&self) -> DateTime<Utc> {
        demo_day_time(&self.origin, self.market, self.inner.now())
    }

    async fn sleep(&self, duration: Duration) {
        let Ok(span) = chrono::Duration::from_std(duration) else {
            self.inner.sleep(duration).await;
            return;
        };
        let until = self.now() + span;
        loop {
            let left = (until - self.now()).to_std().unwrap_or(Duration::ZERO);
            if left.is_zero() {
                return;
            }
            self.inner.sleep(left.min(SLEEP_SLICE)).await;
        }
    }
}
