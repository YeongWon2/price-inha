//! (24차 C26 · E60 · V66) 익절 · 손절 연출 이벤트: 매매가 팔림이 되면 판 까닭(익절 · 손절)으로 `office.effect`를 낸다.
//!
//! - 까닭은 파는 주문을 낼 때 적어 둔다([`Trading::note_sell`]). 익절 · 손절이 아닌 까닭(청산 · 팀장 팔기)은
//!   연출하지 않는다. 적은 것은 이 프로그램이 떠 있는 동안만 쥔다(다시 켜기 전에 낸 주문은 연출하지 않는다 —
//!   연출은 화면만의 일이고 매매 기록과 상관없다, E60).
//! - 수익률은 서버가 센다: 그 매매로 낸 사는 · 파는 주문([`Trading::note_order`]로 적은 주문 번호)의 체결 기록
//!   (`PaperBroker::fills`, 읽기만 — 계좌 시계를 굴리지 않는다)에서 평균 체결 값을 내어 `(판 값 ÷ 산 값 − 1) × 100`을
//!   소수 둘째 자리까지 글자로(`"4.00"`, `"-5.00"`) 싣는다. 체결 기록을 찾지 못하면 칸을 싣지 않는다.
//! - 이벤트 모양: `{"kind": "take_profit" | "stop_loss", "agent": "trader", "trade", "symbol", "at", "return_pct"?}`.

use std::collections::HashMap;

use pi_paper::{Fill, OrderId, Side};
use pi_rules::Purpose;
use pi_store::{TradeId, TradeState};
use rust_decimal::Decimal;
use serde_json::json;

use super::Trading;
use super::base::{lock, millis};

/// (24차 C26) 익절 · 손절 연출 이벤트 종류.
pub const KIND_OFFICE_EFFECT: &str = "office.effect";
/// 연출을 받는 담당(매매 담당).
const EFFECT_AGENT: &str = "trader";

/// 연출을 위해 쥐는 것: 매매마다 판 까닭(익절 · 손절)과 그 매매로 낸 주문 번호들.
#[derive(Debug, Default)]
pub(crate) struct SellNotes {
    why: HashMap<TradeId, Purpose>,
    orders: HashMap<TradeId, Vec<OrderId>>,
}

/// 연출하는 까닭의 이벤트 글자. 익절 · 손절이 아니면 없음.
fn effect_kind(purpose: Purpose) -> Option<&'static str> {
    match purpose {
        Purpose::TakeProfit => Some("take_profit"),
        Purpose::StopLoss => Some("stop_loss"),
        _ => None,
    }
}

/// 그 주문들 · 방향 체결의 평균 값. 체결이 없으면 없음.
fn average(fills: &[Fill], orders: &[OrderId], side: Side) -> Option<Decimal> {
    let (amount, quantity) = fills
        .iter()
        .filter(|f| f.side == side && orders.contains(&f.order_id))
        .fold((Decimal::ZERO, Decimal::ZERO), |(a, q), f| {
            (a + f.amount, q + f.quantity)
        });
    (quantity > Decimal::ZERO).then(|| amount / quantity)
}

/// 산 값 · 판 값으로 수익률 글자(퍼센트, 소수 둘째 자리).
fn return_text(bought: Decimal, sold: Decimal) -> Option<String> {
    if bought <= Decimal::ZERO {
        return None;
    }
    let pct = (sold / bought - Decimal::ONE) * Decimal::ONE_HUNDRED;
    Some(format!("{:.2}", pct.round_dp(2)))
}

impl Trading {
    /// 파는 주문을 낼 때 그 까닭을 적는다(익절 · 손절만 쥔다. 다른 까닭이면 앞서 적은 것을 지운다).
    pub(super) fn note_sell(&self, trade: TradeId, purpose: Purpose) {
        let mut notes = lock(&self.sell_why);
        if effect_kind(purpose).is_some() {
            notes.why.insert(trade, purpose);
        } else {
            notes.why.remove(&trade);
        }
    }

