//! 시연용 시세: 가짜 어댑터에 받을 때마다 조금씩 바뀌는 값을 넣는다. 바깥으로 요청을 보내지 않는다.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeDelta, Utc, Weekday};
use pi_market::{
    Clock, Currency, FakeAdapter, Market, MarketAdapter, MarketCalendar, MarketError, Quote,
    Session, SessionKind, Symbol, TradingDay,
};
use rust_decimal::Decimal;

/// 시연용 어댑터. 기본은 달력을 정하지 않으므로(휴장) 시세 늦음을 알리지 않는다. (24차) 시연 달력을 켜면
/// ([`DemoAdapter::with_calendar`]) [`demo_calendar`]를 낸다.
pub(crate) struct DemoAdapter {
    fake: FakeAdapter,
    market: Market,
    clock: Arc<dyn Clock>,
    /// 종목마다 지금 값(통화의 가장 작은 단위).
    minor: Mutex<HashMap<Symbol, i64>>,
    ticks: AtomicU64,
    /// (24차) 시연 달력을 내는가.
    calendar: bool,
}

/// 가격 모양: 한 걸음의 크기 · 처음 값의 범위(걸음 수) · 소수 자리.
struct Shape {
    currency: Currency,
    step: i64,
    floor_steps: i64,
    span_steps: u64,
    scale: u32,
}

impl DemoAdapter {
    pub(crate) fn new(market: Market, clock: Arc<dyn Clock>) -> Self {
        Self {
            fake: FakeAdapter::new(market),
            market,
            clock,
            minor: Mutex::new(HashMap::new()),
            ticks: AtomicU64::new(0),
            calendar: false,
        }
    }

    /// (24차 1번) `on`이면 시연 달력([`demo_calendar`])을 낸다.
    pub(crate) fn with_calendar(mut self, on: bool) -> Self {
        self.calendar = on;
        self
    }

    fn shape(&self) -> Shape {
        match self.market {
            // 달러: 1센트씩, 50.00 ~ 499.99에서 시작.
            Market::Us => Shape {
                currency: Currency::Usd,
                step: 1,
                floor_steps: 5_000,
                span_steps: 45_000,
                scale: 2,
            },
            // 원: 100원씩, 10,000 ~ 99,900에서 시작.
            _ => Shape {
                currency: Currency::Krw,
                step: 100,
                floor_steps: 100,
                span_steps: 900,
                scale: 0,
            },
        }
    }

    /// 그 종목의 다음 값. 받을 때마다 반드시 바뀐다.
    fn next_price(&self, symbol: &Symbol, tick: u64, shape: &Shape) -> Decimal {
        let seed = fnv1a(symbol.as_str().as_bytes());
        let mut minor = self.minor.lock().unwrap_or_else(PoisonError::into_inner);
        let floor = shape.floor_steps * shape.step;
        let start = floor + i64::try_from(seed % shape.span_steps).unwrap_or(0) * shape.step;
        let value = minor.entry(symbol.clone()).or_insert(start);
        // -3 ~ +3 걸음. 0이면 한 걸음 올린다.
        let roll = i64::try_from(mix(seed ^ tick) % 7).unwrap_or(0) - 3;
        let steps = if roll == 0 { 1 } else { roll };
        let moved = value.saturating_add(steps * shape.step);
        // 바닥 아래로 내려가려 하면 반대로 움직인다(값은 늘 바뀐다).
        *value = if moved < floor {
            value.saturating_add(steps.abs() * shape.step)
        } else {
            moved
        };
        Decimal::new(*value, shape.scale)
    }
}

/// (7차 3번) 시연에 지켜볼 종목이 없을 때 쓰는 시연 종목 하나. 시연(가짜 시세)일 때만 쓰고
/// 저장된 종목 목록은 바꾸지 않는다(등록된 종목도 아니다).
pub(crate) fn demo_symbol(market: Market) -> Symbol {
    match market {
        Market::Us => Symbol::new("AAPL"),
        _ => Symbol::new("005930"),
    }
}

/// (24차 3번) 시연용 가짜 공시를 내는 간격의 기본값(실행 파일의 `--demo` 조립, 3분).
pub const DEMO_DISCLOSURE_EVERY: Duration = Duration::from_secs(180);

/// (25차 3번) 실행 파일의 `--demo` 조립에서 시연용 가짜 AI가 답마다 두는 뜸의 기본값(2.5초).
/// [`crate::RunAssembly::demo_ai_pause`]가 이 값으로 시작하고 시험은 그 칸을 줄인다.
pub const DEMO_AI_PAUSE: Duration = Duration::from_millis(2_500);

