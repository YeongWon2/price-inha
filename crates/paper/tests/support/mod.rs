//! 테스트가 함께 쓰는 도우미: 설정 · 달력 · 시세 · 주문 요청 · 계좌를 다루는 책상(`Desk`).

use std::cell::Cell;
use std::future::Future;
use std::time::Duration;

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_market::{
    Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol, TradingDay,
};
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, BrokerError, CashBalance, CheckAuthority, Holding,
    InitialCash, LimitFillPrice, MarketRules, MaxFill, ModifyRequest, MoneyRounding, Order,
    OrderAck, OrderEvent, OrderEventKind, OrderGroup, OrderId, OrderRequest, OrderType,
    PaperBroker, PaperConfig, ProceedsUsable, RoundingMode, SessionRules, Settlement, Side,
    Snapshot, TickBand, UnfilledTimeout,
};
use rust_decimal::Decimal;
use tokio::runtime::{Builder, Runtime};

/// 삼성전자. 기준가 70,000원(호가 단위 100원, 상한가 91,000 · 하한가 49,000).
pub const SAM: &str = "005930";
/// SK하이닉스 자리. 기준가 30,000원(호가 단위 50원, 상한가 39,000 · 하한가 21,000).
pub const HYN: &str = "000660";
/// 미국 종목.
pub const AAPL: &str = "AAPL";

/// 글자를 십진수로.
pub fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// 종목.
pub fn sym(code: &str) -> Symbol {
    Symbol::new(code)
}

/// 날짜.
pub fn day(year: i32, month: u32, date: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, date).unwrap()
}

/// 기준 영업일: 2026-11-16 월요일.
pub fn monday() -> NaiveDate {
    day(2026, 11, 16)
}

/// 그 날짜의 UTC 시각.
pub fn utc(date: NaiveDate, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(hour, minute, second).unwrap())
}

/// 국내 정규장 안의 기준 시각(월요일 10:00 KST = 01:00 UTC)에서 `seconds`초 뒤.
pub fn at(seconds: i64) -> DateTime<Utc> {
    utc(monday(), 1, 0, 0) + chrono::Duration::seconds(seconds)
}

/// 미국 정규장 안의 기준 시각(월요일 15:00 UTC)에서 `seconds`초 뒤.
pub fn us_at(seconds: i64) -> DateTime<Utc> {
    utc(monday(), 15, 0, 0) + chrono::Duration::seconds(seconds)
}

fn step_weekday(date: NaiveDate, forward: bool) -> NaiveDate {
    let mut next = date;
    loop {
        next = if forward {
            next.succ_opt().unwrap()
        } else {
            next.pred_opt().unwrap()
        };
        if !matches!(next.weekday(), Weekday::Sat | Weekday::Sun) {
            return next;
        }
    }
}

fn session(kind: SessionKind, start: DateTime<Utc>, end: DateTime<Utc>) -> Session {
    Session { kind, start, end }
}

fn kr_day(date: NaiveDate) -> TradingDay {
    // 프리 08:00~09:00, 정규 09:00~15:30, 애프터 15:30~20:00 (KST). UTC로는 9시간 앞이다.
    let open = utc(date, 0, 0, 0);
    TradingDay {
        date,
        sessions: vec![
            session(
                SessionKind::PreMarket,
                open - chrono::Duration::hours(1),
                open,
            ),
            session(SessionKind::Regular, open, utc(date, 6, 30, 0)),
            session(
                SessionKind::AfterMarket,
                utc(date, 6, 30, 0),
                utc(date, 11, 0, 0),
            ),
        ],
    }
}

fn us_day(date: NaiveDate) -> TradingDay {
    // 프리 09:00~14:30, 정규 14:30~21:00, 애프터 21:00~다음 날 01:00 (UTC).
    TradingDay {
        date,
        sessions: vec![
            session(
                SessionKind::PreMarket,
                utc(date, 9, 0, 0),
                utc(date, 14, 30, 0),
            ),
            session(
                SessionKind::Regular,
                utc(date, 14, 30, 0),
                utc(date, 21, 0, 0),
            ),
            session(
                SessionKind::AfterMarket,
                utc(date, 21, 0, 0),
                utc(date, 21, 0, 0) + chrono::Duration::hours(4),
            ),
        ],
    }
}

