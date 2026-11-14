//! 바깥 요청이 없는 것들: 값 · 달력의 `session_at` · 늦은 시세(N6) · 가짜 어댑터 · 지켜보기(V32).

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{
    Currency, DEFAULT_STALE_AFTER, FakeAdapter, Market, MarketAdapter, MarketCalendar, MarketError,
    Quote, Session, SessionKind, StalePolicy, Symbol, TradingDay, WatchEvent, is_stale, watch,
};
use rust_decimal::Decimal;

fn ts(text: &str) -> DateTime<Utc> {
    text.parse::<DateTime<Utc>>().unwrap()
}

fn day(text: &str) -> NaiveDate {
    text.parse::<NaiveDate>().unwrap()
}

fn session(kind: SessionKind, start: &str, end: &str) -> Session {
    Session {
        kind,
        start: ts(start),
        end: ts(end),
    }
}

/// 공식 명세 예시의 국내 영업일: 프리 08:00~09:00, 정규 09:00~15:30, 애프터 15:30~20:00 (KST).
fn kr_day(date: &str) -> TradingDay {
    TradingDay {
        date: day(date),
        sessions: vec![
            session(
                SessionKind::PreMarket,
                &format!("{date}T08:00:00+09:00"),
                &format!("{date}T09:00:00+09:00"),
            ),
            session(
                SessionKind::Regular,
                &format!("{date}T09:00:00+09:00"),
                &format!("{date}T15:30:00+09:00"),
            ),
            session(
                SessionKind::AfterMarket,
                &format!("{date}T15:30:00+09:00"),
                &format!("{date}T20:00:00+09:00"),
            ),
        ],
    }
}

fn holiday(date: &str) -> TradingDay {
    TradingDay {
        date: day(date),
        sessions: Vec::new(),
    }
}

fn kr_calendar() -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        today: kr_day("2026-03-25"),
        previous_business_day: kr_day("2026-03-24"),
        next_business_day: kr_day("2026-03-26"),
    }
}

/// 공식 명세 예시의 미국 영업일(KST): 데이 09:00~16:50, 프리 17:00~22:30,
/// 정규 22:30~다음 날 05:00, 애프터 다음 날 05:00~07:00.
fn us_day(date: &str, next: &str) -> TradingDay {
    TradingDay {
        date: day(date),
        sessions: vec![
            session(
                SessionKind::DayMarket,
                &format!("{date}T09:00:00+09:00"),
                &format!("{date}T16:50:00+09:00"),
            ),
            session(
                SessionKind::PreMarket,
                &format!("{date}T17:00:00+09:00"),
                &format!("{date}T22:30:00+09:00"),
            ),
            session(
                SessionKind::Regular,
                &format!("{date}T22:30:00+09:00"),
                &format!("{next}T05:00:00+09:00"),
            ),
            session(
                SessionKind::AfterMarket,
                &format!("{next}T05:00:00+09:00"),
                &format!("{next}T07:00:00+09:00"),
            ),
        ],
    }
}

fn us_calendar(today: &str) -> MarketCalendar {
    let dates = [
        "2026-03-23",
        "2026-03-24",
        "2026-03-25",
        "2026-03-26",
        "2026-03-27",
        "2026-03-28",
    ];
    let i = dates.iter().position(|d| *d == today).unwrap();
    MarketCalendar {
        market: Market::Us,
        today: us_day(dates[i], dates[i + 1]),
        previous_business_day: us_day(dates[i - 1], dates[i]),
        next_business_day: us_day(dates[i + 1], dates[i + 2]),
    }
}

fn quote(symbol: &str, price: &str, received_at: &str) -> Quote {
    Quote {
        symbol: Symbol::new(symbol),
        price: Decimal::from_str(price).unwrap(),
        currency: Currency::Krw,
        exchange_time: None,
        received_at: ts(received_at),
    }
}

// ───────────── 값 ─────────────

// 검증: D5
#[test]
fn 심볼은_받은_글자를_그대로_둔다() {
    assert_eq!(Symbol::new("005930").as_str(), "005930");
    assert_eq!(Symbol::new("BRK.B").as_str(), "BRK.B");
    assert_eq!(Symbol::new("005930"), Symbol::new(String::from("005930")));
}

