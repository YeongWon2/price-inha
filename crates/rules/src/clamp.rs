//! 한도 적용 기록(5차, 결정 17 · R4-4 · C24 · T31): 주문 전 점검이 수량을 무엇으로 얼마나 줄였는가.
//!
//! 판정과 수량은 [`crate::check`] · [`crate::reduce_within_limit`]이 그대로 정한다. 여기는 그 셈을
//! 따로 읽는 길일 뿐이라 기존 구조체의 칸을 더하지 않는다.

use rust_decimal::Decimal;

use crate::check::evaluate;
use crate::screen::buy_size_stages;
use crate::{Blocked, CheckContext, OrderDraft, Passed, PendingApproval, reduce_within_limit};

/// 수량을 줄인 한도의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    /// 1회 위험 한도(P1).
    RiskPerTrade,
    /// 종목 비중 한도(P8).
    SymbolWeight,
    /// 확신 종목 비중 한도.
    ConvictionWeight,
    /// 업종 비중 한도(P8).
    SectorWeight,
    /// 자동 주문 금액 한도(E21 · E22).
    AutoOrderLimit,
    /// 최소 현금.
    MinCash,
    /// 동시 보유 수.
    MaxPositions,
}

/// 한도 하나가 수량을 줄인 한 줄. 여러 줄이면 적용한 순서이고 앞 줄의 `after`가 다음 줄의 `before`다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitClamp {
    /// 줄인 한도.
    pub limit: LimitKind,
    /// 줄이기 전 수량.
    pub before: Decimal,
    /// 줄인 뒤 수량.
    pub after: Decimal,
}

/// [`crate::check`]가 사는 주문의 수량을 크기 정하기(1회 위험 → 종목 비중 → 업종 비중 순)로 줄인
/// 기록. 줄이지 않았거나, 막혔거나, 파는 주문이거나, AI 수량이 없으면 빈 목록이다. 시작 수량은
/// AI 수량의 내림이고, 한도 수량이 그보다 작을 때만 한 줄이 된다. 자동 주문 금액 한도는 여기 담지
/// 않는다(승인을 기다리게 할 뿐 줄이지 않는다).
///
/// 한도별 수량은 점검이 쓰는 [`crate::size`]와 같은 함수(`size_stages`)로 읽는다.
pub fn limit_clamps(draft: &OrderDraft, ctx: &CheckContext<'_>) -> Vec<LimitClamp> {
    if !draft.purpose.is_buy() || evaluate(draft, ctx).is_err() {
        return Vec::new();
    }
    let ([risk, symbol, sector], ai) = buy_size_stages(draft, ctx);
    let Some(mut current) = ai else {
        return Vec::new();
    };
    let mut clamps = Vec::new();
    for (limit, cap) in [
        (LimitKind::RiskPerTrade, risk),
        (LimitKind::SymbolWeight, symbol),
        (LimitKind::SectorWeight, sector),
    ] {
        if cap < current {
            clamps.push(LimitClamp {
                limit,
                before: current,
                after: cap,
            });
            current = cap;
        }
    }
    clamps
}

/// [`reduce_within_limit`]과 같되, 줄인 기록을 함께 낸다. 기록은 [`limit_clamps`]의 줄들 뒤에
/// 자동 주문 금액 한도가 줄였으면 [`LimitKind::AutoOrderLimit`] 한 줄(앞 = `min(다시 셈한 수량,
/// 기다리던 수량)`, 뒤 = 줄인 수량)이 이어진다. 막힘은 [`reduce_within_limit`]과 같다.
pub fn reduce_within_limit_recorded(
    pending: PendingApproval,
    draft: &OrderDraft,
    ctx: &CheckContext<'_>,
) -> Result<(Passed, Vec<LimitClamp>), Blocked> {
    let waiting = pending.order().quantity;
    let passed = reduce_within_limit(pending, draft, ctx)?;
    let mut clamps = limit_clamps(draft, ctx);
    // 점검이 다시 셈한 수량(막히지 않았으므로 셈이 나온다)과 기다리던 수량 가운데 작은 쪽이 앞이다.
    let rechecked = evaluate(draft, ctx).map_or(waiting, |done| done.order.quantity);
    let before = rechecked.min(waiting);
    let after = passed.order().quantity;
    if after < before {
        clamps.push(LimitClamp {
            limit: LimitKind::AutoOrderLimit,
            before,
            after,
        });
    }
    Ok((passed, clamps))
}
