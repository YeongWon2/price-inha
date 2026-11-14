//! 정확함: 어떤 순서로 주문 · 체결 · 취소가 일어나도 돈과 수량이 어긋나지 않는다.
//! 주문 · 시세 · 취소 · 시간 · 결제 · 되살리기를 무작위 순서로 섞어 매 단계마다 확인한다.
//!
//! 2차: 반올림 세 방향과 "한 시세 최대 수량"을 섞어 돌고, 같은 시세를 되풀이해 주는 동작과
//! 시간을 알려 주지 않고 흘려보내는 동작을 더했다. 더한 확인: 받은 지정가 주문은 돈이 모자라다는
//! 이유로 거부로 끝나지 않는다 · 주문마다 뗀 수수료와 세금은 한 번에 체결됐을 때와 같다 ·
//! 주문 뒤에 온 시세로만 체결된다 · 마감이 지난 주문은 열린 채로 보이지 않는다.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use pi_market::{Currency, Quote};
use pi_paper::{
    CloseReason, MaxFill, Order, OrderGroup, OrderStatus, OrderType, PaperConfig, ProceedsUsable,
    RoundingMode, Side, UnfilledTimeout,
};
use proptest::prelude::{
    Just, ProptestConfig, Strategy, any, prop_assert, prop_assert_eq, prop_oneof, proptest,
};
use proptest::test_runner::TestCaseError;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::support::{
    Desk, HYN, SAM, at, buy, config, d, day, kr_calendar, market_buy, market_sell, monday,
    quote_received, sell,
};

/// 섞어 볼 동작 하나.
#[derive(Debug, Clone)]
enum Op {
    /// 지정가 주문. 값 = 기준가 + `step` × 호가 단위.
    Limit {
        hynix: bool,
        selling: bool,
        quantity: u8,
        step: i8,
        timeout: u8,
    },
    /// 시장가 주문.
    Market {
        hynix: bool,
        selling: bool,
        quantity: u8,
    },
    /// 시세. 값 = 기준가 + `step` × 호가 단위.
    Quote { hynix: bool, step: i8 },
    /// 지금까지 낸 주문 가운데 하나를 취소해 본다.
    Cancel { pick: u8 },
    /// 바로 앞에 준 시세를 그대로 다시 준다(주기적으로 조회하면 같은 체결가가 되풀이 들어온다).
    Requote,
    /// 시간이 흐른다. `notify`가 거짓이면 시간이 흘렀다고 계좌에 알려 주지 않는다.
    Wait { seconds: u8, notify: bool },
    /// 결제일이 지난 달력을 넣었다가 월요일 달력으로 되돌린다(결제 전인 돈이 풀린다).
    Settle,
    /// 상태를 내보냈다가 되살린 계좌로 갈아탄다.
    Reopen,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (any::<bool>(), any::<bool>(), 1u8..=12, -20i8..=20, 0u8..3).prop_map(
            |(hynix, selling, quantity, step, timeout)| Op::Limit {
                hynix,
                selling,
                quantity,
                step,
                timeout,
            }
        ),
        2 => (any::<bool>(), any::<bool>(), 1u8..=12).prop_map(|(hynix, selling, quantity)| {
            Op::Market {
                hynix,
                selling,
                quantity,
            }
        }),
        5 => (any::<bool>(), -20i8..=20).prop_map(|(hynix, step)| Op::Quote { hynix, step }),
        2 => any::<u8>().prop_map(|pick| Op::Cancel { pick }),
        2 => Just(Op::Requote),
        3 => (1u8..=45, any::<bool>()).prop_map(|(seconds, notify)| Op::Wait { seconds, notify }),
        1 => Just(Op::Settle),
        1 => Just(Op::Reopen),
    ]
}

fn code(hynix: bool) -> &'static str {
    if hynix { HYN } else { SAM }
}

/// 기준가에서 호가 단위로 `step`칸 떨어진 값. 삼성 68,000~72,000(100원 단위), 하이닉스 29,000~31,000(50원 단위).
fn price(hynix: bool, step: i8) -> String {
    let (base, tick) = if hynix { (30_000, 50) } else { (70_000, 100) };
    (base + i64::from(step) * tick).to_string()
}

