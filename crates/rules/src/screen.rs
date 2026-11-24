//! 원칙 판정 하나하나(P1~P13)와 주문 직전 점검(P12). [`crate::check`]가 부른다.
//!
//! 테스트가 정하지 않은 것은 막는 쪽으로 정했다(여기 적는다).
//! - 지정가 주문에 AI 값이 없으면 시세를 AI 값 자리에 쓴다(사기는 `min(AI 값, 시세)`가 시세가 된다).
//! - 손절값은 주문 값(진입)과 근거의 진입가 **모두**보다 낮아야 한다.
//! - 이미 가진 종목이 손실 중이면 새 진입이든 추가 매수든 사기를 막는다(P6).
//! - 확신 종목 비중(`conviction_weight_pct`)은 주문 안에 확신 표시가 없어 쓰지 않는다(보통 종목 비중을 쓴다).
//! - 주문 안의 시장이 달력의 시장과 다르면 그 시장의 장 시간을 모르므로 장 닫힘으로 본다.
//! - 가격제한은 AI가 낸 지정가와 최종 주문 값 둘 다 본다. 상한가는 호가 단위 아래로, 하한가는 위로 맞춘다.
//! - 손실 중 · 업종 · 비중의 평가 금액은 보유 종목의 지금 값(`last_price`)으로 센다.

use std::time::Duration;

use pi_market::{StalePolicy, is_stale};
use pi_paper::{OrderType, Side};
use pi_template::AbsoluteCaps;
use rust_decimal::Decimal;

use crate::loss::{day_loss_reached, drawdown_reached};
use crate::sizing::{
    align_down_to_tick, align_up_to_tick, net_expected_value, percent_of, size_stages,
};
use crate::{
    CheckContext, Funding, OrderDraft, ProductKind, Purpose, Rationale, RationaleField,
    SizeRequest, Violation, size,
};

/// 늦은 시세 기준의 상한(P11): 받은 설정이 더 길어도 30초로 막는다.
const STALE_CAP: Duration = Duration::from_secs(30);

/// 규칙 엔진이 셈한 주문의 수량 · 값 · 금액.
pub(crate) struct Plan {
    /// 수량.
    pub(crate) quantity: Decimal,
    /// 주문 값. 시장가면 `None`.
    pub(crate) price: Option<Decimal>,
    /// 주문 금액.
    pub(crate) amount: Decimal,
}

/// 빠진 근거 칸들([`RationaleField::ALL`] 순서).
pub(crate) fn missing_fields(rationale: &Rationale) -> Vec<RationaleField> {
    let blank = |text: &Option<String>| text.as_deref().is_none_or(|t| t.trim().is_empty());
    RationaleField::ALL
        .into_iter()
        .filter(|field| match field {
            RationaleField::Hypothesis => blank(&rationale.hypothesis),
            RationaleField::Trigger => blank(&rationale.trigger),
            RationaleField::Entry => rationale.entry.is_none(),
            RationaleField::Stop => rationale.stop.is_none(),
            RationaleField::Target => rationale.target.is_none(),
            RationaleField::ExpectedValue => rationale.expected_value.is_none(),
        })
        .collect()
}

/// 사는 주문의 수량 · 값과 사는 주문에만 보는 원칙들.
pub(crate) fn plan_buy(draft: &OrderDraft, ctx: &CheckContext<'_>) -> (Plan, Vec<Violation>) {
    let (price, entry) = buy_price(draft, ctx);
    // 손절값이 없으면(근거 점검을 지나 올 수 없지만) 진입가로 두어 막히게 한다.
    let stop = draft.rationale.stop.unwrap_or(entry);
    let stop_ok = stop < entry && draft.rationale.entry.is_none_or(|e| stop < e);

    let mut found = book_violations(draft, ctx);
    if !stop_ok {
        found.push(Violation::StopNotBelowEntry);
    }
    let quantity = buy_quantity(draft, ctx, entry, stop, stop_ok, &mut found);
    found.extend(health_violations(ctx));
    found.extend(status_violations(ctx));
    let edge = draft
        .rationale
        .expected_value
        .map(|ev| net_expected_value(ev, entry, ctx.market_rules));
    if edge.is_none_or(|net| net <= Decimal::ZERO) {
        found.push(Violation::NoPositiveEdge);
    }
    let amount = quantity * entry;
    (
        Plan {
            quantity,
            price,
            amount,
        },
        found,
    )
}

