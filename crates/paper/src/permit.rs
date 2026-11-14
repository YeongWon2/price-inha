//! 점검 통과 표시 (E33).
//!
//! 주문 실행기는 규칙 엔진이 낸 표시가 있는 주문만 받는다. 표시를 만드는 길은 하나뿐이다:
//! 계좌를 열 때([`crate::PaperBroker::open`] · [`crate::PaperBroker::restore`]) 함께 나오는
//! [`CheckAuthority`]. 조립하는 쪽이 이것을 규칙 엔진(T6)에만 건넨다. 다른 곳에서는
//! [`CheckPermit`]을 만들 수 없고(필드가 비공개, 생성자 없음), 복제할 수도 없다.

use std::sync::Arc;

use pi_market::{Market, Symbol};
use rust_decimal::Decimal;

use crate::{AccountKind, OrderType, Side};

/// 규칙 엔진이 승인한 주문 내용. 주문 요청의 같은 칸들과 **모두 같아야** 받는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedOrder {
    /// 시장.
    pub market: Market,
    /// 종목.
    pub symbol: Symbol,
    /// 방향.
    pub side: Side,
    /// 호가 유형.
    pub order_type: OrderType,
    /// 수량.
    pub quantity: Decimal,
    /// 값. 시장가면 `None`.
    pub price: Option<Decimal>,
    /// 계좌 종류.
    pub account_kind: AccountKind,
}

/// 점검 통과 표시를 내는 권한. 계좌 하나에 하나만 있고 복제할 수 없다.
#[derive(Debug)]
pub struct CheckAuthority {
    /// 짝인 계좌와 같은 것을 가리킨다. 표시가 이 계좌의 것인지 이것으로 가린다.
    seal: Arc<()>,
}

impl CheckAuthority {
    /// 이 크레이트 안에서만 만든다(계좌를 열 때).
    pub(crate) fn new(seal: Arc<()>) -> Self {
        Self { seal }
    }

    /// 승인한 내용으로 표시 하나를 낸다. 표시 하나는 주문 하나에만 쓸 수 있다(값이 옮겨 간다).
    pub fn approve(&self, approved: ApprovedOrder) -> CheckPermit {
        CheckPermit {
            seal: Arc::clone(&self.seal),
            approved,
        }
    }
}

/// 점검 통과 표시. [`CheckAuthority::approve`]로만 만들 수 있다.
#[derive(Debug)]
pub struct CheckPermit {
    /// 낸 권한의 표식.
    seal: Arc<()>,
    /// 승인한 내용.
    approved: ApprovedOrder,
}

impl CheckPermit {
    /// 승인한 내용.
    pub fn approved(&self) -> &ApprovedOrder {
        &self.approved
    }

    /// 이 표시를 낸 권한이 그 표식(계좌)의 짝인가.
    pub(crate) fn sealed_by(&self, seal: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.seal, seal)
    }
}
