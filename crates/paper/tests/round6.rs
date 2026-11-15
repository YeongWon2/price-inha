//! 6차 작업 1~3 (`docs/dev/modules/paper.md` "6차 작업").
//!
//! 1. 되살릴 때 먼 앞날 기억을 거부한다: 기억한 거래 시각(`ledger.trade_marks[].traded_at`)이 상태 값의
//!    마지막 시각(`ledger.last_now`) + [`MAX_EXCHANGE_LEAD`]보다 늦으면 `InvalidSnapshot`이다.
//! 2. 되살릴 때 같은 시각 값이 [`MAX_SAME_TIME_PRICES`]개를 넘으면 거부, 거래소 시각이 있는 마지막
//!    시세인데 그 종목의 기억이 없으면 통과.
//! 3. 받은 시각보다 5분 넘게 앞선 거래소 시각의 시세를 섞어도 돈 · 수량 불변식이 지켜진다.
//!
//! 이 파일은 기존 묶음(`suite.rs`)과 따로 도는 테스트 실행 파일이라 도우미와 생성기를 안에 둔다.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{
    Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol, TradingDay,
};
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, BrokerError, CashBalance, CheckAuthority, InitialCash,
    LimitFillPrice, MAX_EXCHANGE_LEAD, MAX_SAME_TIME_PRICES, MarketRules, MaxFill, MoneyRounding,
    Order, OrderAck, OrderGroup, OrderId, OrderRequest, OrderType, PaperBroker, PaperConfig,
    ProceedsUsable, RoundingMode, SessionRules, Settlement, Side, Snapshot, TickBand,
    UnfilledTimeout,
};
use proptest::prelude::{
    Just, ProptestConfig, Strategy, any, prop_assert, prop_assert_eq, prop_oneof, proptest,
};
use proptest::test_runner::TestCaseError;
use rust_decimal::Decimal;
use serde_json::Value;
use tokio::runtime::{Builder, Runtime};

// ───────────── 도우미 ─────────────

/// 삼성전자. 기준가 70,000원(호가 단위 100원).
const SAM: &str = "005930";
/// SK하이닉스 자리. 기준가 30,000원(호가 단위 50원).
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

/// 테스트 설정. 원화 10,000,000 · 달러 10,000. 기본값에 기대지 않게 숫자를 모두 적는다.
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
        Self::restore(config, serde_json::from_str(&text).unwrap())
            .expect("이 계좌가 내보낸 상태 값은 되살린다")
    }

    fn broker(&self) -> &dyn Broker {
        &self.broker
    }

    /// 국내 주문을 승인한 표시를 붙여 낸다. 값이 없으면 시장가다.
    fn place(
        &self,
        code: &str,
        side: Side,
        quantity: &str,
        price: Option<&str>,
        timeout: UnfilledTimeout,
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
            side,
            order_type,
            quantity: d(quantity),
            price: price.map(d),
            account_kind: AccountKind::Paper,
        };
        let request = OrderRequest {
            client_order_id: None,
            market: Market::Kr,
            symbol: Symbol::new(code),
            side,
            order_type,
            quantity: d(quantity),
            price: price.map(d),
            account_kind: AccountKind::Paper,
            timeout,
            confirm_high_value: false,
            permit: Some(self.authority.approve(approved)),
        };
        self.runtime
            .block_on(self.broker().place_order(request, now))
    }

    fn cancel(&self, order_id: &OrderId, now: DateTime<Utc>) -> Result<OrderId, BrokerError> {
        self.runtime
            .block_on(self.broker().cancel_order(order_id, now))
    }

    fn orders(&self, group: OrderGroup, now: DateTime<Utc>) -> Vec<Order> {
        self.runtime
            .block_on(self.broker().orders(group, None, now))
            .unwrap()
    }

    fn holding(&self, code: &str, now: DateTime<Utc>) -> Decimal {
        self.runtime
            .block_on(self.broker().holdings(Some(&Symbol::new(code)), now))
            .unwrap()
            .into_iter()
            .next()
            .map_or(Decimal::ZERO, |holding| holding.quantity)
    }

    fn sellable(&self, code: &str, now: DateTime<Utc>) -> Decimal {
        self.runtime
            .block_on(self.broker().sellable_quantity(&Symbol::new(code), now))
            .unwrap()
    }

    fn cash(&self, currency: Currency, now: DateTime<Utc>) -> CashBalance {
        self.broker.cash(&currency, now)
    }
}