/// 그 날짜가 기준일인 국내 달력. 정규장은 그 날짜 00:00~06:30 UTC다.
pub fn kr_calendar(date: NaiveDate) -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        today: kr_day(date),
        previous_business_day: kr_day(step_weekday(date, false)),
        next_business_day: kr_day(step_weekday(date, true)),
    }
}

/// 그 날짜가 기준일인 미국 달력. 정규장은 그 날짜 14:30~21:00 UTC다.
pub fn us_calendar(date: NaiveDate) -> MarketCalendar {
    MarketCalendar {
        market: Market::Us,
        today: us_day(date),
        previous_business_day: us_day(step_weekday(date, false)),
        next_business_day: us_day(step_weekday(date, true)),
    }
}

/// KRX 호가 단위 표(테스트가 직접 적은 값).
pub fn krx_ticks() -> Vec<TickBand> {
    let band = |below: Option<&str>, tick: &str| TickBand {
        below: below.map(d),
        tick: d(tick),
    };
    vec![
        band(Some("2000"), "1"),
        band(Some("5000"), "5"),
        band(Some("20000"), "10"),
        band(Some("50000"), "50"),
        band(Some("200000"), "100"),
        band(Some("500000"), "500"),
        band(None, "1000"),
    ]
}

/// 그 날짜의 국내 정규장이 끝나는 시각(06:30 UTC = 15:30 KST).
pub fn kr_close(date: NaiveDate) -> DateTime<Utc> {
    utc(date, 6, 30, 0)
}

fn regular_only() -> SessionRules {
    SessionRules {
        limit: vec![SessionKind::Regular],
        market: vec![SessionKind::Regular],
    }
}

/// 테스트 설정. 기본값에 기대지 않게 모든 숫자를 적는다.
///
/// 원화 10,000,000 · 달러 10,000. 국내: 수수료 0.015%, 거래세 0.20%, 원 미만 버림, 미끄러짐 0.1%,
/// T+2 결제 뒤 사용, 1억 원 이상은 확인 · 30억 원 초과는 거부. 미국: 수수료 0.1%, 팔 때 0.01%,
/// 센트 미만 버림, 미끄러짐 0.1%, T+1. 구독 하나가 쥐는 이벤트는 64개까지.
pub fn config() -> PaperConfig {
    PaperConfig {
        initial_cash: InitialCash {
            krw: d("10000000"),
            usd: d("10000"),
        },
        kr: MarketRules {
            currency: Currency::Krw,
            commission_rate: d("0.00015"),
            sell_tax_rate: d("0.0020"),
            rounding: MoneyRounding {
                scale: 0,
                mode: RoundingMode::Down,
            },
            tick_table: krx_ticks(),
            price_limit_rate: Some(d("0.30")),
            settlement: Settlement {
                business_days: 2,
                proceeds_usable: ProceedsUsable::AfterSettlement,
            },
            market_slippage_rate: d("0.001"),
            limit_fill_price: LimitFillPrice::Limit,
            max_fill: MaxFill::Unlimited,
            sessions: regular_only(),
            high_value_confirm_from: Some(d("100000000")),
            max_order_amount: Some(d("3000000000")),
        },
        us: MarketRules {
            currency: Currency::Usd,
            commission_rate: d("0.001"),
            sell_tax_rate: d("0.0001"),
            rounding: MoneyRounding {
                scale: 2,
                mode: RoundingMode::Down,
            },
            tick_table: vec![
                TickBand {
                    below: Some(d("1")),
                    tick: d("0.0001"),
                },
                TickBand {
                    below: None,
                    tick: d("0.01"),
                },
            ],
            price_limit_rate: None,
            settlement: Settlement {
                business_days: 1,
                proceeds_usable: ProceedsUsable::AfterSettlement,
            },
            market_slippage_rate: d("0.001"),
            limit_fill_price: LimitFillPrice::Limit,
            max_fill: MaxFill::Unlimited,
            sessions: regular_only(),
            high_value_confirm_from: None,
            max_order_amount: None,
        },
        stale: StalePolicy {
            max_age: Duration::from_secs(30),
        },
        event_buffer: 64,
    }
}

