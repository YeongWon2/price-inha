//! 돈 계산. 이 모듈의 반올림은 모두 여기 한 곳을 거친다.
//!
//! 십진수 계산이 넘치면(터무니없이 큰 수량 · 값) `None`을 돌려준다. 부르는 쪽은 그 주문을
//! 받지 않거나 그 체결을 건너뛴다.

use rust_decimal::{Decimal, RoundingStrategy};

use crate::{MarketRules, MoneyRounding, RoundingMode, Side};

/// `value`를 `scale` 자리에서 `mode`로 맞춘다.
fn round(value: Decimal, scale: u32, mode: RoundingMode) -> Decimal {
    let strategy = match mode {
        RoundingMode::Down => RoundingStrategy::ToZero,
        RoundingMode::HalfUp => RoundingStrategy::MidpointAwayFromZero,
        RoundingMode::Up => RoundingStrategy::AwayFromZero,
    };
    value.round_dp_with_strategy(scale, strategy)
}

/// 체결에서 떼는 수수료 · 세금: `금액 × 비율`을 설정한 방향으로 맞춘다.
pub(crate) fn charge(amount: Decimal, rate: Decimal, rounding: MoneyRounding) -> Option<Decimal> {
    Some(round(
        amount.checked_mul(rate)?,
        rounding.scale,
        rounding.mode,
    ))
}

/// 사는 주문에 묶는 돈: `수량 × 값 + 올림(수량 × 값 × 수수료율)`. 수수료 몫은 항상 올린다.
pub(crate) fn buy_hold(quantity: Decimal, price: Decimal, rules: &MarketRules) -> Option<Decimal> {
    let notional = quantity.checked_mul(price)?;
    let fee = round(
        notional.checked_mul(rules.commission_rate)?,
        rules.rounding.scale,
        RoundingMode::Up,
    );
    notional.checked_add(fee)
}

/// 사는 체결의 비용: `수량 × 값 + 수수료`.
pub(crate) fn buy_cost(quantity: Decimal, price: Decimal, rules: &MarketRules) -> Option<Decimal> {
    let amount = quantity.checked_mul(price)?;
    amount.checked_add(charge(amount, rules.commission_rate, rules.rounding)?)
}

/// 값이 호가 단위에 맞는가. 단위가 0이면(표가 빈 설정) 맞는 것으로 본다.
pub(crate) fn on_tick(price: Decimal, tick: Decimal) -> bool {
    tick.is_zero() || (price % tick).is_zero()
}

/// 값을 호가 단위 아래로 맞춘다.
fn floor_to_tick(value: Decimal, rules: &MarketRules) -> Option<Decimal> {
    let tick = rules.tick_for(value);
    if tick.is_zero() {
        return Some(value);
    }
    value.checked_div(tick)?.floor().checked_mul(tick)
}

/// 값을 호가 단위 위로 맞춘다.
fn ceil_to_tick(value: Decimal, rules: &MarketRules) -> Option<Decimal> {
    let tick = rules.tick_for(value);
    if tick.is_zero() {
        return Some(value);
    }
    value.checked_div(tick)?.ceil().checked_mul(tick)
}

/// 시장가의 체결 값: 시세에 미끄러짐을 불리한 쪽으로 더하고 불리한 쪽 호가 단위로 맞춘다
/// (사기는 올림, 팔기는 내림).
pub(crate) fn market_price(quote: Decimal, side: Side, rules: &MarketRules) -> Option<Decimal> {
    let slip = rules.market_slippage_rate;
    match side {
        Side::Buy => ceil_to_tick(quote.checked_mul(Decimal::ONE.checked_add(slip)?)?, rules),
        Side::Sell => floor_to_tick(quote.checked_mul(Decimal::ONE.checked_sub(slip)?)?, rules),
    }
}

/// 상 · 하한가 `(하한, 상한)`. 범위 안쪽으로 맞춘다(상한가는 호가 단위 아래로, 하한가는 위로).
pub(crate) fn price_band(
    reference: Decimal,
    rate: Decimal,
    rules: &MarketRules,
) -> Option<(Decimal, Decimal)> {
    let upper = floor_to_tick(
        reference.checked_mul(Decimal::ONE.checked_add(rate)?)?,
        rules,
    )?;
    let lower = ceil_to_tick(
        reference.checked_mul(Decimal::ONE.checked_sub(rate)?)?,
        rules,
    )?;
    Some((lower, upper))
}