/// (24차 3번) 시연용 가짜 공시의 제목 · 본문에 반드시 들어가는 말. 실제 출처 주소는 쓰지 않는다.
pub const DEMO_DISCLOSURE_MARK: &str = "시연";

/// (24차, D14 · N19 · E56) 시연 모드의 조립 값. 기본(`Default`)은 모두 꺼짐이다 — 시연 어댑터의 지금 동작
/// (달력 없음 · 가짜 공시 없음)이고, 기존 시험이 쓰는 시연 조립([`crate::App::launch_with`] 등)은 이것을 쓴다.
/// 실행 파일의 `--demo` 조립([`crate::run_assembly`])만 [`DemoWiring::binary`]로 켠다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DemoWiring {
    /// 시연 달력([`demo_calendar`])을 켠다: 시연 어댑터가 달력을 내고 깨우기 엔진 · 주문 길이 그것을 쓴다.
    pub calendar: bool,
    /// 시연용 가짜 공시 간격(시계 기준). `None`이면 내지 않는다. 지켜보는 종목을 돌아가며 한 건씩 낸다.
    pub disclosure_every: Option<Duration>,
}

impl DemoWiring {
    /// 실행 파일의 `--demo` 조립: 시연 달력 켬 · 가짜 공시 [`DEMO_DISCLOSURE_EVERY`]마다.
    pub fn binary() -> DemoWiring {
        DemoWiring {
            calendar: true,
            disclosure_every: Some(DEMO_DISCLOSURE_EVERY),
        }
    }
}

/// (24차 1번, N19 · C33) 시연 달력: 그 날짜(시장 시간대의 날짜)의 달력.
///
/// 매일이 거래일이다(주말 · 공휴일 없음). 정규장은 시장 시간대(국내 한국 시각, 미국 뉴욕 시각)의
/// 00:05 이상 23:55 미만 하나다. 그래서 자정 앞뒤 10분은 장 밖이다. 앞 · 뒤 거래일은 하루 전 · 하루 뒤다.
/// 시연 어댑터 안에서만 쓴다(실제 모드는 시장 어댑터 달력만 쓴다, E56).
pub fn demo_calendar(market: Market, date: NaiveDate) -> MarketCalendar {
    MarketCalendar {
        market,
        today: demo_day(market, date),
        previous_business_day: demo_day(market, date.pred_opt().unwrap_or(date)),
        next_business_day: demo_day(market, date.succ_opt().unwrap_or(date)),
    }
}

/// 시연 정규장이 여는 시장 시각(00:05)과 닫는 시장 시각(23:55), 분.
const DEMO_OPEN_MINUTE: u32 = 5;
const DEMO_CLOSE_MINUTE: u32 = 23 * 60 + 55;

/// 시연 달력의 하루: 그 시장 시각 00:05 이상 23:55 미만 정규장 하나(시각을 셀 수 없으면 세션 없음).
fn demo_day(market: Market, date: NaiveDate) -> TradingDay {
    let start = market_time(market, date, DEMO_OPEN_MINUTE);
    let end = market_time(market, date, DEMO_CLOSE_MINUTE);
    let sessions = match (start, end) {
        (Some(start), Some(end)) => vec![Session {
            kind: SessionKind::Regular,
            start,
            end,
        }],
        _ => Vec::new(),
    };
    TradingDay { date, sessions }
}

/// 그 시장 날짜 · 시각(자정부터 센 분)의 UTC 시각. 국내는 UTC+9, 미국은 미 동부 시각(서머타임이면 UTC-4,
/// 아니면 UTC-5)이다.
fn market_time(market: Market, date: NaiveDate, minute: u32) -> Option<DateTime<Utc>> {
    let offset_hours: i64 = match market {
        Market::Us if us_eastern_dst(date, minute) => -4,
        Market::Us => -5,
        _ => 9,
    };
    let local = date.and_hms_opt(minute / 60, minute % 60, 0)?;
    let utc = local.checked_sub_signed(TimeDelta::try_hours(offset_hours)?)?;
    Some(utc.and_utc())
}

/// 미 동부의 그 날 · 그 시각(자정부터 센 분)이 서머타임인가(chrono-tz 없이 손으로 센다, `pi_market`과 같은 규칙).
///
/// 서머타임은 3월 둘째 일요일 02:00부터 11월 첫째 일요일 02:00(서머타임 시각)까지다.
fn us_eastern_dst(date: NaiveDate, minute: u32) -> bool {
    let year = date.year();
    let (Some(begin), Some(end)) = (
        NaiveDate::from_weekday_of_month_opt(year, 3, Weekday::Sun, 2),
        NaiveDate::from_weekday_of_month_opt(year, 11, Weekday::Sun, 1),
    ) else {
        return false;
    };
    let after_two = minute >= 2 * 60;
    let started = date > begin || (date == begin && after_two);
    let ended = date > end || (date == end && after_two);
    started && !ended
}