/// 처음 원화만 바꾼 테스트 설정.
pub fn config_with_krw(krw: &str) -> PaperConfig {
    let mut config = config();
    config.initial_cash.krw = d(krw);
    config
}

/// 시세 하나. 받은 시각을 따로 정한다. 거래소 시각은 받은 시각과 같다(그때 막 체결된 값).
pub fn quote_received(
    code: &str,
    price: &str,
    currency: Currency,
    received_at: DateTime<Utc>,
) -> Quote {
    quote_full(code, price, currency, Some(received_at), received_at)
}

/// 시세 하나. 거래소 시각과 받은 시각을 저마다 정한다.
pub fn quote_full(
    code: &str,
    price: &str,
    currency: Currency,
    exchange_time: Option<DateTime<Utc>>,
    received_at: DateTime<Utc>,
) -> Quote {
    Quote {
        symbol: sym(code),
        price: d(price),
        currency,
        exchange_time,
        received_at,
    }
}

/// 표시를 뺀 주문 내용. 복제할 수 있어 같은 내용으로 여러 번 낼 수 있다.
#[derive(Debug, Clone)]
pub struct Req {
    pub client_order_id: Option<String>,
    pub market: Market,
    pub symbol: Symbol,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub account_kind: AccountKind,
    pub timeout: UnfilledTimeout,
    pub confirm_high_value: bool,
}

impl Req {
    fn new(market: Market, code: &str, side: Side, quantity: &str, price: Option<&str>) -> Self {
        Self {
            client_order_id: None,
            market,
            symbol: sym(code),
            side,
            order_type: if price.is_some() {
                OrderType::Limit
            } else {
                OrderType::Market
            },
            quantity: d(quantity),
            price: price.map(d),
            account_kind: AccountKind::Paper,
            timeout: UnfilledTimeout::LONG_TERM,
            confirm_high_value: false,
        }
    }

    /// 큰 주문 확인 표시를 붙인다.
    pub fn confirmed(mut self) -> Self {
        self.confirm_high_value = true;
        self
    }

    /// 멱등성 키를 붙인다.
    pub fn key(mut self, key: &str) -> Self {
        self.client_order_id = Some(key.to_owned());
        self
    }

    /// 미체결 취소 시간을 바꾼다.
    pub fn timeout(mut self, timeout: UnfilledTimeout) -> Self {
        self.timeout = timeout;
        self
    }

    /// 이 내용 그대로의 승인 내용.
    pub fn approved(&self) -> ApprovedOrder {
        ApprovedOrder {
            market: self.market,
            symbol: self.symbol.clone(),
            side: self.side,
            order_type: self.order_type,
            quantity: self.quantity,
            price: self.price,
            account_kind: self.account_kind,
        }
    }

    /// 표시를 붙여(또는 붙이지 않고) 주문 요청으로 만든다.
    pub fn request(&self, permit: Option<pi_paper::CheckPermit>) -> OrderRequest {
        OrderRequest {
            client_order_id: self.client_order_id.clone(),
            market: self.market,
            symbol: self.symbol.clone(),
            side: self.side,
            order_type: self.order_type,
            quantity: self.quantity,
            price: self.price,
            account_kind: self.account_kind,
            timeout: self.timeout,
            confirm_high_value: self.confirm_high_value,
            permit,
        }
    }
}

/// 국내 지정가 사기.
pub fn buy(code: &str, quantity: &str, price: &str) -> Req {
    Req::new(Market::Kr, code, Side::Buy, quantity, Some(price))
}

/// 국내 지정가 팔기.
pub fn sell(code: &str, quantity: &str, price: &str) -> Req {
    Req::new(Market::Kr, code, Side::Sell, quantity, Some(price))
}

/// 국내 시장가 사기.
pub fn market_buy(code: &str, quantity: &str) -> Req {
    Req::new(Market::Kr, code, Side::Buy, quantity, None)
}

