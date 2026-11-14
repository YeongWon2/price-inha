//! 받은 시세를 간직하고 늦은 시세(N6)를 가려낸다.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use pi_market::{Currency, Market, MarketCalendar, Quote, StalePolicy, Symbol};
use serde_json::{Value, json};

/// 마지막으로 알린 "시세 늦음".
#[derive(Default, PartialEq, Eq)]
struct Late {
    symbols: Vec<String>,
    /// 장 시간을 모르는 채로 판단했는가.
    hours_unknown: bool,
}

/// 시세판: 종목마다 가장 최근 시세, 처음 물어본 시각, 장 달력.
#[derive(Default)]
pub(crate) struct Board {
    /// 종목마다 가장 최근에 받은 시세.
    held: HashMap<Symbol, Quote>,
    /// 종목마다 처음 물어본 시각. 시세가 한 번도 오지 않은 종목의 늦음을 여기서부터 잰다.
    asked_at: HashMap<Symbol, DateTime<Utc>>,
    /// 달력과 그 기준일.
    calendar: Option<(NaiveDate, MarketCalendar)>,
    late: Late,
}

impl Board {
    /// 이 종목들을 지금 물어본다고 적는다(처음 물어본 때만 남는다).
    pub(crate) fn asked(&mut self, symbols: &[Symbol], now: DateTime<Utc>) {
        for symbol in symbols {
            self.asked_at.entry(symbol.clone()).or_insert(now);
        }
    }

    /// 받은 시세를 간직한다.
    pub(crate) fn hold(&mut self, quotes: Vec<Quote>) {
        for quote in quotes {
            self.held.insert(quote.symbol.clone(), quote);
        }
    }

    /// 지켜보는 종목이 바뀌었다: 빠진 종목의 것은 버린다.
    pub(crate) fn keep_only(&mut self, symbols: &[Symbol]) {
        self.held.retain(|symbol, _| symbols.contains(symbol));
        self.asked_at.retain(|symbol, _| symbols.contains(symbol));
    }

    /// 시세 받기를 멈췄다: 간직한 시세와 물어본 시각을 버린다(달력은 둔다).
    pub(crate) fn forget(&mut self) {
        self.held.clear();
        self.asked_at.clear();
    }

    /// 그 날짜의 달력을 갖고 있는가.
    pub(crate) fn has_calendar_for(&self, date: NaiveDate) -> bool {
        matches!(&self.calendar, Some((held, _)) if *held == date)
    }

    pub(crate) fn set_calendar(&mut self, date: NaiveDate, calendar: MarketCalendar) {
        self.calendar = Some((date, calendar));
    }

