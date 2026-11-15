//! 승인 대상과 내용 지문.
//!
//! 원본: OpenClaw `system-run-approval-binding` — 승인할 때 묶은 내용을 실행 직전에 다시 비교한다
//! (`docs/research/triggers-approval-memory-deep.md` 3.2 · 3.4). 우리 지문은 주문 내용의 SHA-256이다.

use pi_store::{AccountKind, ProposalId, TemplateId, UnixMillis};
use rust_decimal::Decimal;

/// 주문 방향.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// 사기 (`"buy"`)
    Buy,
    /// 팔기 (`"sell"`)
    Sell,
}

/// 주문 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderType {
    /// 지정가 (`"limit"`). 승인이 필요한 주문은 이것만 된다(결정 7번).
    Limit,
    /// 시장가 (`"market"`). 승인 요청을 만들 수 없다([`crate::Error::LimitOrderRequired`]).
    Market,
}

/// 주문을 부른 신호의 종류. 기다릴 시간(N2)과 답이 없을 때의 처리(E22)를 정한다.
///
/// 사는 주문은 [`SignalKind::Buy`]만, 파는 주문은 [`SignalKind::StopLoss`] · [`SignalKind::TakeProfit`]만
/// 짝이 맞는다. 짝이 안 맞으면 요청은 [`crate::Error::InvalidSubject`]다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalKind {
    /// 손절 신호(파는 쪽).
    StopLoss,
    /// 익절 신호(파는 쪽).
    TakeProfit,
    /// 사는 신호.
    Buy,
}

/// 승인을 받을 주문.
///
/// 지문에 들어가는 칸: 계좌 종류 · 종목 · 방향 · 수량 · 가격 · 주문 종류 · 신호 id.
/// `signal_kind` · `signal_valid_until`은 지문에 넣지 않는다(기다릴 시간과 만료 시각에만 쓴다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderSubject {
    /// 계좌 종류.
    pub account: AccountKind,
    /// 종목 코드.
    pub symbol: String,
    /// 방향.
    pub side: Side,
    /// 수량(주).
    pub quantity: u64,
    /// 지정가. 지문에 넣는다(결정 7번).
    pub price: Decimal,
    /// 주문 종류.
    pub order_type: OrderType,
    /// 신호 id.
    pub signal_id: String,
    /// 신호 종류.
    pub signal_kind: SignalKind,
    /// 신호 유효 시간의 끝. 이 시각부터 승인 요청은 만료다(답 · 소비 모두 효력이 없다).
    pub signal_valid_until: UnixMillis,
}

/// 승인 대상.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// 주문.
    Order(OrderSubject),
    /// 템플릿의 실전 승격.
    LivePromotion {
        /// 승격할 템플릿.
        template: TemplateId,
    },
    /// 규칙 개선안 적용.
    RuleImprovement {
        /// 적용할 개선안.
        proposal: ProposalId,
    },
    /// 전체 정지 뒤 다시 시작.
    Restart,
}

/// 내용 지문: [`canonical_json`]의 UTF-8 바이트를 SHA-256 한 값의 소문자 16진수 64자.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Fingerprint(String);

impl Fingerprint {
    /// 16진수 64자 글자.
    pub fn as_str(&self) -> &str {
        todo!()
    }

    /// 카드에 보이는 짧은 표시: 지문 앞 6자(결정 4번). 사용자가 치지는 않는다.
    pub fn short_code(&self) -> &str {
        todo!()
    }
}

/// 칸 순서를 정한 정규 JSON. 빈칸 · 줄바꿈이 없고, 글자는 JSON 규칙대로 이스케이프한다.
///
/// - 주문: `{"kind":"order","account":"paper"|"live","symbol":…,"side":"buy"|"sell","quantity":수,`
///   `"price":"가격","order_type":"limit"|"market","signal_id":…}`.
///   가격은 [`Decimal::normalize`]한 글자다(`71000.00`과 `71000`은 같은 `"71000"`).
/// - 실전 승격: `{"kind":"live_promotion","template":번호}`
/// - 규칙 개선: `{"kind":"rule_improvement","proposal":번호}`
/// - 다시 시작: `{"kind":"restart"}`
///
/// 대상 종류와 대상 번호가 들어가므로 한 대상의 승인은 다른 대상에 쓸 수 없다.
pub fn canonical_json(subject: &Subject) -> String {
    let _ = subject;
    todo!()
}

/// 대상의 내용 지문.
pub fn fingerprint(subject: &Subject) -> Fingerprint {
    let _ = subject;
    todo!()
}