// 검증: D5
#[test]
fn 영업일은_세션이_없으면_휴장이고_종류로_세션을_찾는다() {
    let open = kr_day("2026-03-25");
    assert!(!open.is_holiday());
    let regular = open.session(SessionKind::Regular).unwrap();
    assert_eq!(regular.start, ts("2026-03-25T09:00:00+09:00"));
    assert_eq!(regular.end, ts("2026-03-25T15:30:00+09:00"));
    assert!(open.session(SessionKind::DayMarket).is_none());

    let closed = holiday("2026-05-05");
    assert!(closed.is_holiday());
    assert!(closed.session(SessionKind::Regular).is_none());
}

// ───────────── session_at ─────────────

// 검증: D5
#[test]
fn 세션_시작_시각은_포함한다() {
    let cal = kr_calendar();
    let got = cal.session_at(ts("2026-03-25T08:00:00+09:00")).unwrap();
    assert_eq!(got.kind, SessionKind::PreMarket);
    assert_eq!(got.start, ts("2026-03-25T08:00:00+09:00"));
    assert_eq!(got.end, ts("2026-03-25T09:00:00+09:00"));
}

// 검증: D5
#[test]
fn 첫_세션_시작_직전은_장_닫힘이다() {
    let cal = kr_calendar();
    assert_eq!(cal.session_at(ts("2026-03-25T07:59:59.999+09:00")), None);
}

// 검증: D5
#[test]
fn 세션_종료_시각은_제외한다() {
    let cal = kr_calendar();
    let just_before = cal.session_at(ts("2026-03-25T19:59:59.999+09:00")).unwrap();
    assert_eq!(just_before.kind, SessionKind::AfterMarket);
    assert_eq!(cal.session_at(ts("2026-03-25T20:00:00+09:00")), None);
}

// 검증: D5
#[test]
fn 맞닿은_세션의_경계는_뒤_세션이다() {
    let cal = kr_calendar();
    let at_open = cal.session_at(ts("2026-03-25T09:00:00+09:00")).unwrap();
    assert_eq!(at_open.kind, SessionKind::Regular);
    let before_open = cal.session_at(ts("2026-03-25T08:59:59.999+09:00")).unwrap();
    assert_eq!(before_open.kind, SessionKind::PreMarket);
    let at_close = cal.session_at(ts("2026-03-25T15:30:00+09:00")).unwrap();
    assert_eq!(at_close.kind, SessionKind::AfterMarket);
}

// 검증: D5
#[test]
fn 시각대가_달라도_같은_순간이면_같은_세션이다() {
    let cal = kr_calendar();
    // 2026-03-25 10:00 KST == 01:00 UTC
    let got = cal.session_at(ts("2026-03-25T01:00:00Z")).unwrap();
    assert_eq!(got.kind, SessionKind::Regular);
}

// 검증: D5
#[test]
fn 앞뒤_영업일의_세션도_찾는다() {
    let cal = kr_calendar();
    let prev = cal.session_at(ts("2026-03-24T10:00:00+09:00")).unwrap();
    assert_eq!(prev.kind, SessionKind::Regular);
    assert_eq!(prev.start, ts("2026-03-24T09:00:00+09:00"));
    let next = cal.session_at(ts("2026-03-26T16:00:00+09:00")).unwrap();
    assert_eq!(next.kind, SessionKind::AfterMarket);
    assert_eq!(next.start, ts("2026-03-26T15:30:00+09:00"));
}

// 검증: D5
#[test]
fn 휴장일에는_세션이_없다() {
    let cal = MarketCalendar {
        market: Market::Kr,
        today: holiday("2026-05-05"),
        previous_business_day: kr_day("2026-05-04"),
        next_business_day: kr_day("2026-05-06"),
    };
    assert_eq!(cal.session_at(ts("2026-05-05T10:00:00+09:00")), None);
    assert_eq!(cal.session_at(ts("2026-05-05T08:00:00+09:00")), None);
    // 앞 영업일은 그대로 열린다.
    assert!(cal.session_at(ts("2026-05-04T10:00:00+09:00")).is_some());
}