    /// 가진 시세 전부(지켜볼 종목 순서). 시세 묶음 알림의 내용이다.
    pub(crate) fn quotes_payload(&self, symbols: &[Symbol], demo: bool) -> Value {
        let quotes: Vec<Value> = symbols
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

    /// 늦은 종목이 바뀌었으면 알릴 내용을 준다(N6). 다시 받아 늦은 종목이 없어지면 빈 목록이다.
    ///
    /// - 장 시간을 모르면(달력을 한 번도 못 받았거나 가진 달력이 지금을 덮지 않음) "장 닫힘"으로
    ///   보지 않는다: 30초 넘게 오지 않은 시세를 늦은 것으로 보고 `hours_unknown`을 참으로 알린다.
    /// - 물어봤는데 시세가 한 번도 오지 않은 종목은 처음 물어본 때부터 잰다(조용히 빠지지 않는다).
    /// - `skip`(증권사가 거절한 종목)은 따로 알리므로 여기에 넣지 않는다.
    pub(crate) fn stale_notice(
        &mut self,
        symbols: &[Symbol],
        skip: &[Symbol],
        market: Market,
        now: DateTime<Utc>,
    ) -> Option<Value> {
        let policy = StalePolicy::default();
        let calendar = self
            .calendar
            .as_ref()
            .map(|(_, calendar)| calendar)
            .filter(|calendar| covers(calendar, market, now));
        let is_late = |symbol: &&Symbol| match (self.held.get(*symbol), calendar) {
            (Some(quote), Some(calendar)) => pi_market::is_stale(quote, calendar, now, &policy),
            (Some(quote), None) => older_than(quote.received_at, now, policy.max_age),
            (None, _) => self.asked_at.get(*symbol).is_some_and(|asked| {
                calendar.is_none_or(|calendar| calendar.session_at(now).is_some())
                    && older_than(*asked, now, policy.max_age)
            }),
        };
        let late = Late {
            symbols: symbols
                .iter()
                .filter(|symbol| !skip.contains(symbol))
                .filter(is_late)
                .map(|symbol| symbol.as_str().to_owned())
                .collect(),
            hours_unknown: calendar.is_none(),
        };
        // 늦은 종목이 없는 동안에는 장 시간을 아는지가 바뀌어도 알릴 것이 없다.
        let changed = late.symbols != self.late.symbols
            || (!late.symbols.is_empty() && late.hours_unknown != self.late.hours_unknown);
        let notice = changed
            .then(|| json!({ "symbols": late.symbols, "hours_unknown": late.hours_unknown }));
        self.late = late;
        notice
    }
}

/// 가진 달력이 지금 시각을 덮는가: 지금의 시장 날짜가 달력의 이전 영업일과 다음 영업일 사이에 있다.
/// (그 사이의 날은 달력이 세션을 알거나, 영업일이 아니라고 알려 준 날이다.)
fn covers(calendar: &MarketCalendar, market: Market, now: DateTime<Utc>) -> bool {
    let today = market_date(market, now);
    calendar.previous_business_day.date <= today && today <= calendar.next_business_day.date
}

/// `since`로부터 `max_age`를 넘었는가(딱 그 시간이면 아직 아니다). 장 시간은 보지 않는다.
/// `since`가 지금보다 뒤면(시계가 어긋남) 나이를 잴 수 없으니 넘었다고 하지 않는다.
fn older_than(since: DateTime<Utc>, now: DateTime<Utc>, max_age: Duration) -> bool {
    now.signed_duration_since(since)
        .to_std()
        .is_ok_and(|age| age > max_age)
}

/// 그 시장의 오늘 날짜. 달력에는 앞뒤 영업일도 들어 있어 하루쯤 어긋나도 세션을 찾는다.
pub(crate) fn market_date(market: Market, now: DateTime<Utc>) -> NaiveDate {
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
    fn 통화는_코드_글자로_적는다() {
        assert_eq!(currency_code(&Currency::Krw), "KRW");
        assert_eq!(currency_code(&Currency::Usd), "USD");
        assert_eq!(currency_code(&Currency::Other("JPY".to_owned())), "JPY");
    }

    #[test]
    fn 한_번도_오지_않은_종목은_처음_물어본_때부터_30초를_넘으면_늦다() {
        let Some(start) = Utc.with_ymd_and_hms(2026, 3, 25, 1, 0, 0).single() else {
            return;
        };
        let symbols = [Symbol::new("005930"), Symbol::new("NOPE")];
        let mut board = Board::default();
        board.asked(&symbols, start);
        // 다시 물어도 처음 물어본 때가 남는다.
        board.asked(&symbols, start + TimeDelta::seconds(20));
        let at = |seconds| start + TimeDelta::seconds(seconds);
        assert_eq!(
            board.stale_notice(&symbols, &[], Market::Kr, at(30)),
            None,
            "딱 30초는 아직 아니다"
        );
        // 거절된 종목은 따로 알리므로 늦은 종목에 넣지 않는다.
        let skip = [Symbol::new("NOPE")];
        assert_eq!(
            board.stale_notice(&symbols, &skip, Market::Kr, at(31)),
            Some(json!({ "symbols": ["005930"], "hours_unknown": true }))
        );
        // 멈추면 늦은 종목이 없어졌다고 알린다.
        board.forget();
        assert_eq!(
            board.stale_notice(&symbols, &skip, Market::Kr, at(32)),
            Some(json!({ "symbols": [], "hours_unknown": true }))
        );
    }
}
