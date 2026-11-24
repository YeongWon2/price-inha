//! 점검 통과 표시 (E33).
//!
//! 주문 실행기는 규칙 엔진이 낸 표시가 있는 주문만 받는다. 표시를 만드는 길은 하나뿐이다:
//! 계좌를 열 때([`crate::PaperBroker::open`] · [`crate::PaperBroker::restore`]) 함께 나오는
//! [`CheckAuthority`]. 조립하는 쪽이 이것을 규칙 엔진(T6)에만 건넨다. 다른 곳에서는
//! [`CheckPermit`]을 만들 수 없고(필드가 비공개, 생성자 없음), 복제할 수도 없다.
//!
//! 표시는 매매 번호에 묶을 수 있다([`CheckAuthority::approve_for_trade`]). 모의계좌는 매매를 모르므로
//! 묶인 표시도 같이 받는다(계좌 표식 · 내용 대조만). 그 매매의 표시인지는 주문을 내는 쪽이 본다.

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
            trade: None,
        }
    }

    /// 매매 하나(`trade`, 저장소의 매매 번호)에 묶인 표시를 낸다. 주문을 내는 쪽은
    /// [`CheckPermit::trade`]가 그 매매의 번호인지 대조한다(같은 종목 · 방향의 다른 매매에 쓰지 못하게).
    pub fn approve_for_trade(&self, trade: i64, approved: ApprovedOrder) -> CheckPermit {
        CheckPermit {
            seal: Arc::clone(&self.seal),
            approved,
            trade: Some(trade),
        }
    }

    /// 승인 증표로 지난 주문용 표시를 낸다(`granted() == true`). 늘 매매 하나(`trade`)에 묶인다.
    /// 뼈대: 몸통은 아직 없다.
    pub fn approve_granted_for_trade(&self, trade: i64, approved: ApprovedOrder) -> CheckPermit {
        // 뼈대: 아직 승인 표시를 달지 않는다(지금 `approve_for_trade`와 같다).
        self.approve_for_trade(trade, approved)
    }
}

/// 점검 통과 표시. [`CheckAuthority::approve`]로만 만들 수 있다.
#[derive(Debug)]
pub struct CheckPermit {
    /// 낸 권한의 표식.
    seal: Arc<()>,
    /// 승인한 내용.
    approved: ApprovedOrder,
    /// 묶인 매매 번호. [`CheckAuthority::approve`]로 낸 표시는 `None`.
    trade: Option<i64>,
}

impl CheckPermit {
    /// 승인한 내용.
    pub fn approved(&self) -> &ApprovedOrder {
        &self.approved
    }

    /// 이 표시가 묶인 매매 번호([`CheckAuthority::approve_for_trade`]). 묶이지 않았으면 `None`.
    pub fn trade(&self) -> Option<i64> {
        self.trade
    }

    /// 승인 증표로 낸 표시인가([`CheckAuthority::approve_granted_for_trade`]). 뼈대: 몸통은 아직 없다.
    pub fn granted(&self) -> bool {
        // 뼈대: 아직 표시를 담지 않는다.
        false
    }

    /// 이 표시를 낸 권한이 그 표식(계좌)의 짝인가.
    pub(crate) fn sealed_by(&self, seal: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.seal, seal)
    }
}
