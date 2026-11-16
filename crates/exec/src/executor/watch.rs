//! 손절 감시(3차): 시세가 손절값에 닿은 감시 자리를 찾고 손절 주문 안을 만든다(F4 대안 흐름 1a).

use pi_market::Quote;
use pi_rules::OrderDraft;

use super::{Executor, Guard};
use crate::error::Error;

impl Executor {
    /// 시세가 손절값에 닿은 감시 자리들.
    pub fn stop_hits(&self, _quote: &Quote) -> Vec<Guard> {
        todo!()
    }

    /// 손절 주문 안.
    pub fn stop_draft(&self, _guard: &Guard, _quote: &Quote) -> Result<OrderDraft, Error> {
        todo!()
    }
}