/// 상태 값을 글자(JSON) 값으로.
fn json(snapshot: &Snapshot) -> Value {
    serde_json::to_value(snapshot).unwrap()
}

/// 상태 값의 `ledger.trade_marks`.
fn marks(value: &mut Value) -> &mut Vec<Value> {
    value["ledger"]["trade_marks"].as_array_mut().unwrap()
}

/// 상태 값의 마지막 시각(`ledger.last_now`).
fn last_now(value: &Value) -> DateTime<Utc> {
    serde_json::from_value(value["ledger"]["last_now"].clone()).unwrap()
}

/// 두 종목(하이닉스 · 삼성)의 거래소 시각이 있는 시세를 받은 계좌의 상태 값.
/// 기억 목록은 종목 순서로 [하이닉스(6초), 삼성(5초)]이고, 마지막 시각은 6초다.
fn two_marks() -> Value {
    let desk = Desk::open(config());
    desk.broker
        .on_quote(&quote(SAM, "69900", Some(at(5)), at(5)), at(5));
    desk.broker
        .on_quote(&quote(HYN, "29500", Some(at(6)), at(6)), at(6));
    let mut value = json(&desk.broker.snapshot());
    let symbols: Vec<Value> = marks(&mut value)
        .iter()
        .map(|mark| mark["symbol"].clone())
        .collect();
    assert_eq!(symbols, [Value::from(HYN), Value::from(SAM)]);
    assert_eq!(last_now(&value), at(6));
    value
}

fn restored(value: Value) -> Result<Desk, BrokerError> {
    Desk::restore(config(), serde_json::from_value(value).unwrap())
}

fn assert_invalid(result: Result<Desk, BrokerError>, why: &str) {
    match result {
        Err(BrokerError::InvalidSnapshot(_)) => {}
        Err(other) => panic!("{why}: 다른 오류다: {other:?}"),
        Ok(_) => panic!("{why}: 되살렸다"),
    }
}

/// 삼성 기억(목록의 두 번째)의 거래 시각을 마지막 시각 + 5분 + `extra`로 옮긴 상태 값.
fn sam_mark_ahead(extra: chrono::Duration) -> Value {
    let mut value = two_marks();
    let ahead = last_now(&value) + lead() + extra;
    marks(&mut value)[1]["traded_at"] = serde_json::to_value(ahead).unwrap();
    value
}

// ───────────── 1. 되살릴 때 먼 앞날 기억을 거부한다 ─────────────

// 검증: V6, N6
#[test]
fn 기억한_거래_시각이_마지막_시각_더하기_5분을_1초라도_넘으면_되살리지_않는다() {
    assert_invalid(
        restored(sam_mark_ahead(chrono::Duration::seconds(1))),
        "기억한 거래 시각이 마지막 시각 + 5분보다 늦음",
    );
}

// 검증: V6, N6
#[test]
fn 기억한_거래_시각이_한참_먼_앞날이면_되살리지_않는다() {
    assert_invalid(
        restored(sam_mark_ahead(chrono::Duration::days(1))),
        "기억한 거래 시각이 하루 넘게 앞섬",
    );
}

// 검증: V6, N6
#[test]
fn 기억한_거래_시각이_딱_마지막_시각_더하기_5분이면_되살린다() {
    assert!(restored(sam_mark_ahead(chrono::Duration::zero())).is_ok());
}

