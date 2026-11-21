//! 승인된 주문을 점검 통과로 바꾸는 길(E21 · V25 · 결정 7번).
//!
//! 승인을 기다리던 주문([`PendingApproval`])은 승인이 소비된 뒤에만 [`approved`]로 다시 점검을
//! 거쳐 [`Passed`]가 된다. 승인 소비(한 번만 · 지문 비교)는 approval 모듈이 하고, 여기서는
//! 그 결과를 증표로 받아 다시 점검한다(이 모듈은 approval에 기대지 않는다).

use pi_paper::{ApprovedOrder, Side};
use rust_decimal::Decimal;

use crate::check::evaluate;
use crate::sizing::floor_within;
use crate::{
    BlockKind, Blocked, CheckContext, OrderDraft, Passed, PendingApproval, Reason, Violation,
};

/// 승인 소비 결과의 증표: 승인 번호 글자와 승인한 내용의 지문 글자.
/// approval 모듈이 `consume`에 성공한 뒤 만든다. 글자는 비어 있으면 안 된다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalGrant {
    id: String,
    fingerprint: String,
}

impl ApprovalGrant {
    /// 승인 번호와 지문으로 증표를 만든다. 비었는지는 [`approved`]가 본다.
    pub fn new(id: impl Into<String>, fingerprint: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            fingerprint: fingerprint.into(),
        }
    }

    /// 승인 번호.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 승인한 내용의 지문.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

/// 승인된 주문을 다시 점검해 점검 통과로 바꾼다.
///
/// - 증표의 번호나 지문이 비었거나 공백뿐이면 막힘([`crate::BlockKind::ApprovalRejected`],
///   [`crate::Reason::ApprovalMismatch`]).
/// - `draft`를 지금의 `ctx`로 [`crate::check`]와 같은 순서로 다시 점검한다. 막히면 그 막힘을
///   그대로 낸다(판단 · 팀 상태 · 원칙 · 주문 직전 점검).
/// - 다시 셈한 주문 내용(종목 · 방향 · 호가 유형 · 수량 · 값)이 `pending.order()`와 다르면
///   승인이 덮지 못한 것이라 막힘([`crate::BlockKind::ApprovalRejected`],
///   [`crate::Reason::ApprovalMismatch`]).
/// - 시장가 사기는 승인으로 지날 수 없다(결정 7번, [`crate::Violation::MarketBuyOverAutoLimit`]).
/// - 다 지나면 금액 한도를 넘어도 [`Passed`]이고, [`Passed::approval`]에 증표가 남는다.
pub fn approved(
    pending: PendingApproval,
    grant: &ApprovalGrant,
    draft: &OrderDraft,
    ctx: &CheckContext<'_>,
) -> Result<Passed, Blocked> {
    let mismatch = || Blocked {
        kind: BlockKind::ApprovalRejected,
        reason: Reason::ApprovalMismatch,
    };
    if grant.id.trim().is_empty() || grant.fingerprint.trim().is_empty() {
        return Err(mismatch());
    }
    let done = evaluate(draft, ctx)?;
    if &done.order != pending.order() {
        return Err(mismatch());
    }
    Ok(Passed::new(done.order, done.amount).with_approval(grant.clone()))
}

/// 답 없는 사기를 지금의 금액 한도 안으로 줄여 다시 점검한다(FN2 3b3 · E22, 4차).
///
/// approval의 `DueAction::ReduceWithinLimit`를 받은 server가 부른다. 답이 없어 승인이 없으므로
/// 승인 증표를 받지 않는다.
///
/// - `draft`를 지금의 `ctx`로 [`crate::check`]와 같은 순서로 다시 점검한다(판단 · 팀 상태 · 근거 ·
///   원칙 · 주문 직전 점검). 막히면 그 막힘 그대로 낸다(시장가 사기는
///   [`crate::Violation::MarketBuyOverAutoLimit`] 그대로).
/// - 다시 셈한 주문의 종목 · 방향 · 호가 유형이 `pending.order()`와 다르거나 사는 주문이 아니면
///   [`crate::Reason::ApprovalMismatch`].
/// - 수량 = `min(다시 셈한 수량, pending의 수량, floor(평가액 × 자동 주문 금액 한도% ÷ 지정가))`.
///   지정가는 다시 셈한 값이다. 0이면 [`crate::Violation::NothingWithinLimit`].
/// - 줄인 금액은 한도 이하라 승인 없이 [`Passed`]이고 [`Passed::approval`]은 `None`이다.
pub fn reduce_within_limit(
    pending: PendingApproval,
    draft: &OrderDraft,
    ctx: &CheckContext<'_>,
) -> Result<Passed, Blocked> {
    let done = evaluate(draft, ctx)?;
    let waiting = pending.order();
    let same = done.order.side == Side::Buy
        && waiting.side == Side::Buy
        && done.order.market == waiting.market
        && done.order.symbol == waiting.symbol
        && done.order.order_type == waiting.order_type;
    if !same {
        return Err(Blocked {
            kind: BlockKind::ApprovalRejected,
            reason: Reason::ApprovalMismatch,
        });
    }
    // 한 주의 값: 지정가는 다시 셈한 값, 시장가(한도 안이라 점검을 지난 것)는 셈에 쓴 진입가.
    let unit = match done.order.price {
        Some(price) => price,
        None if done.order.quantity > Decimal::ZERO => done.amount / done.order.quantity,
        None => Decimal::ZERO,
    };
    let limit = done.limit.unwrap_or(Decimal::ZERO);
    let quantity = done
        .order
        .quantity
        .min(waiting.quantity)
        .min(floor_within(limit, unit));
    if quantity <= Decimal::ZERO {
        return Err(Blocked {
            kind: BlockKind::PrincipleOrRationale,
            reason: Reason::Violations(vec![Violation::NothingWithinLimit]),
        });
    }
    let order = ApprovedOrder {
        quantity,
        ..done.order
    };
    Ok(Passed::new(order, quantity * unit))
}