/// 국내 시장가 팔기.
pub fn market_sell(code: &str, quantity: &str) -> Req {
    Req::new(Market::Kr, code, Side::Sell, quantity, None)
}

/// 미국 지정가 사기.
pub fn us_buy(code: &str, quantity: &str, price: &str) -> Req {
    Req::new(Market::Us, code, Side::Buy, quantity, Some(price))
}

/// 미국 지정가 팔기.
pub fn us_sell(code: &str, quantity: &str, price: &str) -> Req {
    Req::new(Market::Us, code, Side::Sell, quantity, Some(price))
}

/// 이벤트들의 종류만.
pub fn kinds(events: &[OrderEvent]) -> Vec<OrderEventKind> {
    events.iter().map(|event| event.kind).collect()
}

/// 현금 세 칸을 글자로 적어 비교하기 쉽게 만든다.
pub fn cash(available: &str, held: &str, unsettled: &str) -> CashBalance {
    CashBalance {
        available: d(available),
        held: d(held),
        unsettled: d(unsettled),
    }
}

/// 모의계좌 하나와 그 권한, 그리고 async 메서드를 돌릴 실행기.
///
/// 조회에는 시각이 필요하다(조회도 먼저 마감이 된 주문을 취소한다). 책상은 이 책상을 거쳐 간
/// 마지막 호출의 시각을 기억해 두고, 시각 없이 부르는 조회(`order` · `krw` …)에 그 시각을 쓴다.
/// 시각을 정해 조회하려면 `_at`이 붙은 것을 쓴다.
pub struct Desk {
    pub broker: PaperBroker,
    pub authority: CheckAuthority,
    runtime: Runtime,
    clock: Cell<DateTime<Utc>>,
}

impl Desk {
    fn wrap((broker, authority): (PaperBroker, CheckAuthority)) -> Self {
        Self {
            broker,
            authority,
            runtime: Builder::new_current_thread().build().unwrap(),
            clock: Cell::new(DateTime::<Utc>::MIN_UTC),
        }
    }

    /// 이 책상을 거쳐 간 마지막 호출의 시각.
    fn seen(&self) -> DateTime<Utc> {
        self.clock.get()
    }

    fn see(&self, now: DateTime<Utc>) {
        self.clock.set(now);
    }

    /// 계좌를 부르지 않고 책상이 기억하는 시각만 바꾼다. 뒤따르는 시각 없는 조회가 이 시각으로 간다.
    pub fn set_clock(&self, now: DateTime<Utc>) {
        self.see(now);
    }

    /// 달력도 기준가도 넣지 않은 새 계좌.
    pub fn bare(config: PaperConfig) -> Self {
        Self::wrap(PaperBroker::open(config).unwrap())
    }

    /// 월요일 달력(국내 · 미국)과 기준가를 넣은 새 계좌.
    pub fn open(config: PaperConfig) -> Self {
        let desk = Self::bare(config);
        desk.feed();
        desk
    }

    /// 기본 테스트 설정으로 연 계좌.
    pub fn standard() -> Self {
        Self::open(config())
    }

    /// 월요일 달력(국내 · 미국)과 월요일의 기준가(삼성 70,000 · 하이닉스 30,000)를 넣는다.
    pub fn feed(&self) {
        self.broker.set_calendar(kr_calendar(monday()));
        self.broker.set_calendar(us_calendar(monday()));
        self.reference(monday());
    }

    /// 그 날짜의 기준가(삼성 70,000 · 하이닉스 30,000)를 넣는다.
    pub fn reference(&self, date: NaiveDate) {
        self.broker.set_reference_price(sym(SAM), d("70000"), date);
        self.broker.set_reference_price(sym(HYN), d("30000"), date);
    }

    /// 상태를 글자(JSON)로 내보냈다가 그 글자에서 새 계좌로 되살린다(달력 · 기준가는 넣지 않는다).
    /// 책상이 기억한 시각은 이어진다.
    pub fn reopen_bare(&self, config: PaperConfig) -> Self {
        let text = serde_json::to_string(&self.broker.snapshot()).unwrap();
        let snapshot: Snapshot = serde_json::from_str(&text).unwrap();
        let desk = Self::wrap(PaperBroker::restore(config, snapshot).unwrap());
        desk.clock.set(self.seen());
        desk
    }

