//! (24차 C26 · E60 · V66) 익절 · 손절 연출 이벤트(뼈대 — red).

use pi_rules::Purpose;
use pi_store::TradeId;

use super::Trading;
use super::base::lock;

/// (24차 C26) 익절 · 손절 연출 이벤트 종류.
pub const KIND_OFFICE_EFFECT: &str = "office.effect";

impl Trading {
    /// 파는 주문을 낼 때 그 까닭을 적는다(뼈대).
    pub(super) fn note_sell(&self, trade: TradeId, purpose: Purpose) {
        let _ = (lock(&self.sell_why).len(), trade, purpose, KIND_OFFICE_EFFECT);
    }

    /// 팔림이 된 매매마다 연출 이벤트를 낸다(뼈대).
    pub(super) async fn announce_sold(&self) {}
}
