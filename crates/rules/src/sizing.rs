//! 수량 계산(P1 · P7 · P8), 호가 단위, 비용을 뺀 기대값(P13).

use pi_paper::MarketRules;
use pi_template::AbsoluteCaps;
use rust_decimal::Decimal;

/// 수량 계산의 입력. 모두 한 통화다(통화는 섞지 않는다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeRequest {
    /// 그 통화의 평가액.
    pub equity: Decimal,
    /// 1회 위험 비율 R%(템플릿 값). 상한 2%로 다시 막는다.
    pub risk_pct: Decimal,
    /// 진입가(주문 값).
    pub entry: Decimal,
    /// 손절값.
    pub stop: Decimal,
    /// 종목당 최대 비중%(템플릿 값). 상한 25%로 다시 막는다.
    pub symbol_weight_pct: Decimal,
    /// 업종당 최대 비중%(템플릿 값). 상한 50%로 다시 막는다.
    pub sector_weight_pct: Decimal,
    /// 이미 가진 그 종목의 평가 금액.
    pub held_symbol_value: Decimal,
    /// 이미 가진 그 업종(이 종목 포함)의 평가 금액.
    pub held_sector_value: Decimal,
    /// AI가 낸 수량. 있으면 작은 쪽(P7).
    pub ai_quantity: Option<Decimal>,
}

/// 살 수량(0 이상 정수). 다음 값들 가운데 가장 작은 것이고, 0보다 작으면 0이다.
///
/// - P1: `floor(평가액 × min(R, 2) / 100 / (진입 − 손절))`. 진입 ≤ 손절이면 0.
/// - P8 종목: `floor((평가액 × min(종목%, 25) / 100 − 가진 종목 금액) / 진입)`.
/// - P8 업종: `floor((평가액 × min(업종%, 50) / 100 − 가진 업종 금액) / 진입)`.
/// - P7: `floor(AI 수량)`(있으면).
///
/// 셈이 넘치면(값이 너무 크면) 그 한도의 수량은 0이다(막는 쪽).
pub fn size(request: &SizeRequest) -> Decimal {
    let [risk, symbol, sector] = size_stages(request);
    let mut quantity = risk.min(symbol).min(sector);
    if let Some(ai) = request.ai_quantity {
        quantity = quantity.min(ai.floor());
    }
    quantity.max(Decimal::ZERO)
}

/// [`size`]가 `min`으로 합치는 한도별 수량 `[1회 위험, 종목 비중, 업종 비중]`. 한도 적용 기록
/// ([`crate::limit_clamps`])이 같은 셈을 단계별로 읽는 길이다.
pub(crate) fn size_stages(request: &SizeRequest) -> [Decimal; 3] {
    let caps = AbsoluteCaps::FIXED;
    [
        risk_quantity(
            request.equity,
            request.risk_pct,
            request.entry,
            request.stop,
        ),
        weight_quantity(
            request.equity,
            request.symbol_weight_pct.min(caps.symbol_weight_pct),
            request.held_symbol_value,
            request.entry,
        ),
        weight_quantity(
            request.equity,
            request.sector_weight_pct.min(caps.sector_weight_pct),
            request.held_sector_value,
            request.entry,
        ),
    ]
}

/// P1의 수량: `floor(평가액 × min(R, 2) / 100 / (진입 − 손절))`. 진입 ≤ 손절이면 0.
pub(crate) fn risk_quantity(
    equity: Decimal,
    risk_pct: Decimal,
    entry: Decimal,
    stop: Decimal,
) -> Decimal {
    if entry <= stop {
        return Decimal::ZERO;
    }
    let pct = risk_pct.min(AbsoluteCaps::FIXED.risk_per_trade_pct);
    match percent_of(equity, pct) {
        Some(budget) => floor_within(budget, entry - stop),
        None => Decimal::ZERO,
    }
}

/// P8의 수량: `floor((평가액 × 비중% / 100 − 가진 금액) / 진입)`. 비중%는 이미 상한으로 막은 값이다.
pub(crate) fn weight_quantity(
    equity: Decimal,
    pct: Decimal,
    held: Decimal,
    entry: Decimal,
) -> Decimal {
    match percent_of(equity, pct).and_then(|room| room.checked_sub(held)) {
        Some(room) => floor_within(room, entry),
        None => Decimal::ZERO,
    }
}

/// `값 × 퍼센트 / 100`. 넘치면 `None`.
pub(crate) fn percent_of(value: Decimal, pct: Decimal) -> Option<Decimal> {
    value
        .checked_mul(pct)
        .and_then(|scaled| scaled.checked_div(Decimal::ONE_HUNDRED))
}

/// `수량 × 단가 ≤ 예산`인 가장 큰 0 이상 정수. 단가가 0 이하이거나 셈이 넘치면 0이다.
/// 나눗셈의 반올림으로 한 주가 더 붙지 않게 곱해서 다시 확인한다.
pub(crate) fn floor_within(budget: Decimal, unit: Decimal) -> Decimal {
    if unit <= Decimal::ZERO || budget <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    let Some(mut quantity) = budget.checked_div(unit).map(|q| q.floor()) else {
        return Decimal::ZERO;
    };
    while quantity > Decimal::ZERO && quantity.checked_mul(unit).is_none_or(|cost| cost > budget) {
        quantity -= Decimal::ONE;
    }
    quantity
}

/// 값을 그 구간의 호가 단위([`MarketRules::tick_for`])로 내린다. 이미 맞으면 그대로다.
/// 호가 단위가 0 이하(표가 비었음)이면 값을 그대로 둔다.
pub fn align_down_to_tick(price: Decimal, rules: &MarketRules) -> Decimal {
    let tick = rules.tick_for(price);
    if tick <= Decimal::ZERO {
        return price;
    }
    price
        .checked_div(tick)
        .map_or(price, |steps| steps.floor() * tick)
}

/// 값을 그 구간의 호가 단위로 올린다(하한가를 범위 안쪽으로 맞출 때 쓴다).
pub(crate) fn align_up_to_tick(price: Decimal, rules: &MarketRules) -> Decimal {
    let tick = rules.tick_for(price);
    if tick <= Decimal::ZERO {
        return price;
    }
    price
        .checked_div(tick)
        .map_or(price, |steps| steps.ceil() * tick)
}

/// 비용을 뺀 주당 기대값(P13).
///
/// `기대값 − 진입 × (2 × 수수료율 + 매도 세율 + 2 × 미끄러짐)`. 사고팔 때 수수료와 미끄러짐을
/// 한 번씩, 팔 때 세금을 한 번 뗀다(보수적으로 파는 값도 진입가로 센다). 반올림하지 않는다.
pub fn net_expected_value(expected_value: Decimal, entry: Decimal, rules: &MarketRules) -> Decimal {
    let two = Decimal::TWO;
    let cost_rate =
        two * rules.commission_rate + rules.sell_tax_rate + two * rules.market_slippage_rate;
    expected_value - entry * cost_rate
}