/// `budget`으로 살 수 있는 가장 큰 정수 수량(`most` 이하).
pub(crate) fn affordable_quantity(
    budget: Decimal,
    price: Decimal,
    most: Decimal,
    rules: &MarketRules,
) -> Decimal {
    let fits =
        |quantity: Decimal| buy_cost(quantity, price, rules).is_some_and(|cost| cost <= budget);
    // 수수료를 넣은 한 주 값으로 어림한 뒤, 반올림 때문에 어긋난 한두 주를 맞춘다.
    let unit = price
        .checked_mul(Decimal::ONE + rules.commission_rate)
        .filter(|unit| *unit > Decimal::ZERO);
    let mut quantity = unit
        .and_then(|unit| budget.checked_div(unit))
        .map_or(Decimal::ZERO, |estimate| estimate.floor())
        .clamp(Decimal::ZERO, most);
    while quantity > Decimal::ZERO && !fits(quantity) {
        quantity -= Decimal::ONE;
    }
    while quantity < most && fits(quantity + Decimal::ONE) {
        quantity += Decimal::ONE;
    }
    quantity
}

#[cfg(test)]
mod tests {
    use super::{affordable_quantity, buy_cost, buy_hold, charge, market_price, price_band};
    use crate::{MarketRules, MoneyRounding, RoundingMode, Side};
    use rust_decimal::Decimal;

    fn d(text: &str) -> Decimal {
        text.parse().unwrap()
    }

    fn rules(mode: RoundingMode) -> MarketRules {
        let mut rules = MarketRules::kr();
        rules.commission_rate = d("0.00015");
        rules.market_slippage_rate = d("0.001");
        rules.rounding = MoneyRounding { scale: 0, mode };
        rules
    }

    #[test]
    fn 떼는_돈은_설정한_방향으로_묶는_돈의_수수료는_항상_올린다() {
        let down = rules(RoundingMode::Down);
        assert_eq!(
            charge(d("492100"), d("0.00015"), down.rounding),
            Some(d("73"))
        );
        assert_eq!(buy_hold(d("7"), d("70300"), &down), Some(d("492174")));
        assert_eq!(buy_cost(d("7"), d("70300"), &down), Some(d("492173")));
        // 딱 떨어지면 올려도 그대로다.
        assert_eq!(buy_hold(d("2"), d("70000"), &down), Some(d("140021")));
    }

    #[test]
    fn 넘치는_셈은_없음으로_돌려준다() {
        let huge = Decimal::MAX;
        assert_eq!(buy_hold(huge, d("2"), &rules(RoundingMode::Down)), None);
        assert_eq!(buy_cost(huge, d("2"), &rules(RoundingMode::Down)), None);
    }

    #[test]
    fn 시장가_값은_불리한_쪽_호가_단위로_맞춘다() {
        let rules = rules(RoundingMode::Down);
        assert_eq!(
            market_price(d("70200"), Side::Buy, &rules),
            Some(d("70300"))
        );
        assert_eq!(
            market_price(d("70000"), Side::Sell, &rules),
            Some(d("69900"))
        );
        // 49,940 × 1.001 = 49,989.94 → 50원 단위로 올려 50,000(위 구간의 100원 단위에도 맞는다).
        assert_eq!(
            market_price(d("49940"), Side::Buy, &rules),
            Some(d("50000"))
        );
    }

    #[test]
    fn 상하한가는_범위_안쪽_호가_단위로_맞춘다() {
        let rules = rules(RoundingMode::Down);
        assert_eq!(
            price_band(d("70000"), d("0.30"), &rules),
            Some((d("49000"), d("91000")))
        );
        // 기준가 33,350: 43,355 → 43,350, 23,345 → 23,350.
        assert_eq!(
            price_band(d("33350"), d("0.30"), &rules),
            Some((d("23350"), d("43350")))
        );
    }

    #[test]
    fn 살_수_있는_수량은_수수료까지_넣어_가장_큰_정수다() {
        for mode in [RoundingMode::Down, RoundingMode::HalfUp, RoundingMode::Up] {
            let rules = rules(mode);
            for budget in [
                "0", "80111", "85112", "400000", "425562", "425564", "9000000",
            ] {
                let budget = d(budget);
                let most = d("5");
                let quantity = affordable_quantity(budget, d("85100"), most, &rules);
                assert!(quantity >= d("0") && quantity <= most);
                if quantity > d("0") {
                    assert!(buy_cost(quantity, d("85100"), &rules).unwrap() <= budget);
                }
                if quantity < most {
                    let next = buy_cost(quantity + d("1"), d("85100"), &rules).unwrap();
                    assert!(next > budget, "{mode:?} {budget}: 한 주 더 살 수 있다");
                }
            }
        }
        assert_eq!(
            affordable_quantity(d("400000"), d("85100"), d("5"), &rules(RoundingMode::Down)),
            d("4")
        );
    }
}