fn timeout(index: u8) -> UnfilledTimeout {
    match index {
        0 => UnfilledTimeout::SCALP,
        1 => UnfilledTimeout::SWING,
        _ => UnfilledTimeout::LONG_TERM,
    }
}

const MODES: [RoundingMode; 3] = [RoundingMode::Down, RoundingMode::HalfUp, RoundingMode::Up];

/// 무작위 설정: 판 돈을 바로 쓰는가, 한 번에 몇 주까지 체결되는가, 수수료 · 세금을 어느 쪽으로 맞추는가.
fn scenario_config(immediate: bool, max_fill: u8, mode: usize) -> PaperConfig {
    let mut config = config();
    config.initial_cash.krw = d("3000000");
    config.kr.rounding.mode = MODES[mode % MODES.len()];
    if immediate {
        config.kr.settlement.proceeds_usable = ProceedsUsable::Immediately;
    }
    config.kr.max_fill = if max_fill == 0 {
        MaxFill::Unlimited
    } else {
        MaxFill::PerQuote(Decimal::from(max_fill))
    };
    config
}

/// 동작을 차례로 돌리는 쪽. 시각은 뒤로 가지 않는다.
struct Runner {
    config: PaperConfig,
    desk: Desk,
    seconds: i64,
    placed: Vec<pi_paper::OrderId>,
    /// 바로 앞에 준 시세.
    last_quote: Option<Quote>,
    /// 시간으로 마감이 정해진 주문(단타 60초 · 스윙 600초)과 그 마감(초).
    deadlines: BTreeMap<String, i64>,
}

impl Runner {
    fn new(config: PaperConfig) -> Self {
        Self {
            desk: Desk::open(config.clone()),
            config,
            seconds: 0,
            placed: Vec::new(),
            last_quote: None,
            deadlines: BTreeMap::new(),
        }
    }

    fn now(&self) -> DateTime<Utc> {
        at(self.seconds)
    }

    fn apply(&mut self, op: &Op) -> Result<(), TestCaseError> {
        // 동작마다 1초씩 흐른다.
        self.seconds += 1;
        let now = self.now();
        // 책상의 조회는 언제나 지금 시각으로 한다(시간을 알려 주지 않았어도).
        self.desk.set_clock(now);
        match *op {
            Op::Limit {
                hynix,
                selling,
                quantity,
                step,
                timeout: index,
            } => {
                let quantity = quantity.to_string();
                let price = price(hynix, step);
                let req = if selling {
                    sell(code(hynix), &quantity, &price)
                } else {
                    buy(code(hynix), &quantity, &price)
                };
                // 거부돼도 된다(돈 · 수량이 모자람, 반대 방향 주문 등). 거부는 아무것도 바꾸지 않아야 한다.
                if let Ok(ack) = self.desk.place(&req.timeout(timeout(index)), now) {
                    let wait = match index {
                        0 => Some(60),
                        1 => Some(600),
                        _ => None,
                    };
                    if let Some(wait) = wait {
                        self.deadlines
                            .insert(ack.order_id.as_str().to_owned(), self.seconds + wait);
                    }
                    self.placed.push(ack.order_id);
                }
            }
            Op::Market {
                hynix,
                selling,
                quantity,
            } => {
                let quantity = quantity.to_string();
                let req = if selling {
                    market_sell(code(hynix), &quantity)
                } else {
                    market_buy(code(hynix), &quantity)
                };
                if let Ok(ack) = self.desk.place(&req, now) {
                    self.placed.push(ack.order_id);
                }
            }
            Op::Quote { hynix, step } => {
                let quote = quote_received(code(hynix), &price(hynix, step), Currency::Krw, now);
                self.desk.on_quote(&quote, now);
                self.last_quote = Some(quote);
            }
            Op::Requote => {
                if let Some(quote) = self.last_quote.clone() {
                    let before = self.desk.broker.fills().len();
                    let events = self.desk.on_quote(&quote, now);
                    // 이미 처리한 시세로는 체결하지 않는다(그사이 마감이 된 주문의 취소만 나올 수 있다).
                    prop_assert_eq!(
                        self.desk.broker.fills().len(),
                        before,
                        "같은 시세로 다시 체결됐다: {:?}",
                        events
                    );
                }
            }
            Op::Cancel { pick } => {
                if !self.placed.is_empty() {
                    let index = usize::from(pick) % self.placed.len();
                    // 이미 끝난 주문이면 거부된다. 그래도 된다.
                    let _ = self.desk.cancel(&self.placed[index], now);
                }
            }
            Op::Wait { seconds, notify } => {
                self.seconds += i64::from(seconds);
                if notify {
                    self.desk.on_time(self.now());
                } else {
                    self.desk.set_clock(self.now());
                }
            }
            Op::Settle => {
                self.desk
                    .broker
                    .set_calendar(kr_calendar(day(2026, 11, 20)));
                self.desk.broker.set_calendar(kr_calendar(monday()));
            }
            Op::Reopen => {
                self.desk = self.desk.reopen(self.config.clone());
            }
        }
        Ok(())
    }
}

