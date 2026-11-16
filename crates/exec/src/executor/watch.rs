//! 손절 감시(3차): 시세가 손절값에 닿은 감시 자리를 찾고 손절 주문 안을 만든다(F4 대안 흐름 1a).
//!
//! 팀을 거치지 않는다(어떤 멈춤에서도 손절은 받는다, E34). 점검(rules)과 표시 · [`Executor::sell`]로
//! 잇는 것은 조립하는 쪽이다(1a2: 3으로 돌아간다).

use chrono::{DateTime, Utc};
use pi_market::{Currency, Market, Quote, Symbol};
use pi_paper::OrderType;
use pi_rules::{Funding, OrderDraft, ProductKind, Purpose, Rationale, SourceStamp, TradeStage};
use pi_store::{Amount, TradeState};
use rust_decimal::Decimal;

use super::{Executor, Guard, SellPurpose};
use crate::error::Error;

/// 손절 주문 안의 자료 이름(그 시세).
const QUOTE_SOURCE: &str = "시세";

/// 저장소 금액(통화의 최소 단위)을 그 시장 통화의 값으로: 국내는 원, 미국은 센트(÷100).
/// 모르는 시장이면 없음.
fn price_of(market: Market, amount: Amount) -> Option<Decimal> {
    match market {
        Market::Kr => Some(Decimal::from(amount.0)),
        Market::Us => Some(Decimal::new(amount.0, 2)),
        _ => None,
    }
}

/// 그 시장의 통화인가.
fn same_currency(market: Market, currency: &Currency) -> bool {
    matches!(
        (market, currency),
        (Market::Kr, Currency::Krw) | (Market::Us, Currency::Usd)
    )
}

impl Executor {
    /// 시세가 손절값에 닿은(시세 값 ≤ 손절값) 감시 자리들. 매매 번호 순서다.
    ///
    /// 같은 종목 · 그 시장 통화의 시세만 본다. 손절 · 청산을 다시 내기를 기다리는 매매는 뺀다
    /// (이미 내고 있다, E30). 팀 상태와 상관없이 본다(E34).
    pub fn stop_hits(&self, quote: &Quote) -> Vec<Guard> {
        let state = self.lock();
        state
            .guards()
            .into_iter()
            .filter(|guard| {
                guard.symbol == quote.symbol
                    && same_currency(guard.market, &quote.currency)
                    && price_of(guard.market, guard.stop).is_some_and(|stop| quote.price <= stop)
                    && !state.pending.get(&guard.trade_id).is_some_and(|p| {
                        matches!(
                            p.retry.purpose,
                            SellPurpose::StopLoss | SellPurpose::Liquidation
                        )
                    })
            })
            .collect()
    }

    /// 손절 감시 자리로 손절 주문 안을 논의 없이 만든다(1a1).
    ///
    /// 목적 손절 · 시장가 · 수량은 그 매매의 남은 수량(장부가 없으면 감시 자리의 수량) · 현금 ·
    /// 판단 시각은 지금 · 근거는 매매의 것(금액은 그 시장 통화의 값) · 자료는 매매의 자료에 그 시세를
    /// 더한 것. 상품 종류는 모름이다(파는 주문은 상품을 보지 않는다).
    /// 매매가 보유중이 아니면 [`Error::NotHolding`].
    pub fn stop_draft(&self, guard: &Guard, quote: &Quote) -> Result<OrderDraft, Error> {
        let trade = self.store.get_trade(guard.trade_id)?;
        if trade.state != TradeState::Holding {
            return Err(Error::NotHolding(trade.state));
        }
        let quantity = self
            .lock()
            .remaining(guard.trade_id)
            .unwrap_or(guard.quantity);
        let price = |amount: Option<Amount>| amount.and_then(|a| price_of(guard.market, a));
        let given = &trade.rationale;
        let rationale = Rationale {
            hypothesis: given.hypothesis.clone(),
            trigger: given.trigger.clone(),
            entry: price(given.entry),
            stop: price(given.stop),
            target: price(given.target),
            expected_value: price(given.expected_value),
        };
        let mut sources: Vec<SourceStamp> = trade
            .sources
            .iter()
            .filter_map(|s| {
                Some(SourceStamp {
                    name: s.name.clone(),
                    known_at: DateTime::<Utc>::from_timestamp_millis(s.known_at.0)?,
                })
            })
            .collect();
        sources.push(SourceStamp {
            name: QUOTE_SOURCE.to_string(),
            known_at: quote.received_at,
        });
        Ok(OrderDraft {
            market: guard.market,
            symbol: Symbol::new(trade.symbol),
            purpose: Purpose::StopLoss,
            order_type: OrderType::Market,
            limit_price: None,
            ai_quantity: Some(quantity),
            funding: Funding::Cash,
            product: ProductKind::Unknown,
            stage: TradeStage::Judged,
            decided_at: self.clock.now(),
            rationale,
            sources,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_are_won_or_cents() {
        assert_eq!(
            price_of(Market::Kr, Amount(65_000)),
            Some(Decimal::from(65_000))
        );
        assert_eq!(
            price_of(Market::Us, Amount(15_050)),
            Some(Decimal::new(15_050, 2))
        );
        assert!(same_currency(Market::Us, &Currency::Usd));
        assert!(!same_currency(Market::Kr, &Currency::Usd));
    }
}
