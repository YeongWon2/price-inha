//! 시세 흐름: 받은 시세를 지나가는 이벤트로 보내고, 늦은 시세(N6)와 시세 상태(V31)를 알린다.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use pi_gate::Publisher;
use pi_market::{
    Clock, CredentialSource, Currency, Market, MarketAdapter, MarketCalendar, MarketError, Quote,
    StalePolicy, Symbol, WatchEvent,
};
use serde_json::{Value, json};
use tokio::time::Instant;

use crate::{
    FEED_DEMO, FEED_LIVE, FEED_NO_BROKER_KEY, KIND_FEED_STATUS, KIND_QUOTES, KIND_QUOTES_STALE,
};

/// 증권사 키가 없을 때 키가 들어왔는지 다시 보는 간격(키 파일만 읽는다. 바깥으로 나가지 않는다).
const KEY_POLL: Duration = Duration::from_secs(2);
/// 달력을 받지 못했을 때 다시 받기까지 기다리는 시간.
const CALENDAR_RETRY: Duration = Duration::from_secs(30);

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

/// 시세 흐름 하나.
pub(crate) struct Feed {
    publisher: Publisher,
    clock: Arc<dyn Clock>,
    market: Market,
    symbols: Vec<Symbol>,
    interval: Duration,
    /// 종목마다 가장 최근에 받은 시세.
    held: HashMap<Symbol, Quote>,
    /// 달력과 그 기준일.
    calendar: Option<(NaiveDate, MarketCalendar)>,
    calendar_retry_at: Option<Instant>,
    /// 마지막으로 알린 늦은 종목들.
    stale: Vec<String>,
}

/// 받기가 멈춘 까닭.
enum Stopped {
    /// 증권사 키가 사라졌다.
    NoKey,
    /// 흐름이 끝났다(받는 작업이 죽었다).
    Ended,
}

impl Feed {
    pub(crate) fn new(
        publisher: Publisher,
        clock: Arc<dyn Clock>,
        market: Market,
        symbols: Vec<Symbol>,
        interval: Duration,
    ) -> Self {
        Self {
            publisher,
            clock,
            market,
            symbols,
            interval,
            held: HashMap::new(),
            calendar: None,
            calendar_retry_at: None,
            stale: Vec::new(),
        }
    }

    fn status(&self, state: &str) {
        self.publisher
            .publish_transient(KIND_FEED_STATUS, json!({ "state": state }));
    }

    /// 끝없이 돈다. 멈추려면 이 future를 버린다.
    pub(crate) async fn run(mut self, source: Source) {
        match source {
            Source::Demo(adapter) => {
                self.status(FEED_DEMO);
                self.pump(&adapter, true).await;
            }
            Source::Broker { adapter, keys } => loop {
                // 키가 없으면 증권사로 요청을 보내지 않는다(V31). 키 확인(토큰 발급)도 하지 않는다.
                if keys.credentials().is_none() {
                    self.status(FEED_NO_BROKER_KEY);
                    while keys.credentials().is_none() {
                        tokio::time::sleep(KEY_POLL).await;
                    }
                }
                self.status(FEED_LIVE);
                if matches!(self.pump(&adapter, false).await, Stopped::Ended) {
                    tracing::error!("시세 받기가 멈췄습니다. 다시 시작합니다");
                }
                // 곧바로 다시 돌지 않는다(키가 사라졌으면 위에서 키가 없다고 알린다).
                tokio::time::sleep(KEY_POLL).await;
            },
        }
    }

    /// 간격마다 받아 보낸다. 받기에 실패해도 이어진다.
    async fn pump(&mut self, adapter: &Arc<dyn MarketAdapter>, demo: bool) -> Stopped {
        let mut events = pi_market::watch(Arc::clone(adapter), self.symbols.clone(), self.interval);
        while let Some(event) = events.recv().await {
            match event {
                WatchEvent::Quotes(quotes) => {
                    for quote in quotes {
                        self.held.insert(quote.symbol.clone(), quote);
                    }
                    self.publisher
                        .publish_transient(KIND_QUOTES, self.quotes_payload(demo));
                }
                WatchEvent::Failed(MarketError::NoCredentials) => return Stopped::NoKey,
                WatchEvent::Failed(error) => {
                    tracing::warn!(%error, "시세를 받지 못했습니다. 다음 간격에 다시 받습니다");
                }
            }
            self.announce_stale(adapter).await;
        }
        Stopped::Ended
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
    async fn announce_stale(&mut self, adapter: &Arc<dyn MarketAdapter>) {
        let now = self.clock.now();
        self.refresh_calendar(adapter, now).await;
        // 새 달력을 받지 못했으면 가진 것을 쓴다(앞뒤 영업일의 세션도 들어 있다).
        let Some((_, calendar)) = &self.calendar else {
            // 장이 열렸는지 모르면 늦었다고 하지 않는다.
            return;
        };
        let policy = StalePolicy::default();
        let stale: Vec<String> = self
            .symbols
            .iter()
            .filter_map(|symbol| self.held.get(symbol))
            .filter(|quote| pi_market::is_stale(quote, calendar, now, &policy))
            .map(|quote| quote.symbol.as_str().to_owned())
            .collect();
        if stale != self.stale {
            self.publisher
                .publish_transient(KIND_QUOTES_STALE, json!({ "symbols": stale }));
            self.stale = stale;
        }
    }

    /// 오늘(시장 날짜)의 달력을 갖춘다. 날짜가 바뀌면 다시 받는다. 받지 못하면 잠시 뒤에 다시 받는다.
    async fn refresh_calendar(&mut self, adapter: &Arc<dyn MarketAdapter>, now: DateTime<Utc>) {
        let today = market_date(self.market, now);
        let fresh = matches!(&self.calendar, Some((date, _)) if *date == today);
        let waiting = self.calendar_retry_at.is_some_and(|at| Instant::now() < at);
        if !fresh && !waiting {
            match adapter.calendar(today).await {
                Ok(calendar) => {
                    self.calendar = Some((today, calendar));
                    self.calendar_retry_at = None;
                }
                Err(error) => {
                    tracing::warn!(%error, "장 달력을 받지 못했습니다");
                    self.calendar_retry_at = Some(Instant::now() + CALENDAR_RETRY);
                }
            }
        }
    }
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

    #[test]
    fn 통화는_코드_글자로_적는다() {
        assert_eq!(currency_code(&Currency::Krw), "KRW");
        assert_eq!(currency_code(&Currency::Usd), "USD");
        assert_eq!(currency_code(&Currency::Other("JPY".to_owned())), "JPY");
    }
}
