//! 7차 작업 1 · 2 (`docs/dev/modules/paper.md` "7차 작업").
//!
//! 1. 마지막 시각(`ledger.last_now`)이 없는데 거래 기억(`ledger.trade_marks`)이 있는 상태 값은
//!    마지막 시세가 있는 경우처럼 `InvalidSnapshot`으로 거부한다(실행 중에는 만들어지지 않는 손상 값).
//! 2. 받은 시각보다 [`MAX_EXCHANGE_LEAD`] 넘게 앞선 거래소 시각의 시세(먼 앞날 시세)는 "거래소 시각이
//!    없는 시세"로 체결되고, 그 뒤 거래소 시각이 정상인 시세가 막히지 않는다(속성 테스트로 직접 단언).
//!
//! 이 파일은 기존 묶음(`suite.rs`)과 따로 도는 테스트 실행 파일이라 도우미와 생성기를 안에 둔다.

use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{
    Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol, TradingDay,
};
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, BrokerError, CheckAuthority, Fill, InitialCash,
    LimitFillPrice, MAX_EXCHANGE_LEAD, MarketRules, MaxFill, MoneyRounding, Order, OrderAck,
    OrderGroup, OrderId, OrderRequest, OrderType, PaperBroker, PaperConfig, ProceedsUsable,
    RoundingMode, SessionRules, Settlement, Side, Snapshot, TickBand, UnfilledTimeout,
};
use proptest::prelude::{
    Just, ProptestConfig, Strategy, any, prop_assert, prop_assert_eq, prop_oneof, proptest,
};
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
    fn open() -> Self {
        Self::wrap(PaperBroker::open(config()).unwrap())
    }

    /// 상태 값에서 되살린 뒤 달력과 기준가를 다시 넣는다.
    fn restore(snapshot: Snapshot) -> Result<Self, BrokerError> {
        PaperBroker::restore(config(), snapshot).map(Self::wrap)
    }

    /// 상태를 글자(JSON)로 내보냈다가 그 글자에서 되살린다.
    fn reopen(&self) -> Self {
        let text = serde_json::to_string(&self.broker.snapshot()).unwrap();
        Self::restore(serde_json::from_str(&text).unwrap())
            .expect("이 계좌가 내보낸 상태 값은 되살린다")
    }

    /// 국내 지정가 주문을 승인한 표시를 붙여 낸다(정규장 동안 남는 주문).
    fn limit(
        &self,
        code: &str,
        side: Side,
        quantity: &str,
        price: &str,
        now: DateTime<Utc>,
    ) -> Result<OrderAck, BrokerError> {
        let approved = ApprovedOrder {
            market: Market::Kr,
            symbol: Symbol::new(code),
            side,
            order_type: OrderType::Limit,
            quantity: d(quantity),
            price: Some(d(price)),
            account_kind: AccountKind::Paper,
        };
        let request = OrderRequest {
            client_order_id: None,
            market: Market::Kr,
            symbol: Symbol::new(code),
            side,
            order_type: OrderType::Limit,
            quantity: d(quantity),
            price: Some(d(price)),
            account_kind: AccountKind::Paper,
            timeout: UnfilledTimeout::LONG_TERM,
            confirm_high_value: false,
            permit: Some(self.authority.approve(approved)),
        };
        let broker: &dyn Broker = &self.broker;
        self.runtime.block_on(broker.place_order(request, now))
    }

    fn orders(&self, group: OrderGroup, now: DateTime<Utc>) -> Vec<Order> {
        let broker: &dyn Broker = &self.broker;
        self.runtime
            .block_on(broker.orders(group, None, now))
            .unwrap()
    }

    /// 주문 하나의 지금 모습(열린 것 · 끝난 것에서 찾는다).
    fn order(&self, order_id: &OrderId, now: DateTime<Utc>) -> Order {
        let mut all = self.orders(OrderGroup::Open, now);
        all.extend(self.orders(OrderGroup::Closed, now));
        all.into_iter()
            .find(|order| &order.order_id == order_id)
            .expect("낸 주문은 목록에 있다")
    }
}

/// 상태 값을 글자(JSON) 값으로.
fn json(snapshot: &Snapshot) -> Value {
    serde_json::to_value(snapshot).unwrap()
}

fn restored(value: Value) -> Result<Desk, BrokerError> {
    Desk::restore(serde_json::from_value(value).unwrap())
}

