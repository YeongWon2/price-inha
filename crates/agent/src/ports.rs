//! 실행기가 받는 포트: 도구 · 시계. 가리기는 `pi_ai::Redactor`를 그대로 쓴다.

use async_trait::async_trait;
use pi_store::{TradeId, UnixMillis};
use rust_decimal::Decimal;

use crate::action::ActionCall;
use crate::error::ToolError;
use crate::role::Role;

/// 시계. 실행기는 지금 시각을 이것으로만 읽는다(저장할 시각 · 판단 시각).
pub trait Clock: Send + Sync {
    /// 지금 시각.
    fn now(&self) -> UnixMillis;
}

/// 주문 방향.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// 사기.
    Buy,
    /// 팔기.
    Sell,
}

/// 매매 담당이 만든 주문 안(V4).
///
/// 수량 · 가격은 규칙 엔진이 정한다(P7). 매매 담당이 적은 수량 · 가격은 정해진 값이 아니라
/// 참고값(`ai_quantity` · `ai_price`)으로만 넘긴다. 규칙 엔진이 자기 값과 견주어 작은 쪽을 고른다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderDraft {
    /// 매매 번호. 판단됨인 매매다(E20).
    pub trade: TradeId,
    /// 종목 코드(매매의 것 그대로).
    pub symbol: String,
    /// 방향.
    pub side: Side,
    /// 매매 담당이 적은 설명.
    pub note: String,
    /// 매매 담당이 적은 수량(참고값). 안 적었거나 쓸 수 없는 값(0 · 음수 · 숫자 아님)이면 `None`.
    pub ai_quantity: Option<Decimal>,
    /// 매매 담당이 적은 가격(참고값). 안 적었거나 쓸 수 없는 값이면 `None`.
    pub ai_price: Option<Decimal>,
}

/// 도구 포트(자료 · 지식 · 주문 안 · 승인). 주문을 내거나 점검 · 승인하는 일은 포트 너머에 있다.
#[async_trait]
pub trait Tools: Send + Sync {
    /// 그 담당이 부를 수 있는 도구 이름들. 여기에 없는 이름은 "모르는 도구"다.
    fn names(&self, role: Role) -> Vec<String>;

    /// 도구 하나를 부른다. 돌려준 글은 도구 결과로 담당에게 간다.
    /// 오류의 글도 도구 결과로 간다(턴은 이어진다).
    async fn call(&self, role: Role, call: &ActionCall) -> Result<String, ToolError>;

    /// 주문 안을 넘긴다(수량 · 가격 계산과 점검은 넘겨받은 쪽이 한다).
    async fn submit_order_draft(&self, draft: &OrderDraft) -> Result<(), ToolError>;
}