/// 사는 주문의 값: 지정가는 `min(AI 값, 시세)`를 호가 단위로 내린 값, 시장가는 값이 없고
/// 금액은 `시세 × (1 + 미끄러짐)`으로 센다. (주문 값, 수량 · 금액에 쓰는 진입가).
fn buy_price(draft: &OrderDraft, ctx: &CheckContext<'_>) -> (Option<Decimal>, Decimal) {
    let quote = ctx.quote.price;
    match draft.order_type {
        OrderType::Limit => {
            let base = draft.limit_price.map_or(quote, |ai| ai.min(quote));
            let price = align_down_to_tick(base, ctx.market_rules);
            (Some(price), price)
        }
        OrderType::Market => (
            None,
            quote * (Decimal::ONE + ctx.market_rules.market_slippage_rate),
        ),
    }
}

/// 장부와 상품으로 보는 원칙: P1 통화 · P3 · P4 · P5 · P6.
fn book_violations(draft: &OrderDraft, ctx: &CheckContext<'_>) -> Vec<Violation> {
    let mut found = Vec::new();
    let currency = &ctx.market_rules.currency;
    if &ctx.book.currency != currency || &ctx.quote.currency != currency {
        found.push(Violation::CurrencyMismatch);
    }
    let books = std::slice::from_ref(ctx.book);
    if day_loss_reached(ctx.params, books) {
        found.push(Violation::DailyLossLimitReached);
    }
    if drawdown_reached(ctx.params, books) {
        found.push(Violation::DrawdownLimitReached);
    }
    if !matches!(draft.product, ProductKind::Stock | ProductKind::Etf) {
        found.push(Violation::BannedProduct(draft.product));
    }
    if draft.funding != Funding::Cash {
        found.push(Violation::BannedFunding(draft.funding));
    }
    let banned = ctx
        .config
        .banned_symbols
        .iter()
        .any(|(market, symbol)| *market == draft.market && *symbol == draft.symbol);
    if banned {
        found.push(Violation::BannedSymbol);
    }
    let losing = ctx.book.positions.iter().any(|p| {
        p.market == draft.market
            && p.symbol == draft.symbol
            && p.last_price.min(ctx.quote.price) < p.avg_price
    });
    if losing {
        found.push(Violation::AveragingDown);
    }
    found
}

/// 살 수량(P1 · P7 · P8). 0이 되게 한 한도마다 위반을 더한다.
fn buy_quantity(
    draft: &OrderDraft,
    ctx: &CheckContext<'_>,
    entry: Decimal,
    stop: Decimal,
    stop_ok: bool,
    found: &mut Vec<Violation>,
) -> Decimal {
    let (request, sector_known) = buy_size_request(draft, ctx, entry, stop);
    if !sector_known {
        found.push(Violation::UnknownSector);
    }
    let [risk, symbol, sector] = size_stages(&request);
    if stop_ok && risk <= Decimal::ZERO {
        found.push(Violation::RiskTooLarge);
    }
    if symbol <= Decimal::ZERO {
        found.push(Violation::SymbolWeightFull);
    }
    if sector_known && sector <= Decimal::ZERO {
        found.push(Violation::SectorWeightFull);
    }
    size(&request)
}