impl Runner {
    /// 마감이 된 주문은 시간을 알려 주지 않았어도 열린 채로 보이지 않는다.
    fn check_deadlines(&self) -> Result<(), TestCaseError> {
        self.desk.set_clock(self.now());
        for order in self.desk.orders(OrderGroup::Open) {
            if let Some(deadline) = self.deadlines.get(order.order_id.as_str()) {
                prop_assert!(
                    self.seconds < *deadline,
                    "마감({}초)이 지났는데({}초) 열려 있다: {:?}",
                    deadline,
                    self.seconds,
                    order
                );
            }
        }
        Ok(())
    }
}

/// `금액 × 비율`을 그 방향으로 원 단위에 맞춘 값.
fn charge(amount: Decimal, rate: &str, mode: RoundingMode) -> Decimal {
    let strategy = match mode {
        RoundingMode::Down => RoundingStrategy::ToZero,
        RoundingMode::HalfUp => RoundingStrategy::MidpointAwayFromZero,
        RoundingMode::Up => RoundingStrategy::AwayFromZero,
    };
    (amount * d(rate)).round_dp_with_strategy(0, strategy)
}

fn remaining(order: &Order) -> Decimal {
    order.quantity - order.execution.filled_quantity
}

/// 지정가로 사는 열린 주문이 묶는 돈: 남은 수량 × 지정가 + 올림(× 0.015%).
fn limit_hold(order: &Order) -> Decimal {
    let notional = remaining(order) * order.price.unwrap();
    notional + (notional * d("0.00015")).ceil()
}

