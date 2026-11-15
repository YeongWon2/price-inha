//! 주문 전 점검(C9 · FN1)과 점검 통과 표시(E33).

use pi_paper::{CheckAuthority, CheckPermit};

use crate::{CheckContext, OrderDraft, Outcome, Passed};

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
pub fn check(draft: &OrderDraft, ctx: &CheckContext<'_>) -> Outcome {
    let _ = (draft, ctx);
    todo!()
}

/// 점검을 지난 주문에 점검 통과 표시를 낸다(E33). 표시의 승인 내용은 `passed.order()`와 같다.
/// 승인을 기다리는 주문([`crate::PendingApproval`])은 여기로 올 수 없다(타입이 다르다).
pub fn issue(authority: &CheckAuthority, passed: Passed) -> CheckPermit {
    let _ = (authority, passed);
    todo!()
}