    /// 실행기가 받은 주문 번호를 그 매매에 적는다(수익률을 셀 때 그 체결만 본다).
    pub(super) fn note_order<E>(&self, trade: TradeId, sent: &Result<OrderId, E>) {
        if let Ok(id) = sent {
            lock(&self.sell_why)
                .orders
                .entry(trade)
                .or_default()
                .push(id.clone());
        }
    }

    /// 적어 둔 매매 가운데 팔림이 된 것마다 연출 이벤트를 낸다(한 번만). 아직 보유중이면 그대로 둔다.
    pub(super) async fn announce_sold(&self) {
        let sold = {
            let mut notes = lock(&self.sell_why);
            if notes.why.is_empty() {
                return;
            }
            let mut done = Vec::new();
            notes.why.retain(|trade, purpose| {
                match self.store.get_trade(*trade).map(|t| t.state) {
                    Ok(TradeState::Holding | TradeState::Ordered) => true,
                    Ok(TradeState::Sold) => {
                        done.push((*trade, *purpose));
                        false
                    }
                    _ => false,
                }
            });
            done.into_iter()
                .map(|(trade, purpose)| {
                    let orders = notes.orders.remove(&trade).unwrap_or_default();
                    (trade, purpose, orders)
                })
                .collect::<Vec<_>>()
        };
        for (trade, purpose, orders) in sold {
            if let Err(error) = self.announce(trade, purpose, &orders).await {
                tracing::warn!(%error, trade = trade.0, "익절 · 손절 연출을 알리지 못했습니다");
            }
        }
    }

    async fn announce(
        &self,
        trade: TradeId,
        purpose: Purpose,
        orders: &[OrderId],
    ) -> Result<(), crate::TradingError> {
        let Some(kind) = effect_kind(purpose) else {
            return Ok(());
        };
        let symbol = self.store.get_trade(trade)?.symbol;
        let mut payload = json!({
            "kind": kind,
            "agent": EFFECT_AGENT,
            "trade": trade.0,
            "symbol": symbol,
            "at": millis(self.clock.now()).0,
        });
        let fills = self.paper().fills();
        let pct = average(&fills, orders, Side::Buy)
            .zip(average(&fills, orders, Side::Sell))
            .and_then(|(bought, sold)| return_text(bought, sold));
        if let (Some(pct), Some(map)) = (pct, payload.as_object_mut()) {
            map.insert("return_pct".to_owned(), json!(pct));
        }
        // 연출은 화면만의 일이다: 시세 · 주문 길을 기다리게 하지 않게 따로 띄운 작업에서 낸다(E60).
        match self.publisher.get().cloned() {
            Some(publisher) => {
                tokio::spawn(async move {
                    if let Err(error) = publisher.publish(KIND_OFFICE_EFFECT, payload).await {
                        tracing::warn!(%error, "익절 · 손절 연출을 알리지 못했습니다");
                    }
                });
                Ok(())
            }
            None => self.record(KIND_OFFICE_EFFECT, payload).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 수익률은_소수_둘째_자리_글자다() {
        assert_eq!(
            return_text(Decimal::from(100_000), Decimal::from(104_000)).as_deref(),
            Some("4.00")
        );
        assert_eq!(
            return_text(Decimal::from(100_000), Decimal::from(94_000)).as_deref(),
            Some("-6.00")
        );
        assert_eq!(
            return_text(Decimal::from(30_000), Decimal::from(31_000)).as_deref(),
            Some("3.33")
        );
        assert_eq!(return_text(Decimal::ZERO, Decimal::from(1)), None);
    }

    #[test]
    fn 익절_손절만_연출한다() {
        assert_eq!(effect_kind(Purpose::TakeProfit), Some("take_profit"));
        assert_eq!(effect_kind(Purpose::StopLoss), Some("stop_loss"));
        assert_eq!(effect_kind(Purpose::Liquidation), None);
        assert_eq!(effect_kind(Purpose::TeamExit), None);
    }
}