/// 살 수량을 정하는 입력([`SizeRequest`])과 업종을 아는지. 점검([`buy_quantity`])과 한도 적용 기록
/// ([`buy_size_stages`])이 함께 쓴다(같은 셈을 따로 짜지 않는다). 업종을 모르면 같은 종목의 금액을
/// 업종 금액으로 둔다(위반은 부른 쪽이 더한다).
fn buy_size_request(
    draft: &OrderDraft,
    ctx: &CheckContext<'_>,
    entry: Decimal,
    stop: Decimal,
) -> (SizeRequest, bool) {
    let caps = AbsoluteCaps::FIXED;
    let params = ctx.params;
    let held_symbol = held_value(ctx, |p| {
        p.market == draft.market && p.symbol == draft.symbol
    });
    let sector = ctx.config.sectors.get(&draft.symbol);
    let held_sector = match sector {
        Some(name) => held_value(ctx, |p| ctx.config.sectors.get(&p.symbol) == Some(name)),
        None => held_symbol,
    };
    let request = SizeRequest {
        equity: ctx.book.equity,
        risk_pct: params.risk_per_trade_pct,
        entry,
        stop,
        symbol_weight_pct: params.symbol_weight_pct.min(caps.symbol_weight_pct),
        sector_weight_pct: params.sector_weight_pct.min(caps.sector_weight_pct),
        held_symbol_value: held_symbol,
        held_sector_value: held_sector,
        ai_quantity: draft.ai_quantity,
    };
    (request, sector.is_some())
}

/// 사는 주문의 한도별 수량 `[1회 위험, 종목 비중, 업종 비중]`과 AI 수량의 내림.
/// [`crate::limit_clamps`]가 점검이 실제로 쓰는 셈을 단계별로 읽는 길이다.
pub(crate) fn buy_size_stages(
    draft: &OrderDraft,
    ctx: &CheckContext<'_>,
) -> ([Decimal; 3], Option<Decimal>) {
    let (_, entry) = buy_price(draft, ctx);
    let stop = draft.rationale.stop.unwrap_or(entry);
    let (request, _) = buy_size_request(draft, ctx, entry, stop);
    (
        size_stages(&request),
        request.ai_quantity.map(|ai| ai.floor()),
    )
}

/// 고른 보유 종목들의 평가 금액(수량 × 지금 값).
fn held_value(ctx: &CheckContext<'_>, pick: impl Fn(&crate::Position) -> bool) -> Decimal {
    ctx.book
        .positions
        .iter()
        .filter(|p| pick(p))
        .map(|p| p.quantity * p.last_price)
        .sum()
}

/// P11 · N6: 자료와 연결의 상태.
fn health_violations(ctx: &CheckContext<'_>) -> Vec<Violation> {
    let mut found = Vec::new();
    let policy = StalePolicy {
        max_age: ctx.stale.max_age.min(STALE_CAP),
    };
    if is_stale(ctx.quote, ctx.calendar, ctx.now, &policy) {
        found.push(Violation::StaleQuote);
    }
    if !ctx.health.account_in_sync {
        found.push(Violation::AccountMismatch);
    }
    if ctx.health.api_error {
        found.push(Violation::ApiError);
    }
    if ctx.health.data_anomaly {
        found.push(Violation::DataAnomaly);
    }
    found
}

/// P12 가운데 사는 주문에만 보는 것: VI · 거래정지(모르면 막는다) · 일일 매매 횟수.
fn status_violations(ctx: &CheckContext<'_>) -> Vec<Violation> {
    let mut found = Vec::new();
    if ctx.status.vi == Some(true) {
        found.push(Violation::ViActive);
    }
    if ctx.status.halted == Some(true) {
        found.push(Violation::TradingHalted);
    }
    if ctx.status.vi.is_none() || ctx.status.halted.is_none() {
        found.push(Violation::StatusUnknown);
    }
    if ctx.trades_today >= ctx.params.max_trades_per_day {
        found.push(Violation::TradeCountReached);
    }
    found
}