// 검증: V6, N6
#[test]
fn 받은_시각보다_딱_5분_앞선_거래소_시각을_기억한_계좌도_되살린다() {
    // 지금 코드가 만들 수 있는 가장 앞선 기억(받은 시각 + 5분)은 거부하지 않는다.
    let desk = Desk::open(config());
    desk.broker
        .on_quote(&quote(SAM, "69900", Some(at(5) + lead()), at(5)), at(5));
    assert!(restored(json(&desk.broker.snapshot())).is_ok());
}

// ───────────── 2. 기억 목록의 개수 상한과 빠진 기억 ─────────────

/// 삼성 기억의 값 목록을 `count`개의 서로 다른 값(60,000원부터 100원씩)으로 바꾼 상태 값.
fn sam_mark_with_prices(count: usize) -> Value {
    let mut value = two_marks();
    let prices: Vec<Value> = (0..count)
        .map(|i| Value::from((60_000 + 100 * i).to_string()))
        .collect();
    marks(&mut value)[1]["prices"] = Value::from(prices);
    value
}

// 검증: V6
#[test]
fn 같은_시각_값이_64개를_넘는_기억은_되살리지_않는다() {
    assert_invalid(
        restored(sam_mark_with_prices(MAX_SAME_TIME_PRICES + 1)),
        "같은 시각 값이 상한을 넘음",
    );
}

// 검증: V6
#[test]
fn 같은_시각_값이_딱_64개인_기억은_되살린다() {
    assert!(restored(sam_mark_with_prices(MAX_SAME_TIME_PRICES)).is_ok());
}

// 검증: V6
#[test]
fn 거래소_시각이_있는_마지막_시세에_그_종목의_기억이_없어도_되살린다() {
    let mut value = two_marks();
    // 삼성의 마지막 시세는 거래소 시각 5초다. 삼성의 기억만 뺀다.
    marks(&mut value).retain(|mark| mark["symbol"] != SAM);
    assert!(restored(value).is_ok());
}

// ───────────── 3. 먼 앞날 시세를 섞어도 돈 · 수량이 어긋나지 않는다 ─────────────

/// 섞어 볼 동작 하나.
#[derive(Debug, Clone)]
enum Op {
    /// 지정가 주문. 값 = 기준가 + `step` × 호가 단위. `scalp`면 1분 뒤 마감.
    Limit {
        hynix: bool,
        selling: bool,
        quantity: u8,
        step: i8,
        scalp: bool,
    },
    /// 시장가 주문.
    Market {
        hynix: bool,
        selling: bool,
        quantity: u8,
    },
    /// 방금 받은 시세(거래소 시각 = 받은 시각).
    Quote { hynix: bool, step: i8 },
    /// 받은 시각보다 5분 + `beyond`초 앞선 거래소 시각의 시세(먼 앞날).
    FarQuote { hynix: bool, step: i8, beyond: u16 },
    /// 지금까지 낸 주문 가운데 하나를 취소해 본다.
    Cancel { pick: u8 },
    /// 시간이 흐른다(계좌에 알린다).
    Wait { seconds: u8 },
    /// 상태를 내보냈다가 되살린 계좌로 갈아탄다.
    Reopen,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (any::<bool>(), any::<bool>(), 1u8..=12, -20i8..=20, any::<bool>()).prop_map(
            |(hynix, selling, quantity, step, scalp)| Op::Limit {
                hynix,
                selling,
                quantity,
                step,
                scalp,
            }
        ),
        2 => (any::<bool>(), any::<bool>(), 1u8..=12).prop_map(|(hynix, selling, quantity)| {
            Op::Market {
                hynix,
                selling,
                quantity,
            }
        }),
        4 => (any::<bool>(), -20i8..=20).prop_map(|(hynix, step)| Op::Quote { hynix, step }),
        4 => (any::<bool>(), -20i8..=20, 1u16..=3600)
            .prop_map(|(hynix, step, beyond)| Op::FarQuote { hynix, step, beyond }),
        2 => any::<u8>().prop_map(|pick| Op::Cancel { pick }),
        2 => (1u8..=45).prop_map(|seconds| Op::Wait { seconds }),
        1 => Just(Op::Reopen),
    ]
}

