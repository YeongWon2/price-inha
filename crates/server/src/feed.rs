//! 시세 흐름: 받은 시세를 지나가는 이벤트로 보내고, 늦은 시세(N6)와 시세 상태(V31)를 알린다.
//!
//! 증권사 요청이 실패하면 다시 시도하는 간격을 늘린다(물러서기). 같은 실패가 이어지는 동안에는
//! 시세 상태를 다시 알리지 않고, 기록도 상태가 바뀔 때와 [`LOG_EVERY`]마다만 남긴다.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use pi_gate::Publisher;
use pi_market::{
    Clock, CredentialSource, Currency, Market, MarketAdapter, MarketCalendar, MarketError, Quote,
    StalePolicy, Symbol,
};
use serde_json::{Value, json};
use tokio::time::Instant;

use crate::retry::{Backoff, LOG_EVERY, LogGate};
use crate::{
    FEED_BROKER_REJECTED, FEED_BROKER_UNREACHABLE, FEED_CONNECTING, FEED_DEMO, FEED_LIVE,
    FEED_NO_BROKER_KEY, KIND_FEED_STATUS, KIND_QUOTES, KIND_QUOTES_STALE,
};

/// 증권사 키가 없을 때 키가 들어왔는지 다시 보는 간격(키 파일만 읽는다. 바깥으로 나가지 않는다).
const KEY_POLL: Duration = Duration::from_secs(2);
/// 달력을 받지 못했을 때 다시 받기까지 기다리는 시간.
const CALENDAR_RETRY: Duration = Duration::from_secs(30);
/// 다음 받기를 기다리는 동안 늦은 시세를 다시 보는 간격(물러서 있는 동안에도 늦음을 알린다).
const STALE_CHECK: Duration = Duration::from_secs(1);
/// 가장 짧은 시세 간격(0이 와도 쉬지 않고 돌지 않게).
const MIN_INTERVAL: Duration = Duration::from_millis(1);

/// 시세를 어디서 받는가.
pub(crate) enum Source {
    /// 시연용 가짜 어댑터.
    Demo(Arc<dyn MarketAdapter>),
    /// 증권사. 키가 등록돼 있을 때만 받는다.
    Broker {
        adapter: Arc<dyn MarketAdapter>,
        keys: Arc<dyn CredentialSource>,
    },
}

/// 시세를 받는 박자.
pub(crate) struct Pace {
    /// 시세를 받는 간격.
    pub(crate) interval: Duration,
    /// 실패한 뒤 다시 시도하기까지의 첫 간격.
    pub(crate) retry_initial: Duration,
    /// 다시 시도하는 간격의 상한.
    pub(crate) retry_max: Duration,
}

/// 마지막으로 알린 "시세 늦음".
#[derive(Default, PartialEq, Eq)]
struct Late {
    symbols: Vec<String>,
    /// 장 시간을 모르는 채로 판단했는가.
    hours_unknown: bool,
}

/// 시세 흐름 하나.
pub(crate) struct Feed {
    publisher: Publisher,
    clock: Arc<dyn Clock>,
    market: Market,
    symbols: Vec<Symbol>,
    pace: Pace,
    /// 종목마다 가장 최근에 받은 시세.
    held: HashMap<Symbol, Quote>,
    /// 달력과 그 기준일.
    calendar: Option<(NaiveDate, MarketCalendar)>,
    calendar_retry_at: Option<Instant>,
    late: Late,
    /// 마지막으로 알린 시세 상태. 같은 상태는 다시 알리지 않는다.
    status: Option<&'static str>,
    /// 시세 받기 실패의 기록 묶기.
    quotes_log: LogGate,
    /// 달력 받기 실패의 기록 묶기.
    calendar_log: LogGate,
}

impl Feed {
    pub(crate) fn new(
        publisher: Publisher,
        clock: Arc<dyn Clock>,
        market: Market,
        symbols: Vec<Symbol>,
        pace: Pace,
    ) -> Self {
        Self {
            publisher,
            clock,
            market,
            symbols,
            pace,
            held: HashMap::new(),
            calendar: None,
            calendar_retry_at: None,
            late: Late::default(),
            status: None,
            quotes_log: LogGate::new(LOG_EVERY),
            calendar_log: LogGate::new(LOG_EVERY),
        }
    }

