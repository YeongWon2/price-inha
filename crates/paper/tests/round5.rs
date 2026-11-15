//! 5차 작업 1~4 (`docs/dev/modules/paper.md` "5차 작업"). 5 · 6번은 문서 주석뿐이라 테스트가 없다.
//!
//! 1. 받은 시각보다 [`MAX_EXCHANGE_LEAD`](5분) **넘게** 앞선 거래소 시각은 그 시세의 거래소 시각으로
//!    쓰지 않는다(없는 것과 같이 다룬다). 한 번 잘못 온 시각이 그 종목의 체결을 계속 막지 않는다.
//! 2. 모양 3 → 4로 올린 계좌도 같은 시각의 쓴 거래를 다시 쓰지 않는다.
//! 3. 같은 시각 값 목록은 [`MAX_SAME_TIME_PRICES`](64)개까지다. 넘으면 가장 오래 기억한 것부터 버린다.
//! 4. 되살릴 때 기억 목록(`ledger.trade_marks`)을 검사한다: 종목 · 통화 중복 없음, 정렬, 기억한 시각 ≥
//!    마지막 시세의 거래소 시각. 어긋나면 되살리지 않고 오류다.
//!
//! 이 파일은 기존 묶음(`suite.rs`)과 따로 도는 테스트 실행 파일이라 도우미를 안에 둔다.

use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{
    Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol, TradingDay,
};
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, BrokerError, CashBalance, CheckAuthority, InitialCash,
    LimitFillPrice, MAX_EXCHANGE_LEAD, MAX_SAME_TIME_PRICES, MarketRules, MaxFill, MoneyRounding,
    OrderAck, OrderId, OrderRequest, OrderType, PaperBroker, PaperConfig, ProceedsUsable,
    RoundingMode, SessionRules, Settlement, Side, Snapshot, TickBand, UnfilledTimeout,
};
use rust_decimal::Decimal;
use serde_json::Value;
use tokio::runtime::{Builder, Runtime};

// ───────────── 도우미 ─────────────

/// 삼성전자. 기준가 70,000원.
const SAM: &str = "005930";
/// SK하이닉스 자리. 기준가 30,000원.
const HYN: &str = "000660";

fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// 기준 영업일 2026-11-16 월요일.
fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
}

fn utc(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
}

/// 국내 정규장 안의 기준 시각(월요일 10:00 KST = 01:00 UTC)에서 `seconds`초 뒤.
fn at(seconds: i64) -> DateTime<Utc> {
    utc(monday(), 1, 0) + chrono::Duration::seconds(seconds)
}

/// 거래소 시각이 받은 시각보다 앞서도 되는 가장 긴 시간(chrono).
fn lead() -> chrono::Duration {
    chrono::Duration::from_std(MAX_EXCHANGE_LEAD).unwrap()
}

/// 정규장(09:00~15:30 KST = 00:00~06:30 UTC)만 있는 날.
fn kr_day(date: NaiveDate) -> TradingDay {
    TradingDay {
        date,
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start: utc(date, 0, 0),
            end: utc(date, 6, 30),
        }],
    }
}

/// 월요일이 기준일인 국내 달력.
fn kr_calendar() -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        today: kr_day(monday()),
        previous_business_day: kr_day(NaiveDate::from_ymd_opt(2026, 11, 13).unwrap()),
        next_business_day: kr_day(NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()),
    }
}

fn rules(currency: Currency, scale: u32, ticks: Vec<TickBand>, business_days: u32) -> MarketRules {
    MarketRules {
        currency,
        commission_rate: d("0.00015"),
        sell_tax_rate: d("0.0020"),
        rounding: MoneyRounding {
            scale,
            mode: RoundingMode::Down,
        },
        tick_table: ticks,
        price_limit_rate: None,
        settlement: Settlement {
            business_days,
            proceeds_usable: ProceedsUsable::AfterSettlement,
        },
        market_slippage_rate: d("0.001"),
        limit_fill_price: LimitFillPrice::Limit,
        max_fill: MaxFill::Unlimited,
        sessions: SessionRules {
            limit: vec![SessionKind::Regular],
            market: vec![SessionKind::Regular],
        },
        high_value_confirm_from: None,
        max_order_amount: None,
    }
}