fn code(hynix: bool) -> &'static str {
    if hynix { HYN } else { SAM }
}

/// 기준가에서 호가 단위로 `step`칸 떨어진 값. 삼성 68,000~72,000, 하이닉스 29,000~31,000.
fn price(hynix: bool, step: i8) -> String {
    let (base, tick) = if hynix { (30_000, 50) } else { (70_000, 100) };
    (base + i64::from(step) * tick).to_string()
}

/// 동작을 차례로 돌리는 쪽. 시각은 뒤로 가지 않는다.
struct Runner {
    desk: Desk,
    seconds: i64,
    placed: Vec<OrderId>,
}

impl Runner {
    fn now(&self) -> DateTime<Utc> {
        at(self.seconds)
    }

    fn apply(&mut self, op: &Op) {
        // 동작마다 1초씩 흐른다.
        self.seconds += 1;
        let now = self.now();
        match *op {
            Op::Limit {
                hynix,
                selling,
                quantity,
                step,
                scalp,
            } => {
                let side = if selling { Side::Sell } else { Side::Buy };
                let timeout = if scalp {
                    UnfilledTimeout::SCALP
                } else {
                    UnfilledTimeout::LONG_TERM
                };
                let price = price(hynix, step);
                let quantity = quantity.to_string();
                // 거부돼도 된다(돈 · 수량이 모자람 등).
                if let Ok(ack) =
                    self.desk
                        .place(code(hynix), side, &quantity, Some(&price), timeout, now)
                {
                    self.placed.push(ack.order_id);
                }
            }
            Op::Market {
                hynix,
                selling,
                quantity,
            } => {
                let side = if selling { Side::Sell } else { Side::Buy };
                let quantity = quantity.to_string();
                if let Ok(ack) = self.desk.place(
                    code(hynix),
                    side,
                    &quantity,
                    None,
                    UnfilledTimeout::LONG_TERM,
                    now,
                ) {
                    self.placed.push(ack.order_id);
                }
            }
            Op::Quote { hynix, step } => {
                self.desk.broker.on_quote(
                    &quote(code(hynix), &price(hynix, step), Some(now), now),
                    now,
                );
            }
            Op::FarQuote {
                hynix,
                step,
                beyond,
            } => {
                let traded_at = now + lead() + chrono::Duration::seconds(i64::from(beyond));
                self.desk.broker.on_quote(
                    &quote(code(hynix), &price(hynix, step), Some(traded_at), now),
                    now,
                );
            }
            Op::Cancel { pick } => {
                if !self.placed.is_empty() {
                    let index = usize::from(pick) % self.placed.len();
                    // 이미 끝난 주문이면 거부된다. 그래도 된다.
                    let _ = self.desk.cancel(&self.placed[index], now);
                }
            }
            Op::Wait { seconds } => {
                self.seconds += i64::from(seconds);
                self.desk.broker.on_time(self.now());
            }
            Op::Reopen => {
                self.desk = self.desk.reopen(config());
            }
        }
    }
}