// ───────────── 1. 마지막 시각 없이 거래 기억만 있는 상태 값은 거부한다 ─────────────

// 검증: V6, N6
#[test]
fn 거래_기억이_있는데_마지막_시각이_없는_상태_값은_되살리지_않는다() {
    let desk = Desk::open();
    desk.broker
        .on_quote(&quote(SAM, "69900", Some(at(5)), at(5)), at(5));
    let mut value = json(&desk.broker.snapshot());
    assert!(
        !value["ledger"]["trade_marks"]
            .as_array()
            .unwrap()
            .is_empty(),
        "거래소 시각이 있는 시세를 받았으니 거래 기억이 있다"
    );
    // 마지막 시세는 마지막 시각 없이 있으면 이미 거부된다. 거래 기억만 남겨 그 검사만 보게 한다.
    value["ledger"]["last_quotes"]
        .as_array_mut()
        .unwrap()
        .clear();
    value["ledger"]["last_now"] = Value::Null;
    match restored(value) {
        Err(BrokerError::InvalidSnapshot(_)) => {}
        Err(other) => panic!("다른 오류다: {other:?}"),
        Ok(_) => panic!("마지막 시각 없이 거래 기억만 있는 상태 값을 되살렸다"),
    }
}

// 검증: V6
#[test]
fn 거래_기억도_마지막_시각도_없는_새_계좌의_상태_값은_되살린다() {
    let value = json(&Desk::open().broker.snapshot());
    assert!(value["ledger"]["last_now"].is_null());
    assert!(
        value["ledger"]["trade_marks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(restored(value).is_ok());
}

// ───────────── 2. 먼 앞날 시세는 시각 없는 시세로 체결되고, 뒤의 정상 시세를 막지 않는다 ─────────────

fn code(hynix: bool) -> &'static str {
    if hynix { HYN } else { SAM }
}

/// 기준가에서 호가 단위로 `step`칸 떨어진 값. 삼성 68,000~72,000, 하이닉스 29,000~31,000.
fn price(hynix: bool, step: i8) -> String {
    let (base, tick) = if hynix { (30_000, 50) } else { (70_000, 100) };
    (base + i64::from(step) * tick).to_string()
}

/// 체결 기록에서 주문 식별자를 뺀 것(두 계좌의 식별자 모양에 기대지 않는다).
fn fill_view(
    fill: &Fill,
) -> (
    String,
    Side,
    Decimal,
    Decimal,
    Decimal,
    Decimal,
    DateTime<Utc>,
) {
    (
        fill.symbol.as_str().to_owned(),
        fill.side,
        fill.quantity,
        fill.price,
        fill.commission,
        fill.tax,
        fill.filled_at,
    )
}

/// 섞어 볼 동작 하나.
#[derive(Debug, Clone)]
enum Op {
    /// 지정가 주문. 값 = 기준가 + `step` × 호가 단위.
    Limit {
        hynix: bool,
        selling: bool,
        quantity: u8,
        step: i8,
    },
    /// 방금 받은 시세(거래소 시각 = 받은 시각).
    Quote { hynix: bool, step: i8 },
    /// 받은 시각보다 5분 + `beyond`초 앞선 거래소 시각의 시세(먼 앞날).
    FarQuote { hynix: bool, step: i8, beyond: u16 },
    /// 시간이 흐른다(계좌에 알린다).
    Wait { seconds: u8 },
    /// 상태를 내보냈다가 되살린 계좌로 갈아탄다.
    Reopen,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (any::<bool>(), any::<bool>(), 1u8..=12, -20i8..=20).prop_map(
            |(hynix, selling, quantity, step)| Op::Limit {
                hynix,
                selling,
                quantity,
                step,
            }
        ),
        4 => (any::<bool>(), -20i8..=20).prop_map(|(hynix, step)| Op::Quote { hynix, step }),
        4 => (any::<bool>(), -20i8..=20, 1u16..=3600)
            .prop_map(|(hynix, step, beyond)| Op::FarQuote { hynix, step, beyond }),
        2 => (1u8..=45).prop_map(|seconds| Op::Wait { seconds }),
        1 => Just(Op::Reopen),
    ]
}

/// 같은 동작을 두 계좌에 준다. `far`는 먼 앞날 시세를 그대로, `bare`는 같은 시세를 거래소 시각
/// 없이 받는다. 시각은 뒤로 가지 않는다.
struct Twins {
    far: Desk,
    bare: Desk,
    seconds: i64,
}