/// 파는 주문의 수량 · 값. 수량은 `min(floor(AI 수량), 가진 수량)`(AI 수량이 없으면 가진 수량),
/// 지정가는 AI 값(없으면 시세)을 호가 단위로 내린 값. 가진 것이 없으면 공매도(P5)다.
pub(crate) fn plan_sell(draft: &OrderDraft, ctx: &CheckContext<'_>) -> (Plan, Vec<Violation>) {
    let held: Decimal = ctx
        .book
        .positions
        .iter()
        .filter(|p| p.market == draft.market && p.symbol == draft.symbol)
        .map(|p| p.quantity)
        .sum();
    let mut found = Vec::new();
    if held <= Decimal::ZERO {
        found.push(Violation::ShortSale);
    }
    let quantity = draft
        .ai_quantity
        .map_or(held, |ai| ai.floor().min(held))
        .max(Decimal::ZERO);
    let quote = ctx.quote.price;
    let (price, amount) = match draft.order_type {
        OrderType::Limit => {
            let price = align_down_to_tick(draft.limit_price.unwrap_or(quote), ctx.market_rules);
            (Some(price), quantity * price)
        }
        OrderType::Market => (None, quantity * quote),
    };
    (
        Plan {
            quantity,
            price,
            amount,
        },
        found,
    )
}

/// 모든 주문에 보는 것: P10, 장 시간 · 세션의 호가 유형 · 가격제한 · 중복 · 1회 최대 금액 · API 한도.
pub(crate) fn common_violations(
    draft: &OrderDraft,
    ctx: &CheckContext<'_>,
    plan: &Plan,
) -> Vec<Violation> {
    let mut found = Vec::new();
    if draft.sources.iter().any(|s| s.known_at > draft.decided_at) {
        found.push(Violation::DataFromFuture);
    }
    let session = (draft.market == ctx.calendar.market && ctx.calendar.covers(ctx.now))
        .then(|| ctx.calendar.session_at(ctx.now))
        .flatten();
    match session {
        None => found.push(Violation::MarketClosed),
        Some(session)
            if !ctx
                .market_rules
                .sessions
                .allows(session.kind, draft.order_type) =>
        {
            found.push(Violation::OrderTypeNotAllowed);
        }
        Some(_) => {}
    }
    if outside_price_limit(draft, ctx, plan) {
        found.push(Violation::OutsidePriceLimit);
    }
    if ctx.quote.symbol != draft.symbol {
        found.push(Violation::QuoteSymbolMismatch);
    }
    let side: Side = draft.purpose.side();
    // 손절 · 청산 · 팀장 판단 매도는 열린 익절 매도에 막히지 않는다(열린 주문 취소는 exec가 한다).
    let exempt = matches!(
        draft.purpose,
        Purpose::StopLoss | Purpose::Liquidation | Purpose::TeamExit
    );
    let duplicate = !exempt
        && ctx
            .open_orders
            .iter()
            .any(|o| o.market == draft.market && o.symbol == draft.symbol && o.side == side);
    if duplicate {
        found.push(Violation::DuplicateOrder);
    }
    if ctx
        .market_rules
        .max_order_amount
        .is_some_and(|max| plan.amount > max)
    {
        found.push(Violation::OverMaxOrderAmount);
    }
    if ctx.api.used >= ctx.api.limit {
        found.push(Violation::ApiLimitReached);
    }
    found
}

/// AI 지정가나 최종 주문 값이 `기준가 × (1 ± 제한폭)` 밖인가. 제한이 없거나 시장가면 거짓.
fn outside_price_limit(draft: &OrderDraft, ctx: &CheckContext<'_>, plan: &Plan) -> bool {
    let rules = ctx.market_rules;
    let Some(rate) = rules.price_limit_rate else {
        return false;
    };
    if draft.order_type != OrderType::Limit {
        return false;
    }
    let reference = ctx.reference_price;
    let (Some(up), Some(down)) = (
        percent_of(
            reference,
            Decimal::ONE_HUNDRED + rate * Decimal::ONE_HUNDRED,
        ),
        percent_of(
            reference,
            Decimal::ONE_HUNDRED - rate * Decimal::ONE_HUNDRED,
        ),
    ) else {
        // 셈이 넘치면 범위를 모른다 → 막는다.
        return true;
    };
    let upper = align_down_to_tick(up, rules);
    let lower = align_up_to_tick(down, rules);
    [draft.limit_price, plan.price]
        .into_iter()
        .flatten()
        .any(|price| price > upper || price < lower)
}