    /// 시세 상태를 알린다. 마지막으로 알린 것과 같으면 다시 알리지 않는다.
    fn status(&mut self, state: &'static str) {
        if self.status != Some(state) {
            self.status = Some(state);
            self.publisher
                .publish_transient(KIND_FEED_STATUS, json!({ "state": state }));
        }
    }

    /// 끝없이 돈다. 멈추려면 이 future를 버린다.
    pub(crate) async fn run(mut self, source: Source) {
        match source {
            Source::Demo(adapter) => loop {
                self.status(FEED_DEMO);
                self.pump(&adapter, true).await;
                // 가짜 어댑터는 키를 묻지 않는다. 그래도 돌아오면 쉬었다가 다시 돈다.
                tokio::time::sleep(KEY_POLL).await;
            },
            Source::Broker { adapter, keys } => loop {
                // 키가 없으면 증권사로 요청을 보내지 않는다(V31). 키 확인(토큰 발급)도 하지 않는다.
                if keys.credentials().is_none() {
                    self.status(FEED_NO_BROKER_KEY);
                    while keys.credentials().is_none() {
                        tokio::time::sleep(KEY_POLL).await;
                    }
                }
                // 시세를 실제로 받기 전에는 "증권사 시세"라고 하지 않는다.
                self.status(FEED_CONNECTING);
                self.pump(&adapter, false).await;
                // 어댑터가 키가 없다고 했다. 곧바로 다시 돌지 않는다(키가 사라졌으면 위에서 알린다).
                tokio::time::sleep(KEY_POLL).await;
            },
        }
    }

    /// 받아 보내기를 되풀이한다. 성공하면 시세 간격만큼, 실패하면 물러서며 기다린다.
    /// 어댑터가 증권사 키가 없다고 하면 돌아온다.
    async fn pump(&mut self, adapter: &Arc<dyn MarketAdapter>, demo: bool) {
        let mut backoff = Backoff::new(self.pace.retry_initial, self.pace.retry_max);
        loop {
            let wait = match adapter.quotes(&self.symbols).await {
                Ok(quotes) => {
                    backoff.reset();
                    self.received(quotes, demo);
                    // 달력은 증권사에 닿는 동안에만 받는다(실패 중에 요청을 더 보내지 않는다).
                    self.refresh_calendar(adapter).await;
                    self.pace.interval.max(MIN_INTERVAL)
                }
                Err(MarketError::NoCredentials) => return,
                Err(error) => {
                    let wait = backoff.next_delay();
                    self.failed(&error, demo, wait);
                    wait
                }
            };
            self.rest(wait).await;
        }
    }

    /// 시세를 받았다: 간직하고 화면에 보낸다. 증권사 시세는 이때에야 "증권사 시세"다.
    fn received(&mut self, quotes: Vec<Quote>, demo: bool) {
        if self.quotes_log.clear() {
            tracing::info!("시세를 다시 받습니다");
        }
        if quotes.is_empty() {
            // 받은 것이 없으면(지켜볼 종목이 없음 등) 시세를 받았다고 하지 않는다.
            return;
        }
        for quote in quotes {
            self.held.insert(quote.symbol.clone(), quote);
        }
        if !demo {
            self.status(FEED_LIVE);
        }
        self.publisher
            .publish_transient(KIND_QUOTES, self.quotes_payload(demo));
    }

    /// 시세를 받지 못했다: 까닭을 알리고(바뀌었을 때만), 기록은 묶어서 남긴다.
    fn failed(&mut self, error: &MarketError, demo: bool, retry_in: Duration) {
        let state = failure_state(error);
        if !demo {
            self.status(state);
        }
        if self.quotes_log.should_log(state, std::time::Instant::now()) {
            tracing::warn!(
                %error,
                retry_in_ms = u64::try_from(retry_in.as_millis()).unwrap_or(u64::MAX),
                "시세를 받지 못했습니다. 간격을 늘려 다시 시도합니다(같은 실패는 1분에 한 번만 기록합니다)"
            );
        }
    }

    /// `wait`만큼 쉰다. 쉬는 동안에도 늦은 시세를 본다.
    async fn rest(&mut self, wait: Duration) {
        let until = Instant::now() + wait;
        loop {
            self.announce_stale();
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            tokio::time::sleep(left.min(STALE_CHECK)).await;
        }
    }