/// 테스트 설정. 원화 10,000,000. 국내 호가 단위는 100원 하나(이 파일의 값은 모두 5만~20만 원).
/// 기본값에 기대지 않게 숫자를 모두 적는다.
fn config() -> PaperConfig {
    let band = |below: Option<&str>, tick: &str| TickBand {
        below: below.map(d),
        tick: d(tick),
    };
    PaperConfig {
        initial_cash: InitialCash {
            krw: d("10000000"),
            usd: d("10000"),
        },
        kr: rules(
            Currency::Krw,
            0,
            vec![
                band(Some("2000"), "1"),
                band(Some("5000"), "5"),
                band(Some("20000"), "10"),
                band(Some("50000"), "50"),
                band(Some("200000"), "100"),
                band(None, "1000"),
            ],
            2,
        ),
        us: rules(Currency::Usd, 2, vec![band(None, "0.01")], 1),
        stale: StalePolicy {
            max_age: Duration::from_secs(30),
        },
        event_buffer: 64,
    }
}

/// 한 시세에 1주까지 체결되는 설정.
fn one_per_quote() -> PaperConfig {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d("1"));
    config
}

/// 원화 시세. 거래소 시각과 받은 시각을 저마다 정한다.
fn quote(
    code: &str,
    price: &str,
    traded_at: Option<DateTime<Utc>>,
    received_at: DateTime<Utc>,
) -> Quote {
    Quote {
        symbol: Symbol::new(code),
        price: d(price),
        currency: Currency::Krw,
        exchange_time: traded_at,
        received_at,
    }
}

/// 모의계좌 하나와 그 권한, async 메서드를 돌릴 실행기.
struct Desk {
    broker: PaperBroker,
    authority: CheckAuthority,
    runtime: Runtime,
}

impl Desk {
    fn wrap((broker, authority): (PaperBroker, CheckAuthority)) -> Self {
        let desk = Self {
            broker,
            authority,
            runtime: Builder::new_current_thread().build().unwrap(),
        };
        desk.broker.set_calendar(kr_calendar());
        desk.broker
            .set_reference_price(Symbol::new(SAM), d("70000"), monday());
        desk.broker
            .set_reference_price(Symbol::new(HYN), d("30000"), monday());
        desk
    }

    /// 달력과 기준가를 넣은 새 계좌.
    fn open(config: PaperConfig) -> Self {
        Self::wrap(PaperBroker::open(config).unwrap())
    }

    /// 상태 값에서 되살린 뒤 달력과 기준가를 다시 넣는다.
    fn restore(config: PaperConfig, snapshot: Snapshot) -> Result<Self, BrokerError> {
        PaperBroker::restore(config, snapshot).map(Self::wrap)
    }

    /// 상태를 글자(JSON)로 내보냈다가 그 글자에서 되살린다.
    fn reopen(&self, config: PaperConfig) -> Self {
        let text = serde_json::to_string(&self.broker.snapshot()).unwrap();
        Self::restore(config, serde_json::from_str(&text).unwrap()).unwrap()
    }