// 검증: D5
#[test]
fn 부분_휴장이면_없는_세션_시간은_장_닫힘이다() {
    let mut cal = kr_calendar();
    cal.today
        .sessions
        .retain(|s| s.kind != SessionKind::PreMarket);
    assert_eq!(cal.session_at(ts("2026-03-25T08:30:00+09:00")), None);
    let regular = cal.session_at(ts("2026-03-25T09:00:00+09:00")).unwrap();
    assert_eq!(regular.kind, SessionKind::Regular);
}

// 검증: D5
#[test]
fn 미국_정규장은_날짜를_넘어도_이어진다() {
    let cal = us_calendar("2026-03-25");
    let open = cal.session_at(ts("2026-03-25T22:30:00+09:00")).unwrap();
    assert_eq!(open.kind, SessionKind::Regular);
    let midnight = cal.session_at(ts("2026-03-26T00:00:00+09:00")).unwrap();
    assert_eq!(midnight.kind, SessionKind::Regular);
    assert_eq!(midnight.start, ts("2026-03-25T22:30:00+09:00"));
    assert_eq!(midnight.end, ts("2026-03-26T05:00:00+09:00"));
    let last = cal.session_at(ts("2026-03-26T04:59:59.999+09:00")).unwrap();
    assert_eq!(last.kind, SessionKind::Regular);
    let after = cal.session_at(ts("2026-03-26T05:00:00+09:00")).unwrap();
    assert_eq!(after.kind, SessionKind::AfterMarket);
    assert_eq!(cal.session_at(ts("2026-03-26T07:00:00+09:00")), None);
}

// 검증: D5
#[test]
fn 미국_새벽은_이전_영업일의_정규장이다() {
    // 기준일이 03-26인 달력에서 03-26 03:00 KST는 03-25(이전 영업일) 정규장이다.
    let cal = us_calendar("2026-03-26");
    let got = cal.session_at(ts("2026-03-26T03:00:00+09:00")).unwrap();
    assert_eq!(got.kind, SessionKind::Regular);
    assert_eq!(got.start, ts("2026-03-25T22:30:00+09:00"));
}

// 검증: D5
#[test]
fn 미국_데이마켓과_프리마켓_사이는_장_닫힘이다() {
    let cal = us_calendar("2026-03-25");
    let day_market = cal.session_at(ts("2026-03-25T16:49:59+09:00")).unwrap();
    assert_eq!(day_market.kind, SessionKind::DayMarket);
    assert_eq!(cal.session_at(ts("2026-03-25T16:50:00+09:00")), None);
    assert_eq!(cal.session_at(ts("2026-03-25T16:59:59+09:00")), None);
    let pre = cal.session_at(ts("2026-03-25T17:00:00+09:00")).unwrap();
    assert_eq!(pre.kind, SessionKind::PreMarket);
}

// ───────────── 늦은 시세 (N6) ─────────────

// 검증: N6
#[test]
fn 늦은_시세_기준의_기본값은_30초다() {
    assert_eq!(DEFAULT_STALE_AFTER, Duration::from_secs(30));
    assert_eq!(StalePolicy::default().max_age, Duration::from_secs(30));
}

// 검증: N6
#[test]
fn 장_중에_받은_지_30초가_넘으면_늦은_시세다() {
    let cal = kr_calendar();
    let policy = StalePolicy::default();
    let q = quote("005930", "72000", "2026-03-25T10:00:00+09:00");

    assert!(!is_stale(
        &q,
        &cal,
        ts("2026-03-25T10:00:00+09:00"),
        &policy
    ));
    assert!(!is_stale(
        &q,
        &cal,
        ts("2026-03-25T10:00:29.999+09:00"),
        &policy
    ));
    // 딱 30초는 아직 늦지 않았다("30초 넘게").
    assert!(!is_stale(
        &q,
        &cal,
        ts("2026-03-25T10:00:30+09:00"),
        &policy
    ));
    assert!(is_stale(
        &q,
        &cal,
        ts("2026-03-25T10:00:30.001+09:00"),
        &policy
    ));
    assert!(is_stale(&q, &cal, ts("2026-03-25T10:05:00+09:00"), &policy));
}