    /// 가진 시세 전부(지켜볼 종목 순서).
    fn quotes_payload(&self, demo: bool) -> Value {
        let quotes: Vec<Value> = self
            .symbols
            .iter()
            .filter_map(|symbol| self.held.get(symbol))
            .map(|quote| {
                json!({
                    "symbol": quote.symbol.as_str(),
                    "price": quote.price.to_string(),
                    "currency": currency_code(&quote.currency),
                    "received_at": quote.received_at.timestamp_millis(),
                })
            })
            .collect();
        json!({ "demo": demo, "quotes": quotes })
    }

    /// 늦은 종목이 바뀌었으면 알린다(N6). 다시 받아 늦은 종목이 없어지면 빈 목록을 알린다.
    ///
    /// 장 시간을 모르면(달력을 한 번도 못 받았거나 가진 달력이 지금을 덮지 않음) "장 닫힘"으로
    /// 보지 않는다: 30초 넘게 오지 않은 시세를 늦은 것으로 보고 `hours_unknown`을 참으로 알린다.
    fn announce_stale(&mut self) {
        let now = self.clock.now();
        let policy = StalePolicy::default();
        let calendar = self
            .calendar
            .as_ref()
            .map(|(_, calendar)| calendar)
            .filter(|calendar| covers(calendar, self.market, now));
        let symbols: Vec<String> = self
            .symbols
            .iter()
            .filter_map(|symbol| self.held.get(symbol))
            .filter(|quote| match calendar {
                Some(calendar) => pi_market::is_stale(quote, calendar, now, &policy),
                None => older_than(quote, now, policy.max_age),
            })
            .map(|quote| quote.symbol.as_str().to_owned())
            .collect();
        let late = Late {
            symbols,
            hours_unknown: calendar.is_none(),
        };
        // 늦은 종목이 없는 동안에는 장 시간을 아는지가 바뀌어도 알릴 것이 없다.
        let changed = late.symbols != self.late.symbols
            || (!late.symbols.is_empty() && late.hours_unknown != self.late.hours_unknown);
        if changed {
            self.publisher.publish_transient(
                KIND_QUOTES_STALE,
                json!({ "symbols": late.symbols, "hours_unknown": late.hours_unknown }),
            );
        }
        self.late = late;
    }

    /// 오늘(시장 날짜)의 달력을 갖춘다. 날짜가 바뀌면 다시 받는다. 받지 못하면 잠시 뒤에 다시 받는다.
    async fn refresh_calendar(&mut self, adapter: &Arc<dyn MarketAdapter>) {
        let today = market_date(self.market, self.clock.now());
        let fresh = matches!(&self.calendar, Some((date, _)) if *date == today);
        let waiting = self.calendar_retry_at.is_some_and(|at| Instant::now() < at);
        if !fresh && !waiting {
            match adapter.calendar(today).await {
                Ok(calendar) => {
                    self.calendar = Some((today, calendar));
                    self.calendar_retry_at = None;
                    self.calendar_log.clear();
                }
                Err(error) => {
                    if self
                        .calendar_log
                        .should_log("calendar", std::time::Instant::now())
                    {
                        tracing::warn!(%error, "장 달력을 받지 못했습니다(같은 실패는 1분에 한 번만 기록합니다)");
                    }
                    self.calendar_retry_at = Some(Instant::now() + CALENDAR_RETRY);
                }
            }
        }
    }
}

/// 실패를 화면에 알릴 시세 상태로 옮긴다.
///
/// 인증 실패 · 허용 IP 아님은 "증권사가 키를 받지 않음", 그 밖(연결 실패 · 점검 · 장애 · 한도 ·
/// 읽을 수 없는 응답 · 그 밖의 거절)은 "증권사에 닿지 않음"이다.
fn failure_state(error: &MarketError) -> &'static str {
    match error {
        MarketError::AuthFailed { .. } | MarketError::IpNotAllowed => FEED_BROKER_REJECTED,
        _ => FEED_BROKER_UNREACHABLE,
    }
}

/// 가진 달력이 지금 시각을 덮는가: 지금의 시장 날짜가 달력의 이전 영업일과 다음 영업일 사이에 있다.
/// (그 사이의 날은 달력이 세션을 알거나, 영업일이 아니라고 알려 준 날이다.)
fn covers(calendar: &MarketCalendar, market: Market, now: DateTime<Utc>) -> bool {
    let today = market_date(market, now);
    calendar.previous_business_day.date <= today && today <= calendar.next_business_day.date
}