/// 돈과 수량이 어긋나지 않았는지 공개 API만으로 확인한다.
fn check(desk: &Desk, now: DateTime<Utc>) -> Result<(), TestCaseError> {
    let zero = Decimal::ZERO;
    let initial = config().initial_cash;
    let cash = desk.cash(Currency::Krw, now);
    prop_assert!(cash.available >= zero, "쓸 수 있는 돈이 음수: {cash:?}");
    prop_assert!(cash.held >= zero, "묶인 돈이 음수: {cash:?}");
    prop_assert!(cash.unsettled >= zero, "결제 전인 돈이 음수: {cash:?}");
    prop_assert_eq!(desk.cash(Currency::Usd, now).total(), initial.usd);

    let open = desk.orders(OrderGroup::Open, now);
    let closed = desk.orders(OrderGroup::Closed, now);
    let ordered_at: BTreeMap<String, DateTime<Utc>> = open
        .iter()
        .chain(closed.iter())
        .map(|order| (order.order_id.as_str().to_owned(), order.ordered_at))
        .collect();

    // 돈은 체결로만 움직인다: 처음 돈 − 산 것(금액 + 수수료) + 판 것(금액 − 수수료 − 세금).
    let mut expected = initial.krw;
    let mut net: BTreeMap<String, Decimal> = BTreeMap::new();
    let mut filled: BTreeMap<String, Decimal> = BTreeMap::new();
    for fill in desk.broker.fills() {
        prop_assert!(fill.quantity > zero);
        prop_assert_eq!(fill.amount, fill.quantity * fill.price);
        prop_assert!(fill.commission >= zero && fill.tax >= zero);
        // 주문 뒤에 받은 시세로만 체결된다.
        let placed_at = ordered_at.get(fill.order_id.as_str()).copied();
        prop_assert!(
            placed_at.is_some_and(|placed_at| fill.filled_at > placed_at),
            "주문 전의 시세로 체결됐다: {:?}",
            fill
        );
        let position = net.entry(fill.symbol.as_str().to_owned()).or_default();
        match fill.side {
            Side::Buy => {
                expected -= fill.amount + fill.commission;
                *position += fill.quantity;
            }
            Side::Sell => {
                expected += fill.amount - fill.commission - fill.tax;
                *position -= fill.quantity;
            }
        }
        *filled.entry(fill.order_id.as_str().to_owned()).or_default() += fill.quantity;
    }
    prop_assert_eq!(cash.total(), expected, "돈이 생기거나 사라졌다");

    for order in open.iter().chain(closed.iter()) {
        let done = order.execution.filled_quantity;
        prop_assert!(done >= zero && done <= order.quantity, "{:?}", order);
        let recorded = filled.get(order.order_id.as_str()).copied().unwrap_or(zero);
        prop_assert_eq!(done, recorded, "체결 기록과 주문의 체결 수량이 다르다");
    }

    // 종목마다: 보유 = 산 수량 − 판 수량, 묶인 수량 = 열린 파는 주문의 남은 수량.
    for symbol in [SAM, HYN] {
        let owned = net.get(symbol).copied().unwrap_or(zero);
        prop_assert!(owned >= zero, "없는 주식을 팔았다: {} {}", symbol, owned);
        let holding = desk.holding(symbol, now);
        prop_assert_eq!(holding, owned, "보유가 체결 기록과 다르다: {}", symbol);
        let locked: Decimal = open
            .iter()
            .filter(|order| order.symbol.as_str() == symbol && order.side == Side::Sell)
            .map(|order| order.quantity - order.execution.filled_quantity)
            .sum();
        prop_assert_eq!(
            holding - desk.sellable(symbol, now),
            locked,
            "묶인 수량이 다르다: {}",
            symbol
        );
    }
    Ok(())
}

proptest! {
    // 실패한 씨앗을 테스트 폴더에 파일로 남기지 않는다(잠긴 폴더가 바뀌면 안 된다).
    #![proptest_config(ProptestConfig {
        cases: 96,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    // 검증: C10, N6, V6
    #[test]
    fn 먼_앞날_시세를_섞어도_돈과_수량이_어긋나지_않는다(
        ops in proptest::collection::vec(op(), 1..60),
    ) {
        let mut runner = Runner {
            desk: Desk::open(config()),
            seconds: 0,
            placed: Vec::new(),
        };
        check(&runner.desk, runner.now())?;
        for op in &ops {
            runner.apply(op);
            check(&runner.desk, runner.now())?;
        }

        // 끝: 남은 주문을 모두 취소하면 묶인 것이 하나도 없다.
        let now = runner.now();
        for order in runner.desk.orders(OrderGroup::Open, now) {
            runner.desk.cancel(&order.order_id, now).unwrap();
        }
        check(&runner.desk, now)?;
        prop_assert_eq!(runner.desk.cash(Currency::Krw, now).held, Decimal::ZERO);
        for symbol in [SAM, HYN] {
            prop_assert_eq!(runner.desk.sellable(symbol, now), runner.desk.holding(symbol, now));
        }
    }
}