// 검증: N6
#[test]
fn 늦은_시세_기준은_설정값으로_바꿀_수_있다() {
    let cal = kr_calendar();
    let q = quote("005930", "72000", "2026-03-25T10:00:00+09:00");

    let short = StalePolicy {
        max_age: Duration::from_secs(5),
    };
    assert!(!is_stale(&q, &cal, ts("2026-03-25T10:00:05+09:00"), &short));
    assert!(is_stale(&q, &cal, ts("2026-03-25T10:00:06+09:00"), &short));

    let long = StalePolicy {
        max_age: Duration::from_secs(120),
    };
    assert!(!is_stale(&q, &cal, ts("2026-03-25T10:00:31+09:00"), &long));
    assert!(!is_stale(&q, &cal, ts("2026-03-25T10:02:00+09:00"), &long));
    assert!(is_stale(&q, &cal, ts("2026-03-25T10:02:01+09:00"), &long));
}

// 검증: N6
#[test]
fn 장이_닫힌_동안은_늦은_시세로_보지_않는다() {
    let cal = kr_calendar();
    let policy = StalePolicy::default();
    // 장 끝나기 직전에 받은 시세를 밤에 본다.
    let q = quote("005930", "72000", "2026-03-25T19:59:00+09:00");
    assert!(!is_stale(
        &q,
        &cal,
        ts("2026-03-25T20:00:00+09:00"),
        &policy
    ));
    assert!(!is_stale(
        &q,
        &cal,
        ts("2026-03-25T23:00:00+09:00"),
        &policy
    ));
    // 다음 날 장이 열리면 늦은 시세다.
    assert!(is_stale(&q, &cal, ts("2026-03-26T08:00:00+09:00"), &policy));
}

// 검증: N6
#[test]
fn 휴장일에는_늦은_시세로_보지_않는다() {
    let cal = MarketCalendar {
        market: Market::Kr,
        today: holiday("2026-05-05"),
        previous_business_day: kr_day("2026-05-04"),
        next_business_day: kr_day("2026-05-06"),
    };
    let q = quote("005930", "72000", "2026-05-04T19:00:00+09:00");
    assert!(!is_stale(
        &q,
        &cal,
        ts("2026-05-05T10:00:00+09:00"),
        &StalePolicy::default()
    ));
}

// 검증: N6
#[test]
fn 프리마켓과_애프터마켓도_열린_것으로_본다() {
    let cal = kr_calendar();
    let policy = StalePolicy::default();
    let pre = quote("005930", "72000", "2026-03-25T08:10:00+09:00");
    assert!(is_stale(
        &pre,
        &cal,
        ts("2026-03-25T08:11:00+09:00"),
        &policy
    ));
    let after = quote("005930", "72000", "2026-03-25T16:00:00+09:00");
    assert!(is_stale(
        &after,
        &cal,
        ts("2026-03-25T16:01:00+09:00"),
        &policy
    ));
}

// 검증: N6
#[test]
fn 늦은_시세는_거래소_시각이_아니라_받은_시각으로_판단한다() {
    let cal = kr_calendar();
    let policy = StalePolicy::default();

    // 거래소 시각은 오래됐지만(거래가 뜸한 종목) 방금 받았다 → 늦지 않았다.
    let mut fresh = quote("005930", "72000", "2026-03-25T10:00:00+09:00");
    fresh.exchange_time = Some(ts("2026-03-25T09:10:00+09:00"));
    assert!(!is_stale(
        &fresh,
        &cal,
        ts("2026-03-25T10:00:10+09:00"),
        &policy
    ));

    // 거래소 시각은 새것처럼 보여도 받은 지 오래됐다 → 늦었다.
    let mut old = quote("005930", "72000", "2026-03-25T10:00:00+09:00");
    old.exchange_time = Some(ts("2026-03-25T10:00:50+09:00"));
    assert!(is_stale(
        &old,
        &cal,
        ts("2026-03-25T10:01:00+09:00"),
        &policy
    ));
}

// 검증: N6, D5
#[test]
fn 미국_정규장이_날짜를_넘은_뒤에도_늦은_시세를_잡는다() {
    let cal = us_calendar("2026-03-25");
    let mut q = quote("AAPL", "185.70", "2026-03-26T01:00:00+09:00");
    q.currency = Currency::Usd;
    assert!(is_stale(
        &q,
        &cal,
        ts("2026-03-26T01:00:31+09:00"),
        &StalePolicy::default()
    ));
}

// ───────────── 가짜 어댑터 ─────────────