/// FNV-1a. 종목마다 다른 시작 값을 정하는 데만 쓴다.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// splitmix64의 섞기. 시연용 값의 오르내림을 정하는 데만 쓴다(보안과 무관).
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[async_trait]
impl MarketAdapter for DemoAdapter {
    fn market(&self) -> Market {
        self.market
    }

    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
        let tick = self.ticks.fetch_add(1, Ordering::Relaxed);
        let now = self.clock.now();
        let shape = self.shape();
        for symbol in symbols {
            self.fake.set_quote(Quote {
                symbol: symbol.clone(),
                price: self.next_price(symbol, tick, &shape),
                currency: shape.currency.clone(),
                exchange_time: Some(now),
                received_at: now,
            });
        }
        self.fake.quotes(symbols).await
    }

    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError> {
        if self.calendar {
            return Ok(demo_calendar(self.market, date));
        }
        self.fake.calendar(date).await
    }
}

#[cfg(test)]
mod tests {
    use pi_market::SystemClock;

    use super::*;

    async fn prices(adapter: &DemoAdapter, symbols: &[Symbol]) -> Vec<Decimal> {
        let quotes = adapter.quotes(symbols).await.unwrap_or_default();
        quotes.into_iter().map(|quote| quote.price).collect()
    }

    #[tokio::test]
    async fn 받을_때마다_값이_바뀌고_바닥_아래로_내려가지_않는다() {
        let adapter = DemoAdapter::new(Market::Kr, Arc::new(SystemClock));
        let symbols = [Symbol::new("005930"), Symbol::new("000660")];
        let mut last = prices(&adapter, &symbols).await;
        assert_eq!(last.len(), 2);
        for _ in 0..2_000 {
            let now = prices(&adapter, &symbols).await;
            for (before, after) in last.iter().zip(&now) {
                assert_ne!(before, after, "받을 때마다 바뀐다");
                assert!(*after >= Decimal::from(10_000), "{after}");
                assert_eq!(after.scale(), 0, "원화는 소수 자리가 없다");
            }
            last = now;
        }
    }

    #[test]
    fn 미국_시연_달력은_서머타임이면_utc_4로_센다() {
        let summer = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap_or_default();
        let day = demo_calendar(Market::Us, summer).today;
        let start = day.sessions.first().map(|session| session.start);
        let end = day.sessions.first().map(|session| session.end);
        // 뉴욕 00:05(UTC-4) = 04:05 UTC, 23:55 = 다음 날 03:55 UTC.
        assert_eq!(
            start.map(|at| at.to_rfc3339()),
            Some("2026-07-15T04:05:00+00:00".to_string())
        );
        assert_eq!(
            end.map(|at| at.to_rfc3339()),
            Some("2026-07-16T03:55:00+00:00".to_string())
        );
        // 바뀌는 날: 3월 둘째 일요일(2026-03-08)은 00:05가 표준시, 23:55가 서머타임이다.
        let switch = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap_or_default();
        let day = demo_calendar(Market::Us, switch).today;
        let hours = day
            .sessions
            .first()
            .map(|session| (session.end - session.start).num_minutes());
        assert_eq!(
            hours,
            Some(23 * 60 + 50 - 60),
            "서머타임 시작 날은 한 시간 짧다"
        );
    }

    #[test]
    fn 시연_어댑터는_켰을_때만_달력을_낸다() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap_or_default();
        let off = DemoAdapter::new(Market::Kr, Arc::new(SystemClock));
        let on = DemoAdapter::new(Market::Kr, Arc::new(SystemClock)).with_calendar(true);
        let runtime = tokio::runtime::Builder::new_current_thread().build();
        let Ok(runtime) = runtime else { return };
        let off = runtime
            .block_on(off.calendar(date))
            .map(|c| c.today.sessions.len());
        let on = runtime
            .block_on(on.calendar(date))
            .map(|c| c.today.sessions.len());
        assert_eq!(off.ok(), Some(0), "기본은 달력 없음(휴장)");
        assert_eq!(on.ok(), Some(1), "켜면 시연 정규장 하나");
    }

    #[tokio::test]
    async fn 미국_시장은_달러와_센트로_준다() {
        let adapter = DemoAdapter::new(Market::Us, Arc::new(SystemClock));
        let quotes = adapter
            .quotes(&[Symbol::new("AAPL")])
            .await
            .unwrap_or_default();
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].currency, Currency::Usd);
        assert_eq!(quotes[0].price.scale(), 2);
        assert!(quotes[0].price >= Decimal::new(4_997, 2));
    }
}
