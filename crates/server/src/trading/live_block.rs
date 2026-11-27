//! (백로그 Q · E25) 실전중 템플릿의 사는 주문이 실행기에서 막히면 그 매매에 막힌 까닭을 남긴다.
//!
//! 실전중 템플릿의 주문은 실전 계좌의 것이고 실전 주문은 만들지 않으므로 실행기가 계좌 종류가 다르다고 막는다
//! (`pi_exec::Error::AccountKindMismatch`). 그때 매매가 판단됨에 남아 까닭 없이 멈춘 것처럼 보이지 않게, 판단됨이면
//! 막힘으로 옮기고 [`LIVE_ORDER_BLOCKED`]를 남긴다(E25 "보이는 것: 막힌 이유가 보인다").

use pi_store::{BlockKind, BlockReason, TradeId, TradeState, UnixMillis};

use super::Trading;

/// 실전중 템플릿이라 사는 주문을 내지 않았을 때 매매에 남기는 까닭.
pub(crate) const LIVE_ORDER_BLOCKED: &str = "쓰는 템플릿이 실전중이라 주문을 내지 않았습니다(실전 주문은 \
만들지 않습니다 · E25). 템플릿을 모의중으로 되돌리거나 새 템플릿을 고르세요";

impl Trading {
    /// 실행기가 계좌 종류가 달라 사는 주문을 막았다: 판단됨인 매매를 막힘으로 옮기고 까닭을 남긴다. 남기지 못하면
    /// 기록만 한다(주문이 나가지 않은 것은 그대로다).
    pub(super) fn live_blocked(&self, trade: TradeId) {
        let judged = self
            .store
            .get_trade(trade)
            .is_ok_and(|found| found.state == TradeState::Judged);
        if !judged {
            return;
        }
        let reason = BlockReason {
            kind: BlockKind::PrincipleOrRationale,
            detail: LIVE_ORDER_BLOCKED.to_string(),
        };
        let at = UnixMillis(self.clock.now().timestamp_millis());
        if let Err(error) = self.store.block_trade(trade, reason, at) {
            tracing::warn!(%error, trade = trade.0, "실전중 템플릿으로 막힌 매매에 까닭을 남기지 못했습니다");
        }
    }
}