/// 받은 지 `max_age`를 넘었는가(딱 그 시간이면 아직 아니다). 장 시간은 보지 않는다.
/// 받은 시각이 지금보다 뒤면(시계가 어긋남) 나이를 잴 수 없으니 넘었다고 하지 않는다.
fn older_than(quote: &Quote, now: DateTime<Utc>, max_age: Duration) -> bool {
    now.signed_duration_since(quote.received_at)
        .to_std()
        .is_ok_and(|age| age > max_age)
}

/// 그 시장의 오늘 날짜. 달력에는 앞뒤 영업일도 들어 있어 하루쯤 어긋나도 세션을 찾는다.
fn market_date(market: Market, now: DateTime<Utc>) -> NaiveDate {
    let offset_hours = match market {
        Market::Us => -5,
        _ => 9,
    };
    now.checked_add_signed(TimeDelta::hours(offset_hours))
        .unwrap_or(now)
        .date_naive()
}

fn currency_code(currency: &Currency) -> &str {
    match currency {
        Currency::Krw => "KRW",
        Currency::Usd => "USD",
        Currency::Other(code) => code,
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn 시장_날짜는_그_시장의_시각으로_정한다() {
        let late_utc = Utc.with_ymd_and_hms(2026, 3, 24, 16, 0, 0).single();
        let Some(now) = late_utc else {
            return;
        };
        // 한국은 이미 25일 01시, 미국 동부는 아직 24일 11시.
        assert_eq!(
            Some(market_date(Market::Kr, now)),
            NaiveDate::from_ymd_opt(2026, 3, 25)
        );
        assert_eq!(
            Some(market_date(Market::Us, now)),
            NaiveDate::from_ymd_opt(2026, 3, 24)
        );
    }

    fn day(date: NaiveDate) -> pi_market::TradingDay {
        pi_market::TradingDay {
            date,
            sessions: Vec::new(),
        }
    }

    #[test]
    fn 달력은_이전_영업일부터_다음_영업일까지만_덮는다() {
        let ymd = |d| NaiveDate::from_ymd_opt(2026, 3, d);
        let (Some(prev), Some(today), Some(next)) = (ymd(26), ymd(27), ymd(30)) else {
            return;
        };
        // 금요일(27일)의 달력: 다음 영업일은 월요일(30일)이다.
        let calendar = MarketCalendar {
            market: Market::Kr,
            today: day(today),
            previous_business_day: day(prev),
            next_business_day: day(next),
        };
        let at = |d, h| Utc.with_ymd_and_hms(2026, 3, d, h, 0, 0).single();
        let cases = [
            (at(25, 3), false),
            (at(26, 3), true),
            // 주말은 달력이 "영업일이 아니다"라고 알려 준 날이다.
            (at(28, 3), true),
            (at(30, 3), true),
            // 한국은 이미 31일이다.
            (at(30, 16), false),
        ];
        for (now, expected) in cases {
            let Some(now) = now else { return };
            assert_eq!(covers(&calendar, Market::Kr, now), expected, "{now}");
        }
    }

    #[test]
    fn 실패는_키를_받지_않음과_닿지_않음으로_나눈다() {
        let rejected = [
            MarketError::AuthFailed {
                code: "invalid_client".to_owned(),
            },
            MarketError::IpNotAllowed,
        ];
        for error in rejected {
            assert_eq!(failure_state(&error), FEED_BROKER_REJECTED, "{error}");
        }
        let unreachable = [
            MarketError::Connection {
                detail: "refused".to_owned(),
            },
            MarketError::Unavailable {
                status: 503,
                code: None,
            },
            MarketError::RateLimited { retry_after: None },
        ];
        for error in unreachable {
            assert_eq!(failure_state(&error), FEED_BROKER_UNREACHABLE, "{error}");
        }
    }

    #[test]
    fn 통화는_코드_글자로_적는다() {
        assert_eq!(currency_code(&Currency::Krw), "KRW");
        assert_eq!(currency_code(&Currency::Usd), "USD");
        assert_eq!(currency_code(&Currency::Other("JPY".to_owned())), "JPY");
    }
}
