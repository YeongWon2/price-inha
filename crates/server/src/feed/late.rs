//! 받은 시세를 간직하고 늦은 시세(N6)를 가려낸다.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use pi_market::{Currency, Market, MarketCalendar, Quote, StalePolicy, Symbol, TradingDay};
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
    /// - (7차 7번) 달력이 지금을 덮는지는 `MarketCalendar::covers`가, 늦음은 `pi_market::is_stale`이 정한다
    ///   (따로 세지 않는다). 달력이 없어도 받은 시각이 지금보다 뒤인 시세(시계가 어긋남)는 늦은 것이다.
    pub(crate) fn stale_notice(
        &mut self,
        symbols: &[Symbol],
        skip: &[Symbol],
        now: DateTime<Utc>,
    ) -> Option<Value> {
        let policy = StalePolicy::default();
        let calendar = self
            .calendar
            .as_ref()
            .map(|(_, calendar)| calendar)
            .filter(|calendar| calendar.covers(now));
        let is_late = |symbol: &&Symbol| match (self.held.get(*symbol), calendar) {
            (Some(quote), Some(calendar)) => pi_market::is_stale(quote, calendar, now, &policy),
            (Some(quote), None) => {
                quote.received_at > now || older_than(quote.received_at, now, policy.max_age)
            }
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

/// `since`로부터 `max_age`를 넘었는가(딱 그 시간이면 아직 아니다). 장 시간은 보지 않는다.
/// `since`가 지금보다 뒤면(시계가 어긋남) 나이를 잴 수 없으니 넘었다고 하지 않는다.
fn older_than(since: DateTime<Utc>, now: DateTime<Utc>, max_age: Duration) -> bool {
    now.signed_duration_since(since)
        .to_std()
        .is_ok_and(|age| age > max_age)
}

/// 그 시장의 오늘 날짜(달력을 받을 날).
///
/// 날짜의 경계(그 시장 날짜의 자정, 미국은 서머타임을 따른다)는 따로 세지 않고 `pi_market`의
/// `MarketCalendar::covers`에 맡긴다(7차 7번): 그 시장에서 있을 수 있는 시차(국내 UTC+9,
/// 미국 UTC-4 · UTC-5)로 센 날짜 가운데 그 날짜 하나로 된 달력이 지금을 덮는 날을 고른다.
pub(crate) fn market_date(market: Market, now: DateTime<Utc>) -> NaiveDate {
    let offsets: &[i64] = match market {
        Market::Us => &[-4, -5],
        _ => &[9],
    };
    let mut candidates = offsets
        .iter()
        .filter_map(|hours| now.checked_add_signed(TimeDelta::hours(*hours)))
        .map(|local| local.date_naive())
        .peekable();
    let first = candidates.peek().copied();
    candidates
        .find(|date| one_day(market, *date).covers(now))
        .or(first)
        .unwrap_or_else(|| now.date_naive())
}

/// 그 날짜 하나(세션 없음)로 된 달력. 그 날짜가 지금을 덮는지 볼 때만 쓴다.
fn one_day(market: Market, date: NaiveDate) -> MarketCalendar {
    let day = || TradingDay {
        date,
        sessions: Vec::new(),
    };
    MarketCalendar {
        market,
        today: day(),
        previous_business_day: day(),
        next_business_day: day(),
    }
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
    fn 미국_시장_날짜는_서머타임을_따른다() {
        // 2026-07-15 04:30 UTC = 미 동부 서머타임(UTC-4) 15일 00:30. UTC-5로 세면 14일이다.
        let summer = Utc.with_ymd_and_hms(2026, 7, 15, 4, 30, 0).single();
        // 2026-01-15 04:30 UTC = 미 동부 표준시(UTC-5) 14일 23:30.
        let winter = Utc.with_ymd_and_hms(2026, 1, 15, 4, 30, 0).single();
        let (Some(summer), Some(winter)) = (summer, winter) else {
            return;
        };
        assert_eq!(
            Some(market_date(Market::Us, summer)),
            NaiveDate::from_ymd_opt(2026, 7, 15)
        );
        assert_eq!(
            Some(market_date(Market::Us, winter)),
            NaiveDate::from_ymd_opt(2026, 1, 14)
        );
    }

    #[test]
    fn 달력이_없어도_받은_시각이_미래인_시세는_늦다() {
        let Some(now) = Utc.with_ymd_and_hms(2026, 3, 25, 1, 0, 0).single() else {
            return;
        };
        let symbols = [Symbol::new("005930")];
        let mut board = Board::default();
        board.hold(vec![Quote {
            symbol: symbols[0].clone(),
            price: rust_decimal::Decimal::ONE,
            currency: Currency::Krw,
            exchange_time: None,
            received_at: now + TimeDelta::seconds(5),
        }]);
        assert_eq!(
            board.stale_notice(&symbols, &[], now),
            Some(json!({ "symbols": ["005930"], "hours_unknown": true }))
        );
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
            board.stale_notice(&symbols, &[], at(30)),
            None,
            "딱 30초는 아직 아니다"
        );
        // 거절된 종목은 따로 알리므로 늦은 종목에 넣지 않는다.
        let skip = [Symbol::new("NOPE")];
        assert_eq!(
            board.stale_notice(&symbols, &skip, at(31)),
            Some(json!({ "symbols": ["005930"], "hours_unknown": true }))
        );
        // 멈추면 늦은 종목이 없어졌다고 알린다.
        board.forget();
        assert_eq!(
            board.stale_notice(&symbols, &skip, at(32)),
            Some(json!({ "symbols": [], "hours_unknown": true }))
        );
    }
}