    /// 되살린 뒤 월요일 달력과 기준가를 다시 넣는다.
    pub fn reopen(&self, config: PaperConfig) -> Self {
        let desk = self.reopen_bare(config);
        desk.feed();
        desk
    }

    /// async 호출을 끝까지 돌린다.
    pub fn run<F: Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    /// 주문을 trait으로만 다루는 쪽이 보는 모습.
    pub fn as_broker(&self) -> &dyn Broker {
        &self.broker
    }

    /// 이 계좌의 권한이 그 내용 그대로 승인한 표시를 붙여 주문을 낸다.
    pub fn place(&self, req: &Req, now: DateTime<Utc>) -> Result<OrderAck, BrokerError> {
        let permit = self.authority.approve(req.approved());
        self.place_raw(req.request(Some(permit)), now)
    }

    /// 만든 요청 그대로 주문을 낸다.
    pub fn place_raw(
        &self,
        request: OrderRequest,
        now: DateTime<Utc>,
    ) -> Result<OrderAck, BrokerError> {
        self.see(now);
        self.run(self.as_broker().place_order(request, now))
    }

    /// 주문을 내고 받아들여진 식별자를 돌려준다.
    pub fn accepted(&self, req: &Req, now: DateTime<Utc>) -> OrderId {
        self.place(req, now).unwrap().order_id
    }

    /// 취소.
    pub fn cancel(&self, order_id: &OrderId, now: DateTime<Utc>) -> Result<OrderId, BrokerError> {
        self.see(now);
        self.run(self.as_broker().cancel_order(order_id, now))
    }

    /// 정정. 원주문의 시장 · 종목 · 방향에 새 유형 · 수량 · 값으로 승인한 표시를 붙인다.
    /// `approved_quantity`는 표시에 적을 수량이다(국내는 새 수량, 미국은 원주문 수량).
    pub fn modify(
        &self,
        order_id: &OrderId,
        quantity: Option<&str>,
        price: Option<&str>,
        approved_quantity: &str,
        now: DateTime<Utc>,
    ) -> Result<OrderId, BrokerError> {
        self.modify_with(order_id, quantity, price, approved_quantity, false, now)
    }

    /// 큰 주문 확인 표시를 정해서 하는 정정.
    pub fn modify_with(
        &self,
        order_id: &OrderId,
        quantity: Option<&str>,
        price: Option<&str>,
        approved_quantity: &str,
        confirm_high_value: bool,
        now: DateTime<Utc>,
    ) -> Result<OrderId, BrokerError> {
        // 원주문의 내용은 정정하는 시각보다 앞선 시각으로 본다(이 조회가 마감을 처리하지 않게).
        let original = self.order_at(order_id, self.seen().min(now));
        let order_type = if price.is_some() {
            OrderType::Limit
        } else {
            OrderType::Market
        };
        let permit = self.authority.approve(ApprovedOrder {
            market: original.market,
            symbol: original.symbol,
            side: original.side,
            order_type,
            quantity: d(approved_quantity),
            price: price.map(d),
            account_kind: AccountKind::Paper,
        });
        let request = ModifyRequest {
            order_type,
            quantity: quantity.map(d),
            price: price.map(d),
            confirm_high_value,
            permit: Some(permit),
        };
        self.modify_raw(order_id, request, now)
    }

    /// 만든 정정 요청 그대로.
    pub fn modify_raw(
        &self,
        order_id: &OrderId,
        request: ModifyRequest,
        now: DateTime<Utc>,
    ) -> Result<OrderId, BrokerError> {
        self.see(now);
        self.run(self.as_broker().modify_order(order_id, request, now))
    }

    /// 주문 상세(책상이 기억한 시각으로).
    pub fn order(&self, order_id: &OrderId) -> Order {
        self.order_at(order_id, self.seen())
    }

    /// 그 시각의 주문 상세.
    pub fn order_at(&self, order_id: &OrderId, now: DateTime<Utc>) -> Order {
        self.run(self.as_broker().order(order_id, now)).unwrap()
    }