/// 돈과 수량이 어긋나지 않았는지 공개 API만으로 확인한다. 조회는 `now` 시각으로 한다.
fn check(
    desk: &Desk,
    initial: Decimal,
    mode: RoundingMode,
    now: DateTime<Utc>,
) -> Result<(), TestCaseError> {
    desk.set_clock(now);
    let cash = desk.krw();
    let fills = desk.broker.fills();
    let open = desk.orders(OrderGroup::Open);
    let closed = desk.orders(OrderGroup::Closed);
    let zero = d("0");

    // 1. 어느 칸도 음수가 아니다.
    prop_assert!(cash.available >= zero, "쓸 수 있는 돈이 음수: {cash:?}");
    prop_assert!(cash.held >= zero, "묶인 돈이 음수: {cash:?}");
    prop_assert!(cash.unsettled >= zero, "결제 전인 돈이 음수: {cash:?}");
    prop_assert_eq!(desk.buying_power(Currency::Krw), cash.available);

    // 2. 돈은 체결로만 움직인다: 처음 돈 − 산 것(금액 + 수수료) + 판 것(금액 − 수수료 − 세금).
    let mut expected_total = initial;
    let mut net: BTreeMap<String, Decimal> = BTreeMap::new();
    let mut filled_by_order: BTreeMap<String, Decimal> = BTreeMap::new();
    for fill in &fills {
        prop_assert!(fill.quantity > zero);
        prop_assert_eq!(fill.amount, fill.quantity * fill.price);
        prop_assert!(fill.commission >= zero && fill.tax >= zero);
        let position = net.entry(fill.symbol.as_str().to_owned()).or_default();
        match fill.side {
            Side::Buy => {
                expected_total -= fill.amount + fill.commission;
                prop_assert_eq!(fill.tax, zero, "살 때는 세금이 없다");
                *position += fill.quantity;
            }
            Side::Sell => {
                expected_total += fill.amount - fill.commission - fill.tax;
                *position -= fill.quantity;
            }
        }
        *filled_by_order
            .entry(fill.order_id.as_str().to_owned())
            .or_default() += fill.quantity;
    }
    prop_assert_eq!(cash.total(), expected_total, "돈이 생기거나 사라졌다");
    prop_assert_eq!(desk.usd().total(), d("10000"), "달러는 건드리지 않았다");

    // 3. 주문마다: 체결 수량은 주문 수량을 넘지 않고, 체결 기록의 합과 같다.
    for order in open.iter().chain(closed.iter()) {
        let filled = order.execution.filled_quantity;
        prop_assert!(filled >= zero && filled <= order.quantity, "{order:?}");
        let recorded = filled_by_order
            .get(order.order_id.as_str())
            .copied()
            .unwrap_or(zero);
        prop_assert_eq!(filled, recorded, "체결 기록과 주문의 체결 수량이 다르다");
    }
    for order in &open {
        prop_assert!(order.status.is_open());
        prop_assert!(remaining(order) > zero, "다 체결된 주문이 열려 있다");
    }
    for order in &closed {
        prop_assert!(!order.status.is_open());
        // 받은 지정가 주문은 돈이 모자라다는 이유로 거부로 끝나지 않는다.
        if order.order_type == OrderType::Limit {
            prop_assert!(
                order.status != OrderStatus::Rejected
                    && order.close_reason != Some(CloseReason::InsufficientBuyingPower),
                "지정가 주문이 거부로 끝났다: {:?}",
                order
            );
        }
    }
    for order in open.iter().chain(closed.iter()) {
        // 주문마다 뗀 수수료 · 세금은 체결 금액 전체가 한 번에 체결됐을 때와 같다(누적해 뗀다).
        let amount = order.execution.filled_amount;
        prop_assert_eq!(
            order.execution.commission,
            charge(amount, "0.00015", mode),
            "수수료가 한 번에 체결됐을 때와 다르다: {:?}",
            order
        );
        let tax = match order.side {
            Side::Buy => zero,
            Side::Sell => charge(amount, "0.0020", mode),
        };
        prop_assert_eq!(order.execution.tax, tax, "세금이 다르다: {:?}", order);
        let taken: Decimal = fills
            .iter()
            .filter(|fill| fill.order_id == order.order_id)
            .map(|fill| fill.commission)
            .sum();
        prop_assert_eq!(
            taken,
            order.execution.commission,
            "체결 기록의 수수료 합이 다르다"
        );
        // 주문 뒤에 온 시세로만 체결된다.
        for fill in fills.iter().filter(|fill| fill.order_id == order.order_id) {
            prop_assert!(fill.filled_at > order.ordered_at, "{:?} {:?}", fill, order);
        }
    }

    // 4. 종목마다: 보유 = 산 수량 − 판 수량, 묶인 수량 = 열린 파는 주문의 남은 수량.
    for symbol in [SAM, HYN] {
        let owned = net.get(symbol).copied().unwrap_or(zero);
        prop_assert!(owned >= zero, "없는 주식을 팔았다: {symbol} {owned}");
        let holding = desk
            .holding(symbol)
            .map_or(zero, |holding| holding.quantity);
        prop_assert_eq!(holding, owned, "보유가 체결 기록과 다르다: {}", symbol);
        let locked: Decimal = open
            .iter()
            .filter(|order| order.symbol.as_str() == symbol && order.side == Side::Sell)
            .map(remaining)
            .sum();
        let sellable = desk.sellable(symbol);
        prop_assert!(sellable >= zero);
        prop_assert_eq!(holding - sellable, locked, "묶인 수량이 다르다: {}", symbol);
    }

    // 5. 묶인 돈 = 열린 사는 주문들이 묶은 돈. 시장가가 묶은 돈은 밖에서 셈할 수 없어 따로 본다.
    let open_buys: Vec<&Order> = open
        .iter()
        .filter(|order| order.side == Side::Buy)
        .collect();
    let limit_holds: Decimal = open_buys
        .iter()
        .filter(|order| order.order_type == OrderType::Limit)
        .map(|order| limit_hold(order))
        .sum();
    let all_limit = open_buys
        .iter()
        .all(|order| order.order_type == OrderType::Limit);
    if all_limit && mode == RoundingMode::Down {
        prop_assert_eq!(cash.held, limit_holds, "묶인 돈이 열린 주문과 다르다");
    } else if all_limit {
        // 올림 · 반올림에서는 누적 수수료가 남은 수량으로 다시 셈한 몫보다 1원 앞설 수 있어,
        // 남은 수량에 필요한 돈보다 적게 묶여 있을 수 있다. 더 많이 묶지는 않는다.
        prop_assert!(
            cash.held <= limit_holds,
            "남은 수량에 필요한 돈보다 많이 묶였다: {:?} > {}",
            cash,
            limit_holds
        );
    } else {
        prop_assert!(
            cash.held >= limit_holds,
            "지정가 주문이 묶은 돈보다 적게 묶였다"
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

    // 검증: C10, E12, E29, N4, V6
    #[test]
    fn 어떤_순서로_섞어도_돈과_수량이_어긋나지_않는다(
        immediate in any::<bool>(),
        max_fill in 0u8..=4,
        mode in 0usize..3,
        ops in proptest::collection::vec(op(), 1..60),
    ) {
        let config = scenario_config(immediate, max_fill, mode);
        let initial = config.initial_cash.krw;
        let mode = config.kr.rounding.mode;
        let mut runner = Runner::new(config);
        check(&runner.desk, initial, mode, runner.now())?;
        for op in &ops {
            runner.apply(op)?;
            check(&runner.desk, initial, mode, runner.now())?;
            runner.check_deadlines()?;
        }

        // 끝: 남은 주문을 모두 취소하면 묶인 것이 하나도 없다.
        let now = runner.now();
        for order in runner.desk.orders(OrderGroup::Open) {
            runner.desk.cancel(&order.order_id, now).unwrap();
        }
        check(&runner.desk, initial, mode, now)?;
        prop_assert_eq!(runner.desk.krw().held, d("0"));
        prop_assert!(runner.desk.orders(OrderGroup::Open).is_empty());
        for symbol in [SAM, HYN] {
            let holding = runner.desk.holding(symbol).map_or(d("0"), |holding| holding.quantity);
            prop_assert_eq!(runner.desk.sellable(symbol), holding);
        }
    }

    // 검증: V6
    #[test]
    fn 같은_입력은_같은_결과를_낸다(
        immediate in any::<bool>(),
        max_fill in 0u8..=4,
        mode in 0usize..3,
        ops in proptest::collection::vec(op(), 1..40),
    ) {
        let mut first = Runner::new(scenario_config(immediate, max_fill, mode));
        let mut second = Runner::new(scenario_config(immediate, max_fill, mode));
        for op in &ops {
            first.apply(op)?;
            second.apply(op)?;
        }
        prop_assert_eq!(first.desk.broker.snapshot(), second.desk.broker.snapshot());
        prop_assert_eq!(first.desk.broker.fills(), second.desk.broker.fills());
        prop_assert_eq!(first.desk.krw(), second.desk.krw());
        prop_assert_eq!(first.placed, second.placed);
    }

    // 검증: V6
    #[test]
    fn 중간에_껐다_켜도_끊기지_않은_계좌와_같은_결과가_나온다(
        immediate in any::<bool>(),
        max_fill in 0u8..=4,
        mode in 0usize..3,
        before in proptest::collection::vec(op(), 0..30),
        after in proptest::collection::vec(op(), 1..30),
    ) {
        let mut straight = Runner::new(scenario_config(immediate, max_fill, mode));
        let mut restarted = Runner::new(scenario_config(immediate, max_fill, mode));
        for op in &before {
            straight.apply(op)?;
            restarted.apply(op)?;
        }
        restarted.desk = restarted.desk.reopen(restarted.config.clone());
        for op in &after {
            straight.apply(op)?;
            restarted.apply(op)?;
        }
        prop_assert_eq!(restarted.desk.broker.snapshot(), straight.desk.broker.snapshot());
        prop_assert_eq!(restarted.desk.broker.fills(), straight.desk.broker.fills());
        prop_assert_eq!(restarted.desk.krw(), straight.desk.krw());
        prop_assert_eq!(restarted.desk.holdings(), straight.desk.holdings());
        prop_assert_eq!(restarted.placed, straight.placed);
    }
}