impl Twins {
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
            } => {
                let side = if selling { Side::Sell } else { Side::Buy };
                let (quantity, price) = (quantity.to_string(), price(hynix, step));
                // 거부돼도 된다(돈 · 수량이 모자람 등). 두 계좌의 결과가 같은지는 체결 기록으로 본다.
                for desk in [&self.far, &self.bare] {
                    let _ = desk.limit(code(hynix), side, &quantity, &price, now);
                }
            }
            Op::Quote { hynix, step } => {
                let quote = quote(code(hynix), &price(hynix, step), Some(now), now);
                for desk in [&self.far, &self.bare] {
                    desk.broker.on_quote(&quote, now);
                }
            }
            Op::FarQuote {
                hynix,
                step,
                beyond,
            } => {
                let traded_at = now + lead() + chrono::Duration::seconds(i64::from(beyond));
                let (code, price) = (code(hynix), price(hynix, step));
                self.far
                    .broker
                    .on_quote(&quote(code, &price, Some(traded_at), now), now);
                self.bare
                    .broker
                    .on_quote(&quote(code, &price, None, now), now);
            }
            Op::Wait { seconds } => {
                self.seconds += i64::from(seconds);
                for desk in [&self.far, &self.bare] {
                    desk.broker.on_time(self.now());
                }
            }
            Op::Reopen => {
                self.far = self.far.reopen();
                self.bare = self.bare.reopen();
            }
        }
    }
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
    fn 먼_앞날_시세는_거래소_시각이_없는_시세와_똑같이_체결된다(
        ops in proptest::collection::vec(op(), 1..60),
    ) {
        let mut twins = Twins {
            far: Desk::open(),
            bare: Desk::open(),
            seconds: 0,
        };
        for op in &ops {
            twins.apply(op);
            let far: Vec<_> = twins.far.broker.fills().iter().map(fill_view).collect();
            let bare: Vec<_> = twins.bare.broker.fills().iter().map(fill_view).collect();
            prop_assert_eq!(far, bare, "먼 앞날 시세가 시각 없는 시세와 다르게 체결됐다: {:?}", op);
        }
    }

    // 검증: C10, N6, V6
    #[test]
    fn 먼_앞날_시세로_체결된_뒤에도_정상_시세가_막히지_않는다(
        hynix in any::<bool>(),
        earlier in proptest::option::of(-20i8..=20),
        far_step in -20i8..=20,
        next_step in -20i8..=20,
        beyond in 1u16..=3600,
        quantity in 1u8..=12,
        reopen in any::<bool>(),
    ) {
        let (code, limit) = (code(hynix), price(hynix, 20));
        let quantity = Decimal::from(quantity);
        let desk = Desk::open();
        // 먼저 정상 시세를 받아 그 종목의 거래 기억이 있는 경우도 본다.
        if let Some(step) = earlier {
            desk.broker
                .on_quote(&quote(code, &price(hynix, step), Some(at(1)), at(1)), at(1));
        }

        // 가장 높은 값의 사기 주문은 어느 시세로든 체결된다.
        let first = desk
            .limit(code, Side::Buy, &quantity.to_string(), &limit, at(2))
            .unwrap()
            .order_id;
        let far_at = at(3) + lead() + chrono::Duration::seconds(i64::from(beyond));
        desk.broker
            .on_quote(&quote(code, &price(hynix, far_step), Some(far_at), at(3)), at(3));
        let filled = desk.order(&first, at(3)).execution.filled_quantity;
        prop_assert_eq!(filled, quantity, "먼 앞날 시세로 체결되지 않았다");

        let desk = if reopen { desk.reopen() } else { desk };

        // 거래소 시각이 먼 앞날 시각보다 이른 정상 시세도 체결한다(먼 앞날 시각을 기억하지 않았다).
        let second = desk
            .limit(code, Side::Buy, &quantity.to_string(), &limit, at(4))
            .unwrap()
            .order_id;
        prop_assert!(at(5) < far_at);
        desk.broker
            .on_quote(&quote(code, &price(hynix, next_step), Some(at(5)), at(5)), at(5));
        let filled = desk.order(&second, at(5)).execution.filled_quantity;
        prop_assert_eq!(filled, quantity, "먼 앞날 시세 뒤의 정상 시세가 막혔다");
    }
}
