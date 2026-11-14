//! 정확함: 어떤 순서로 주문 · 체결 · 취소가 일어나도 돈과 수량이 어긋나지 않는다.
//! 주문 · 시세 · 취소 · 시간 · 결제 · 되살리기를 무작위 순서로 섞어 매 단계마다 확인한다.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use pi_market::Currency;
use pi_paper::{
    MaxFill, Order, OrderGroup, OrderType, PaperConfig, ProceedsUsable, Side, UnfilledTimeout,
};
use proptest::prelude::{
    Just, ProptestConfig, Strategy, any, prop_assert, prop_assert_eq, prop_oneof, proptest,
};
use proptest::test_runner::TestCaseError;
use rust_decimal::Decimal;

use crate::support::{
    Desk, HYN, SAM, at, buy, config, d, day, kr_calendar, market_buy, market_sell, monday, sell,
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
    /// 시간이 흐른다.
    Wait { seconds: u8 },
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
        2 => (1u8..=45).prop_map(|seconds| Op::Wait { seconds }),
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

/// 무작위 설정: 판 돈을 바로 쓰는가, 한 번에 몇 주까지 체결되는가.
fn scenario_config(immediate: bool, max_fill: u8) -> PaperConfig {
    let mut config = config();
    config.initial_cash.krw = d("3000000");
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
}

impl Runner {
    fn new(config: PaperConfig) -> Self {
        Self {
            desk: Desk::open(config.clone()),
            config,
            seconds: 0,
            placed: Vec::new(),
        }
    }

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
                self.desk.quote(code(hynix), &price(hynix, step), now);
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
    }
}

fn remaining(order: &Order) -> Decimal {
    order.quantity - order.execution.filled_quantity
}

/// 지정가로 사는 열린 주문이 묶는 돈: 남은 수량 × 지정가 + 올림(× 0.015%).
fn limit_hold(order: &Order) -> Decimal {
    let notional = remaining(order) * order.price.unwrap();
    notional + (notional * d("0.00015")).ceil()
}

/// 돈과 수량이 어긋나지 않았는지 공개 API만으로 확인한다.
fn check(desk: &Desk, initial: Decimal) -> Result<(), TestCaseError> {
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
    if open_buys
        .iter()
        .all(|order| order.order_type == OrderType::Limit)
    {
        prop_assert_eq!(cash.held, limit_holds, "묶인 돈이 열린 주문과 다르다");
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

    // 검증: C10, E12, E29, V6
    #[test]
    fn 어떤_순서로_섞어도_돈과_수량이_어긋나지_않는다(
        immediate in any::<bool>(),
        max_fill in 0u8..=4,
        ops in proptest::collection::vec(op(), 1..60),
    ) {
        let config = scenario_config(immediate, max_fill);
        let initial = config.initial_cash.krw;
        let mut runner = Runner::new(config);
        check(&runner.desk, initial)?;
        for op in &ops {
            runner.apply(op);
            check(&runner.desk, initial)?;
        }

        // 끝: 남은 주문을 모두 취소하면 묶인 것이 하나도 없다.
        let now = runner.now();
        for order in runner.desk.orders(OrderGroup::Open) {
            runner.desk.cancel(&order.order_id, now).unwrap();
        }
        check(&runner.desk, initial)?;
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
        ops in proptest::collection::vec(op(), 1..40),
    ) {
        let mut first = Runner::new(scenario_config(immediate, max_fill));
        let mut second = Runner::new(scenario_config(immediate, max_fill));
        for op in &ops {
            first.apply(op);
            second.apply(op);
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
        before in proptest::collection::vec(op(), 0..30),
        after in proptest::collection::vec(op(), 1..30),
    ) {
        let mut straight = Runner::new(scenario_config(immediate, max_fill));
        let mut restarted = Runner::new(scenario_config(immediate, max_fill));
        for op in &before {
            straight.apply(op);
            restarted.apply(op);
        }
        restarted.desk = restarted.desk.reopen(restarted.config.clone());
        for op in &after {
            straight.apply(op);
            restarted.apply(op);
        }
        prop_assert_eq!(restarted.desk.broker.snapshot(), straight.desk.broker.snapshot());
        prop_assert_eq!(restarted.desk.broker.fills(), straight.desk.broker.fills());
        prop_assert_eq!(restarted.desk.krw(), straight.desk.krw());
        prop_assert_eq!(restarted.desk.holdings(), straight.desk.holdings());
        prop_assert_eq!(restarted.placed, straight.placed);
    }
}
