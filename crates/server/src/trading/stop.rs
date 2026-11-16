//! 손절 감시(F4 대안 흐름 1a): 시세가 들어올 때마다 손절값에 닿은 보유 매매를 논의 없이 손절로 판다.
//!
//! `pi_exec::Executor::stop_hits` → `stop_draft`(시장가 · 상품 모름, exec 3차 결정) → 주문 길
//! ([`Trading::place_order`]: 규칙 점검 → 매매에 묶은 점검 표시 → `Executor::sell(StopLoss)`).
//! 어떤 멈춤에서도 손절은 받는다(E34, 쉬는 날 검사도 건너뛴다). 이상 감지 멈춤 요구는 주문 길이
//! 팀 상태로 옮긴다(E13). 같은 매매의 손절은 한 번만 낸다(E30).

use pi_market::Quote;
use pi_paper::{Broker, OrderGroup, OrderId};
use pi_rules::SymbolStatus;
use pi_store::{TradeId, TradeState};

use super::base::lock;
use super::{OrderRequest, Placed, Trading};

impl Trading {
    /// 그 매매에 낸 손절 주문이 아직 모의계좌에 걸려 있거나 매매가 더는 보유중이 아니면 그 주문 번호.
    /// 주문이 걸려 있지 않은데 아직 보유중이면(취소됨) 기억을 지운다.
    pub(super) async fn stop_sent(&self, trade: TradeId) -> Option<OrderId> {
        let id = lock(&self.stops).get(&trade).cloned()?;
        let now = self.clock.now();
        let open = Broker::orders(self.executor.broker(), OrderGroup::Open, None, now)
            .await
            .map(|orders| orders.iter().any(|o| o.order_id == id))
            .unwrap_or(true);
        let holding = self
            .store
            .get_trade(trade)
            .is_ok_and(|t| t.state == TradeState::Holding);
        if open || !holding {
            return Some(id);
        }
        lock(&self.stops).remove(&trade);
        None
    }

    /// 낸 손절 주문을 기억한다.
    pub(super) fn note_stop(&self, trade: TradeId, placed: &Placed) {
        if let Placed::Ordered(id) = placed {
            lock(&self.stops).insert(trade, id.clone());
        }
    }

    /// 그 시세로 손절값에 닿은 매매마다 손절 주문을 낸다. 막힘 · 실패는 기록에 남기고 다음 시세에
    /// 다시 본다(시세 받기는 멈추지 않는다).
    pub(super) async fn watch_stops(&self, quote: &Quote) {
        for guard in self.executor.stop_hits(quote) {
            let trade = guard.trade_id;
            if self.stop_sent(trade).await.is_some() {
                continue;
            }
            let draft = match self.executor.stop_draft(&guard, quote) {
                Ok(draft) => draft,
                Err(error) => {
                    tracing::warn!(%error, trade = trade.0, "손절 주문 안을 만들지 못했다");
                    continue;
                }
            };
            let request = OrderRequest {
                trade_id: trade,
                draft,
                // 기준가를 따로 모르면 그 시세를 쓴다(시장가로 파는 주문이다).
                reference_price: quote.price,
                // 파는 주문은 VI · 거래정지를 모른다고 막지 않는다.
                status: SymbolStatus {
                    vi: None,
                    halted: None,
                },
            };
            match self.place_order(request).await {
                Ok(Placed::Ordered(_)) => {}
                Ok(other) => tracing::warn!(trade = trade.0, ?other, "손절 주문을 내지 못했다"),
                Err(error) => tracing::warn!(%error, trade = trade.0, "손절 주문을 내지 못했다"),
            }
        }
    }
}