    /// 국내 주문을 승인한 표시를 붙여 낸다. 값이 없으면 시장가다.
    fn place(
        &self,
        code: &str,
        quantity: &str,
        price: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<OrderAck, BrokerError> {
        let order_type = if price.is_some() {
            OrderType::Limit
        } else {
            OrderType::Market
        };
        let approved = ApprovedOrder {
            market: Market::Kr,
            symbol: Symbol::new(code),
            side: Side::Buy,
            order_type,
            quantity: d(quantity),
            price: price.map(d),
            account_kind: AccountKind::Paper,
        };
        let request = OrderRequest {
            client_order_id: None,
            market: Market::Kr,
            symbol: Symbol::new(code),
            side: Side::Buy,
            order_type,
            quantity: d(quantity),
            price: price.map(d),
            account_kind: AccountKind::Paper,
            timeout: UnfilledTimeout::LONG_TERM,
            confirm_high_value: false,
            permit: Some(self.authority.approve(approved)),
        };
        let broker: &dyn Broker = &self.broker;
        self.runtime.block_on(broker.place_order(request, now))
    }

    /// 지정가 사기를 내고 식별자를 돌려준다.
    fn buy(&self, quantity: &str, price: &str, now: DateTime<Utc>) -> OrderId {
        self.place(SAM, quantity, Some(price), now)
            .unwrap()
            .order_id
    }

    fn on_quote(&self, quote: &Quote, now: DateTime<Utc>) {
        self.broker.on_quote(quote, now);
    }

    /// 그 시각에 본 주문의 체결 수량.
    fn filled(&self, order_id: &OrderId, now: DateTime<Utc>) -> Decimal {
        let broker: &dyn Broker = &self.broker;
        self.runtime
            .block_on(broker.order(order_id, now))
            .unwrap()
            .execution
            .filled_quantity
    }

    fn krw(&self, now: DateTime<Utc>) -> CashBalance {
        self.broker.cash(&Currency::Krw, now)
    }
}

/// 상태 값을 글자(JSON) 값으로.
fn json(snapshot: &Snapshot) -> Value {
    serde_json::to_value(snapshot).unwrap()
}

/// 글자(JSON) 값을 상태 값으로.
fn snapshot_of(value: Value) -> Snapshot {
    serde_json::from_value(value).unwrap()
}

/// 상태 값의 `ledger.trade_marks`.
fn marks(value: &mut Value) -> &mut Vec<Value> {
    value["ledger"]["trade_marks"].as_array_mut().unwrap()
}

/// 그 종목의 기억에 든 값들(받은 순서).
fn mark_prices(snapshot: &Snapshot, code: &str) -> Vec<Decimal> {
    let mut value = json(snapshot);
    let mark = marks(&mut value)
        .iter()
        .find(|mark| mark["symbol"] == code)
        .unwrap_or_else(|| panic!("{code}의 기억이 없다"))
        .clone();
    mark["prices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|price| price.as_str().unwrap().parse().unwrap())
        .collect()
}

// ───────────── 1. 먼 앞날 거래소 시각은 버린다 ─────────────

// 검증: E25, N6
#[test]
fn 거래소_시각이_앞서도_되는_시간은_5분이다() {
    assert_eq!(MAX_EXCHANGE_LEAD, Duration::from_secs(300));
}

// 검증: E25, N6
#[test]
fn 먼_앞날_거래소_시각이_한_번_와도_그_종목의_뒤_시세로_체결한다() {
    let desk = Desk::open(config());
    let order_id = desk.buy("1", "70000", at(0));
    // 거래소 시각이 받은 시각보다 1시간 앞선 시세(지정가보다 비싸 체결되지 않는다).
    desk.on_quote(
        &quote(
            SAM,
            "71000",
            Some(at(10) + chrono::Duration::hours(1)),
            at(10),
        ),
        at(10),
    );
    // 바르게 온 다음 시세.
    desk.on_quote(&quote(SAM, "69000", Some(at(11)), at(11)), at(11));
    assert_eq!(
        desk.filled(&order_id, at(11)),
        d("1"),
        "한 번 잘못 온 먼 앞날 시각이 그 종목의 체결을 막았다"
    );
}

// 검증: E25, N6
#[test]
fn 거래소_시각이_5분을_1초라도_넘게_앞서면_없는_것과_같다() {
    let desk = Desk::open(config());
    let order_id = desk.buy("1", "70000", at(0));
    let too_far = at(10) + lead() + chrono::Duration::seconds(1);
    desk.on_quote(&quote(SAM, "71000", Some(too_far), at(10)), at(10));
    desk.on_quote(&quote(SAM, "69000", Some(at(11)), at(11)), at(11));
    assert_eq!(desk.filled(&order_id, at(11)), d("1"));
}

// 검증: E25, N6
#[test]
fn 거래소_시각이_딱_5분_앞선_것은_거래소_시각으로_쓴다() {
    let desk = Desk::open(config());
    let order_id = desk.buy("1", "70000", at(0));
    let edge = at(10) + lead();
    desk.on_quote(&quote(SAM, "71000", Some(edge), at(10)), at(10));
    // 딱 5분 앞선 시각은 그 종목의 가장 늦은 거래소 시각이 되어, 그보다 앞인 시세는 옛 시세다.
    desk.on_quote(&quote(SAM, "69000", Some(at(11)), at(11)), at(11));
    assert_eq!(
        desk.filled(&order_id, at(11)),
        d("0"),
        "딱 5분 앞선 거래소 시각은 버리지 않는다"
    );
    // 그보다 뒤의 거래소 시각(받은 시각보다 5분 안쪽으로 앞섬)으로는 체결된다.
    let later = edge + chrono::Duration::seconds(1);
    desk.on_quote(&quote(SAM, "69000", Some(later), at(12)), at(12));
    assert_eq!(desk.filled(&order_id, at(12)), d("1"));
}

// 검증: E25, N6
#[test]
fn 먼_앞날_거래소_시각의_시세는_시각_없는_시세처럼_체결에_쓴다() {
    let desk = Desk::open(config());
    let order_id = desk.buy("1", "70000", at(0));
    desk.on_quote(
        &quote(
            SAM,
            "69000",
            Some(at(1) + chrono::Duration::hours(1)),
            at(1),
        ),
        at(1),
    );
    assert_eq!(
        desk.filled(&order_id, at(1)),
        d("1"),
        "주문 뒤에 받은 시세다"
    );
}

// 검증: E25, N6
#[test]
fn 먼_앞날_거래소_시각이_와도_뒤_시세를_마지막_시세로_기억한다() {
    // 같은 바른 시세를 받은 두 계좌 가운데 하나만 먼저 먼 앞날 시각의 시세를 받는다.
    // 시장가 사기가 묶는 돈이 같아야 한다(뒤의 60,000원으로 셈한다).
    let given = Desk::open(config());
    let control = Desk::open(config());
    given.on_quote(
        &quote(
            SAM,
            "71000",
            Some(at(10) + chrono::Duration::hours(1)),
            at(10),
        ),
        at(10),
    );
    for desk in [&given, &control] {
        desk.on_quote(&quote(SAM, "60000", Some(at(11)), at(11)), at(11));
    }
    let ordered = given.place(SAM, "10", None, at(12));
    let expected = control.place(SAM, "10", None, at(12));
    assert!(expected.is_ok(), "{expected:?}");
    assert!(ordered.is_ok(), "{ordered:?}");
    assert_eq!(
        given.krw(at(12)),
        control.krw(at(12)),
        "먼 앞날 시각 때문에 뒤의 시세를 옛 시세로 버렸다"
    );
}

// 검증: E25, N6, V6
#[test]
fn 먼_앞날_거래소_시각을_받은_계좌도_되살리고_뒤_시세로_체결한다() {
    let desk = Desk::open(config());
    let order_id = desk.buy("1", "70000", at(0));
    desk.on_quote(
        &quote(
            SAM,
            "71000",
            Some(at(10) + chrono::Duration::hours(1)),
            at(10),
        ),
        at(10),
    );
    let reopened = desk.reopen(config());
    reopened.on_quote(&quote(SAM, "69000", Some(at(11)), at(11)), at(11));
    assert_eq!(reopened.filled(&order_id, at(11)), d("1"));
}

// ───────────── 2. 모양 3 → 4로 올려도 쓴 거래를 다시 쓰지 않는다 ─────────────

/// 상태 값을 모양 3으로 바꾼다: 모양 번호를 3으로, 모양 4에서 더한 `ledger.trade_marks`를 뺀다.
fn as_shape_three(snapshot: &Snapshot) -> Snapshot {
    let mut value = json(snapshot);
    value["version"] = Value::from(3);
    value["ledger"]
        .as_object_mut()
        .unwrap()
        .remove("trade_marks");
    snapshot_of(value)
}

// 검증: E25, N6, V6
#[test]
fn 모양_3에서_올린_계좌는_같은_시각의_쓴_거래를_다시_쓰지_않는다() {
    let desk = Desk::open(one_per_quote());
    let order_id = desk.buy("10", "70000", at(0));
    desk.on_quote(&quote(SAM, "69900", Some(at(5)), at(5)), at(5));
    assert_eq!(desk.filled(&order_id, at(5)), d("1"));
    let current = desk.broker.snapshot();

    let restored = Desk::restore(one_per_quote(), as_shape_three(&current))
        .expect("모양 3 상태 값은 올려서 되살린다");
    assert_eq!(
        json(&restored.broker.snapshot())["version"],
        json(&current)["version"],
        "되살린 계좌는 지금 모양으로 내보낸다"
    );

    // 5초의 69,900원 거래는 이미 썼다.
    restored.on_quote(&quote(SAM, "69900", Some(at(5)), at(7)), at(7));
    assert_eq!(
        restored.filled(&order_id, at(7)),
        d("1"),
        "모양 3에서 올린 뒤 같은 시각의 쓴 거래로 다시 체결됐다"
    );
    // 5초의 다른 값은 처음 보는 거래다.
    restored.on_quote(&quote(SAM, "69800", Some(at(5)), at(8)), at(8));
    assert_eq!(restored.filled(&order_id, at(8)), d("2"));
    // 5초보다 앞인 시세는 옛 시세다.
    restored.on_quote(&quote(SAM, "69000", Some(at(4)), at(9)), at(9));
    assert_eq!(restored.filled(&order_id, at(9)), d("2"));
}

// ───────────── 3. 같은 시각 값 목록의 상한 ─────────────

/// 같은 거래소 시각(5초)의 `i`번째 거래 값: 60,000원부터 100원씩.
fn nth_price(i: usize) -> String {
    (60_000 + 100 * i).to_string()
}

/// 한 시세에 1주까지 체결되는 계좌에서 100주 사기를 내고, 같은 거래소 시각의 서로 다른 값
/// `MAX_SAME_TIME_PRICES + 1`개를 차례로 받는다. 받은 시각은 6초부터 1초씩.
fn over_capacity() -> (Desk, OrderId, DateTime<Utc>) {
    let desk = Desk::open(one_per_quote());
    let order_id = desk.buy("100", "70000", at(0));
    let count = MAX_SAME_TIME_PRICES + 1;
    for i in 0..count {
        let now = at(6 + i as i64);
        desk.on_quote(&quote(SAM, &nth_price(i), Some(at(5)), now), now);
    }
    let last = at(6 + count as i64 - 1);
    assert_eq!(
        desk.filled(&order_id, last),
        Decimal::from(count),
        "서로 다른 거래는 저마다 한 번씩 쓴다"
    );
    (desk, order_id, last)
}

// 검증: E25, N6
#[test]
fn 같은_시각_값_목록의_상한은_64개다() {
    assert_eq!(MAX_SAME_TIME_PRICES, 64);
}

// 검증: E25, N6, V6
#[test]
fn 같은_시각_값이_상한을_넘으면_가장_오래_기억한_것부터_버린다() {
    let (desk, _, _) = over_capacity();
    let prices = mark_prices(&desk.broker.snapshot(), SAM);
    let expected: Vec<Decimal> = (1..=MAX_SAME_TIME_PRICES)
        .map(|i| d(&nth_price(i)))
        .collect();
    assert_eq!(
        prices, expected,
        "상한만큼, 가장 오래 기억한 첫 값을 버리고 받은 순서대로 남는다"
    );
}

// 검증: E25, N6
#[test]
fn 상한을_넘어_버린_값만_다시_처음_보는_거래가_된다() {
    let (desk, order_id, last) = over_capacity();
    let count = Decimal::from(MAX_SAME_TIME_PRICES + 1);
    // 남아 있는 값(마지막 · 두 번째)으로는 다시 체결하지 않는다.
    let mut now = last;
    for i in [MAX_SAME_TIME_PRICES, 1] {
        now += chrono::Duration::seconds(1);
        desk.on_quote(&quote(SAM, &nth_price(i), Some(at(5)), now), now);
        assert_eq!(
            desk.filled(&order_id, now),
            count,
            "{i}번째 값은 아직 기억에 있다"
        );
    }
    // 버린 첫 값은 다시 처음 보는 거래다.
    now += chrono::Duration::seconds(1);
    desk.on_quote(&quote(SAM, &nth_price(0), Some(at(5)), now), now);
    assert_eq!(desk.filled(&order_id, now), count + Decimal::ONE);
}

// 검증: E25, N6, V6
#[test]
fn 상한까지_찬_기억도_되살린_뒤에_그대로다() {
    let (desk, order_id, last) = over_capacity();
    let count = Decimal::from(MAX_SAME_TIME_PRICES + 1);
    let reopened = desk.reopen(one_per_quote());
    let now = last + chrono::Duration::seconds(1);
    reopened.on_quote(&quote(SAM, &nth_price(1), Some(at(5)), now), now);
    assert_eq!(reopened.filled(&order_id, now), count);
    let now = now + chrono::Duration::seconds(1);
    reopened.on_quote(&quote(SAM, &nth_price(0), Some(at(5)), now), now);
    assert_eq!(reopened.filled(&order_id, now), count + Decimal::ONE);
}

// ───────────── 4. 되살릴 때 기억 목록을 검사한다 ─────────────

/// 두 종목(하이닉스 · 삼성)의 거래소 시각이 있는 시세를 받은 계좌의 상태 값.
/// 기억 목록은 종목 순서로 [하이닉스(6초), 삼성(5초)]다.
fn two_marks() -> Value {
    let desk = Desk::open(config());
    desk.on_quote(&quote(SAM, "69900", Some(at(5)), at(5)), at(5));
    desk.on_quote(&quote(HYN, "29500", Some(at(6)), at(6)), at(6));
    let mut value = json(&desk.broker.snapshot());
    let symbols: Vec<Value> = marks(&mut value)
        .iter()
        .map(|mark| mark["symbol"].clone())
        .collect();
    assert_eq!(symbols, [Value::from(HYN), Value::from(SAM)]);
    value
}

fn restored(value: Value) -> Result<Desk, BrokerError> {
    Desk::restore(config(), snapshot_of(value))
}

fn assert_invalid(result: Result<Desk, BrokerError>, why: &str) {
    match result {
        Err(BrokerError::InvalidSnapshot(_)) => {}
        Err(other) => panic!("{why}: 다른 오류다: {other:?}"),
        Ok(_) => panic!("{why}: 되살렸다"),
    }
}

// 검증: V6
#[test]
fn 기억_목록이_바르면_되살린다() {
    assert!(restored(two_marks()).is_ok());
}

// 검증: V6
#[test]
fn 기억_목록에_같은_종목_같은_통화가_둘이면_되살리지_않는다() {
    let mut value = two_marks();
    let list = marks(&mut value);
    // 정렬은 지킨 채(같은 것을 바로 뒤에) 하이닉스를 하나 더 둔다.
    let copy = list[0].clone();
    list.insert(1, copy);
    assert_invalid(restored(value), "종목 · 통화가 겹치는 기억 목록");
}

// 검증: V6
#[test]
fn 기억_목록이_종목_순서가_아니면_되살리지_않는다() {
    let mut value = two_marks();
    marks(&mut value).reverse();
    assert_invalid(restored(value), "정렬되지 않은 기억 목록");
}

// 검증: V6
#[test]
fn 기억한_시각이_마지막_시세의_거래소_시각보다_앞이면_되살리지_않는다() {
    let mut value = two_marks();
    // 삼성의 마지막 시세는 거래소 시각 5초다. 기억한 시각을 4초로 당긴다.
    marks(&mut value)[1]["traded_at"] = serde_json::to_value(at(4)).unwrap();
    assert_invalid(
        restored(value),
        "기억한 시각이 마지막 시세의 거래소 시각보다 앞",
    );
}

// 검증: V6
#[test]
fn 마지막_시세에_거래소_시각이_없으면_기억한_시각이_앞서도_되살린다() {
    // 거래소 시각 5초의 시세 뒤에 시각 없는 시세가 마지막이 된다. 기억한 시각(5초)은 남는다.
    let desk = Desk::open(config());
    desk.on_quote(&quote(SAM, "69900", Some(at(5)), at(5)), at(5));
    desk.on_quote(&quote(SAM, "70000", None, at(6)), at(6));
    assert!(restored(json(&desk.broker.snapshot())).is_ok());
}