// 검증: D5
#[tokio::test]
async fn 가짜_어댑터는_정해_준_시세를_달라는_순서대로_돌려준다() {
    let fake = FakeAdapter::new(Market::Kr);
    assert_eq!(fake.market(), Market::Kr);
    let samsung = quote("005930", "72000", "2026-03-25T10:00:00+09:00");
    let hynix = quote("000660", "185500", "2026-03-25T10:00:01+09:00");
    fake.set_quote(samsung.clone());
    fake.set_quote(hynix.clone());

    let got = fake
        .quotes(&[Symbol::new("000660"), Symbol::new("005930")])
        .await
        .unwrap();
    assert_eq!(got, vec![hynix, samsung.clone()]);

    // 달라고 한 종목만 준다. 정하지 않은 종목은 뺀다.
    let got = fake
        .quotes(&[Symbol::new("005930"), Symbol::new("035420")])
        .await
        .unwrap();
    assert_eq!(got, vec![samsung]);
}

// 검증: D5
#[tokio::test]
async fn 가짜_어댑터의_시세를_다시_정하면_바뀐다() {
    let fake = FakeAdapter::new(Market::Us);
    assert_eq!(fake.market(), Market::Us);
    fake.set_quote(quote("AAPL", "185.70", "2026-03-25T23:00:00+09:00"));
    let newer = quote("AAPL", "186.05", "2026-03-25T23:00:05+09:00");
    fake.set_quote(newer.clone());
    let got = fake.quotes(&[Symbol::new("AAPL")]).await.unwrap();
    assert_eq!(got, vec![newer]);
}

// 검증: D5
#[tokio::test]
async fn 가짜_어댑터는_정해_준_달력을_돌려주고_세션을_알려_준다() {
    let fake = FakeAdapter::new(Market::Kr);
    fake.set_calendar(kr_calendar());
    let cal = fake.calendar(day("2026-03-25")).await.unwrap();
    assert_eq!(cal, kr_calendar());

    // trait의 session_at은 달력의 session_at과 같다.
    let at = ts("2026-03-25T10:00:00+09:00");
    let got = fake.session_at(&cal, at).unwrap();
    assert_eq!(got.kind, SessionKind::Regular);
    assert_eq!(fake.session_at(&cal, at), cal.session_at(at));
    assert_eq!(fake.session_at(&cal, ts("2026-03-25T21:00:00+09:00")), None);
}

// 검증: D5
#[tokio::test]
async fn 가짜_어댑터는_달력을_정하지_않으면_휴장_달력을_돌려준다() {
    let fake = FakeAdapter::new(Market::Us);
    let cal = fake.calendar(day("2026-03-25")).await.unwrap();
    assert_eq!(cal.market, Market::Us);
    assert_eq!(cal.today.date, day("2026-03-25"));
    assert!(cal.today.is_holiday());
    assert!(cal.previous_business_day.is_holiday());
    assert!(cal.next_business_day.is_holiday());
    assert_eq!(cal.session_at(ts("2026-03-25T23:00:00+09:00")), None);
}

// 검증: D5
#[tokio::test]
async fn 가짜_어댑터는_실패를_정할_수_있고_부른_횟수를_센다() {
    let fake = FakeAdapter::new(Market::Kr);
    fake.set_quote(quote("005930", "72000", "2026-03-25T10:00:00+09:00"));
    assert_eq!(fake.quote_calls(), 0);

    let failure = MarketError::Unavailable {
        status: 500,
        code: Some("maintenance".to_string()),
    };
    fake.fail_quotes(Some(failure.clone()));
    let err = fake.quotes(&[Symbol::new("005930")]).await.unwrap_err();
    assert_eq!(err, failure);
    assert_eq!(fake.quote_calls(), 1);

    fake.fail_quotes(None);
    let got = fake.quotes(&[Symbol::new("005930")]).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(fake.quote_calls(), 2);
}

// 검증: D5
#[tokio::test]
async fn 어댑터는_trait_객체로_쓸_수_있다() {
    let adapters: Vec<Arc<dyn MarketAdapter>> = vec![
        Arc::new(FakeAdapter::new(Market::Kr)),
        Arc::new(FakeAdapter::new(Market::Us)),
    ];
    let markets: Vec<Market> = adapters.iter().map(|a| a.market()).collect();
    assert_eq!(markets, vec![Market::Kr, Market::Us]);
    for adapter in &adapters {
        assert_eq!(adapter.quotes(&[]).await.unwrap(), Vec::new());
    }
}