    /// 주문 목록(모든 종목, 책상이 기억한 시각으로).
    pub fn orders(&self, group: OrderGroup) -> Vec<Order> {
        self.orders_at(group, self.seen())
    }

    /// 그 시각의 주문 목록(모든 종목).
    pub fn orders_at(&self, group: OrderGroup, now: DateTime<Utc>) -> Vec<Order> {
        self.run(self.as_broker().orders(group, None, now)).unwrap()
    }

    /// 시세 하나를 그대로 준다.
    pub fn on_quote(&self, quote: &Quote, now: DateTime<Utc>) -> Vec<OrderEvent> {
        self.see(now);
        self.broker.on_quote(quote, now)
    }

    /// 시각만 알려 준다.
    pub fn on_time(&self, now: DateTime<Utc>) -> Vec<OrderEvent> {
        self.see(now);
        self.broker.on_time(now)
    }

    /// 방금 받은 원화 시세를 준다(거래소 시각 = 받은 시각 = `now`. 그때 막 체결된 새 값이다).
    pub fn quote(&self, code: &str, price: &str, now: DateTime<Utc>) -> Vec<OrderEvent> {
        self.on_quote(&quote_received(code, price, Currency::Krw, now), now)
    }

    /// 방금 받은 달러 시세를 준다.
    pub fn us_quote(&self, code: &str, price: &str, now: DateTime<Utc>) -> Vec<OrderEvent> {
        self.on_quote(&quote_received(code, price, Currency::Usd, now), now)
    }

    /// 원화 현금.
    pub fn krw(&self) -> CashBalance {
        self.krw_at(self.seen())
    }

    /// 그 시각의 원화 현금.
    pub fn krw_at(&self, now: DateTime<Utc>) -> CashBalance {
        self.broker.cash(&Currency::Krw, now)
    }

    /// 달러 현금.
    pub fn usd(&self) -> CashBalance {
        self.broker.cash(&Currency::Usd, self.seen())
    }

    /// 매수 가능 금액.
    pub fn buying_power(&self, currency: Currency) -> Decimal {
        self.buying_power_at(currency, self.seen())
    }

    /// 그 시각의 매수 가능 금액.
    pub fn buying_power_at(&self, currency: Currency, now: DateTime<Utc>) -> Decimal {
        self.run(self.as_broker().buying_power(currency, now))
            .unwrap()
    }

    /// 판매 가능 수량.
    pub fn sellable(&self, code: &str) -> Decimal {
        self.sellable_at(code, self.seen())
    }

    /// 그 시각의 판매 가능 수량.
    pub fn sellable_at(&self, code: &str, now: DateTime<Utc>) -> Decimal {
        self.run(self.as_broker().sellable_quantity(&sym(code), now))
            .unwrap()
    }

    /// 보유 전부.
    pub fn holdings(&self) -> Vec<Holding> {
        self.holdings_at(self.seen())
    }

    /// 그 시각의 보유 전부.
    pub fn holdings_at(&self, now: DateTime<Utc>) -> Vec<Holding> {
        self.run(self.as_broker().holdings(None, now)).unwrap()
    }

    /// 그 종목의 보유. 없으면 `None`.
    pub fn holding(&self, code: &str) -> Option<Holding> {
        self.run(self.as_broker().holdings(Some(&sym(code)), self.seen()))
            .unwrap()
            .into_iter()
            .next()
    }

    /// 지정가로 사서 바로 체결시켜 보유를 만든다(국내). 주문은 `now`에 내고, 체결시키는 시세는
    /// 그 0.5초 뒤에 준다(주문 뒤에 온 시세로만 체결되므로).
    pub fn own(&self, code: &str, quantity: &str, price: &str, now: DateTime<Utc>) {
        let order_id = self.accepted(&buy(code, quantity, price), now);
        self.quote(code, price, now + chrono::Duration::milliseconds(500));
        assert_eq!(
            self.order(&order_id).execution.filled_quantity,
            d(quantity),
            "보유를 만드는 주문이 다 체결돼야 한다"
        );
    }
}
