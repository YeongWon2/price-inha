//! pi-paper: docs/dev/modules/paper.md 를 따른다.
//!
//! 구성 요소 A8 주문 실행기 가운데 **모의계좌와 체결**. 토스증권 Open API에는 모의투자가
//! 없어서, 실제 돈이 움직이는 것과 같은 흐름으로 도는 모의계좌를 직접 만든다.
//!
//! - 주문을 내는 쪽은 [`Broker`] trait 하나만 안다. [`PaperBroker`]가 그 구현이다.
//! - **실제 증권사로 주문을 보내는 코드는 이 크레이트에 없다.** 인터넷 · 파일 · DB · 시계를
//!   직접 쓰지 않는다. 시각 · 시세 · 달력은 인자로 받고, 저장은 [`Snapshot`] 값을 내보내
//!   조립하는 쪽에 맡긴다. 같은 입력은 같은 결과를 낸다.
//! - 돈은 십진수([`rust_decimal::Decimal`])로만 셈한다.

mod accept;
mod book;
mod broker;
mod config;
mod error;
mod fill;
mod ledger;
mod model;
mod money;
mod orders;
mod paper;
mod permit;

pub use broker::Broker;
pub use config::{
    DEFAULT_EVENT_BUFFER, InitialCash, LimitFillPrice, MarketRules, MaxFill, MoneyRounding,
    PaperConfig, ProceedsUsable, RoundingMode, SessionRules, Settlement, TickBand, UnverifiedRules,
};
pub use error::{BrokerError, InvalidReason, SnapshotReason};
pub use model::{
    AccountKind, Archived, CashBalance, CloseReason, Commission, Execution, Fill, Holding,
    ModifyRequest, Order, OrderAck, OrderEvent, OrderEventKind, OrderGroup, OrderId, OrderRequest,
    OrderStatus, OrderType, Side, UnfilledTimeout,
};
pub use paper::{Fault, IDEMPOTENCY_WINDOW, PaperBroker, Snapshot};
pub use permit::{ApprovedOrder, CheckAuthority, CheckPermit};
