//! 주문 전 점검(C9 · FN1)과 점검 통과 표시(E33).

use pi_paper::{AccountKind, ApprovedOrder, CheckAuthority, CheckPermit};
use rust_decimal::Decimal;

use crate::screen::{common_violations, missing_fields, plan_buy, plan_sell};
use crate::sizing::percent_of;
use crate::{
    BlockKind, Blocked, CheckContext, OrderDraft, Outcome, Passed, PendingApproval, Reason,
    TeamStatus, TradeStage, Violation,
};

/// 주문 안을 점검한다(C9). 순서는 FN1을 따르고, 앞에서 막히면 뒤는 보지 않는다.
///
/// 1. 판단 상태: [`crate::TradeStage::Judged`]가 아니면 막힘(E20, [`crate::Reason::NotJudged`]).
/// 2. 팀 상태: [`crate::TeamStatus::Operating`]이 아니면 사는 주문을 막는다(E31,
///    [`crate::Reason::TeamNotOperating`]). 손절 · 청산은 어떤 멈춤에서도 지나간다(E34).
/// 3. 근거(P9 · E10): 빠진 칸이 있으면 [`crate::Reason::MissingRationale`].
/// 4. 원칙 P1~P13과 주문 직전 점검 P12(E11 · E18): 어긴 것을 모두 모아
///    [`crate::Reason::Violations`]. 사는 주문에만 보는 것: P1 · P2(손절 < 진입) · P3 · P4 ·
///    P5(상품 · 돈 · 금지 종목) · P6 · P8 · P11 · P13, 일일 매매 횟수 · VI · 거래정지.
///    모든 주문에 보는 것: P10, 장 시간 · 세션의 호가 유형 · 가격제한(AI 지정가) · 중복 ·
///    1회 최대 금액 · API 요청 한도. 파는 주문은 가진 수량을 넘을 수 없다(P5).
/// 5. 수량과 값(P7): 사기는 [`crate::size`]의 수량, 지정가는 `min(AI 값, 시세)`를 호가 단위로
///    내린 값. 팔기는 `min(AI 수량, 가진 수량)`(AI 수량이 없으면 가진 수량), 지정가는 AI 값을
///    호가 단위로 내린 값.
/// 6. 사는 주문의 금액이 `평가액 × 자동 주문 금액 한도%`를 **넘으면** [`Outcome::NeedsApproval`]
///    (E21 · V25). 같으면 지나간다. 파는 주문은 승인을 기다리지 않는다(위험을 줄이는 행동, P18).
/// 7. 다 지나면 [`Outcome::Pass`]. 계좌 종류는 언제나 모의(`AccountKind::Paper`)다.
///
/// 2차에 더한 것:
/// - 시세의 종목이 주문 안의 종목과 다르면 사고팔기 모두 [`Violation::QuoteSymbolMismatch`].
/// - 늦은 시세 기준은 받은 설정을 30초로 다시 막는다(`min(ctx.stale.max_age, 30초)`, P11).
/// - P6 손실 중 판단은 `min(장부 마지막 값, 시세)`가 평균 단가보다 낮은가로 본다.
/// - 중복 점검에서 손절 · 청산은 열린 매도(익절)에 막히지 않는다(열린 주문 취소는 exec가 한다).
/// - 시장가 사기가 금액 한도를 넘으면 승인을 기다리지 않고 [`Violation::MarketBuyOverAutoLimit`]로
///   막는다(결정 7번: 승인이 필요한 사기는 지정가만).
///
/// 테스트가 정하지 않아 막는 쪽으로 정한 것은 [`crate::screen`] 모듈 주석에 적었다. 더해서:
/// - 근거(P9)는 파는 주문에도 본다.
/// - 다른 위반 없이 셈한 수량이 0이면(AI 수량이 1 미만 등) [`Violation::RiskTooLarge`]로 막는다.
/// - 자동 주문 금액 한도의 셈이 넘치면 승인을 기다리게 한다.
pub fn check(draft: &OrderDraft, ctx: &CheckContext<'_>) -> Outcome {
    if draft.stage != TradeStage::Judged {
        return blocked(Reason::NotJudged(draft.stage));
    }
    let buy = draft.purpose.is_buy();
    if buy && ctx.team != TeamStatus::Operating {
        return blocked(Reason::TeamNotOperating(ctx.team));
    }
    let missing = missing_fields(&draft.rationale);
    if !missing.is_empty() {
        return blocked(Reason::MissingRationale(missing));
    }
    let (plan, mut found) = if buy {
        plan_buy(draft, ctx)
    } else {
        plan_sell(draft, ctx)
    };
    found.extend(common_violations(draft, ctx, &plan));
    if found.is_empty() && plan.quantity <= Decimal::ZERO {
        found.push(Violation::RiskTooLarge);
    }
    if !found.is_empty() {
        return blocked(Reason::Violations(unique(found)));
    }
    let order = ApprovedOrder {
        market: draft.market,
        symbol: draft.symbol.clone(),
        side: draft.purpose.side(),
        order_type: draft.order_type,
        quantity: plan.quantity,
        price: plan.price,
        account_kind: AccountKind::Paper,
    };
    if buy {
        let limit = percent_of(ctx.book.equity, ctx.params.auto_order_limit_pct);
        match limit {
            Some(limit) if plan.amount <= limit => {}
            Some(limit) => {
                return Outcome::NeedsApproval(PendingApproval::new(order, plan.amount, limit));
            }
            None => {
                return Outcome::NeedsApproval(PendingApproval::new(
                    order,
                    plan.amount,
                    Decimal::ZERO,
                ));
            }
        }
    }
    Outcome::Pass(Passed::new(order, plan.amount))
}

/// 규칙 엔진의 막힘(언제나 원칙 · 근거 부족).
fn blocked(reason: Reason) -> Outcome {
    Outcome::Blocked(Blocked {
        kind: BlockKind::PrincipleOrRationale,
        reason,
    })
}

/// 처음 나온 순서를 지키며 겹친 위반을 뺀다.
fn unique(found: Vec<Violation>) -> Vec<Violation> {
    let mut list: Vec<Violation> = Vec::with_capacity(found.len());
    for violation in found {
        if !list.contains(&violation) {
            list.push(violation);
        }
    }
    list
}

/// 점검을 지난 주문에 점검 통과 표시를 낸다(E33). 표시의 승인 내용은 `passed.order()`와 같다.
/// 승인을 기다리는 주문([`crate::PendingApproval`])은 여기로 올 수 없다(타입이 다르다).
pub fn issue(authority: &CheckAuthority, passed: Passed) -> CheckPermit {
    authority.approve(passed.into_order())
}