// ───────────── 지켜보기 (V32) ─────────────

const INTERVAL: Duration = Duration::from_secs(5);

// 검증: V32
#[tokio::test(start_paused = true)]
async fn 지켜보기는_부르자마자_한_번_받고_간격마다_시세를_보낸다() {
    let fake = Arc::new(FakeAdapter::new(Market::Kr));
    let first = quote("005930", "72000", "2026-03-25T10:00:00+09:00");
    fake.set_quote(first.clone());
    // 지켜보지 않는 종목은 흐름에 오지 않는다.
    fake.set_quote(quote("000660", "185500", "2026-03-25T10:00:00+09:00"));

    let started = tokio::time::Instant::now();
    let mut rx = watch(fake.clone(), vec![Symbol::new("005930")], INTERVAL);

    let event = rx.recv().await.unwrap();
    assert_eq!(event, WatchEvent::Quotes(vec![first]));
    assert!(started.elapsed() < Duration::from_secs(1));

    // 시세가 바뀌면 다음 간격에 바뀐 값이 온다.
    let second = quote("005930", "72100", "2026-03-25T10:00:05+09:00");
    fake.set_quote(second.clone());
    let event = rx.recv().await.unwrap();
    assert_eq!(event, WatchEvent::Quotes(vec![second]));
    let elapsed = started.elapsed();
    assert!(elapsed >= INTERVAL && elapsed < INTERVAL + Duration::from_secs(1));

    let third = quote("005930", "71900", "2026-03-25T10:00:10+09:00");
    fake.set_quote(third.clone());
    let event = rx.recv().await.unwrap();
    assert_eq!(event, WatchEvent::Quotes(vec![third]));
    let elapsed = started.elapsed();
    assert!(elapsed >= INTERVAL * 2 && elapsed < INTERVAL * 2 + Duration::from_secs(1));

    assert_eq!(fake.quote_calls(), 3);
}

// 검증: V32, N6
#[tokio::test(start_paused = true)]
async fn 지켜보기는_실패도_흐름으로_알리고_끊기지_않는다() {
    let fake = Arc::new(FakeAdapter::new(Market::Kr));
    let q = quote("005930", "72000", "2026-03-25T10:00:00+09:00");
    fake.set_quote(q.clone());
    let mut rx = watch(fake.clone(), vec![Symbol::new("005930")], INTERVAL);
    assert_eq!(
        rx.recv().await.unwrap(),
        WatchEvent::Quotes(vec![q.clone()])
    );

    let failure = MarketError::Connection {
        detail: "시험용 연결 실패".to_string(),
    };
    fake.fail_quotes(Some(failure.clone()));
    assert_eq!(
        rx.recv().await.unwrap(),
        WatchEvent::Failed(failure.clone())
    );
    assert_eq!(rx.recv().await.unwrap(), WatchEvent::Failed(failure));

    // 다시 받을 수 있게 되면 같은 흐름으로 시세가 온다.
    fake.fail_quotes(None);
    assert_eq!(rx.recv().await.unwrap(), WatchEvent::Quotes(vec![q]));
    assert_eq!(fake.quote_calls(), 4);
}

// 검증: V32
#[tokio::test(start_paused = true)]
async fn 지켜보기는_받는_쪽이_사라지면_멈춘다() {
    let fake = Arc::new(FakeAdapter::new(Market::Kr));
    fake.set_quote(quote("005930", "72000", "2026-03-25T10:00:00+09:00"));
    let mut rx = watch(fake.clone(), vec![Symbol::new("005930")], INTERVAL);
    rx.recv().await.unwrap();
    rx.recv().await.unwrap();
    let before = fake.quote_calls();
    assert_eq!(before, 2);
    drop(rx);

    // 사라진 것을 알아채는 데 한 번까지는 더 받을 수 있다.
    tokio::time::sleep(INTERVAL * 10).await;
    let after = fake.quote_calls();
    assert!(
        after <= before + 1,
        "받는 쪽이 사라진 뒤 {}번 더 받았다",
        after - before
    );

    // 그 뒤로는 더 부르지 않는다.
    tokio::time::sleep(INTERVAL * 10).await;
    assert_eq!(fake.quote_calls(), after);
}
