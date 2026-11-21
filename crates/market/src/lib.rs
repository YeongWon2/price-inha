//! pi-market: docs/dev/modules/market.md 를 따른다.
//!
//! 구성 요소 A9 시장 어댑터. 시세와 장 시간을 시장마다 감싼다(D5).
//! 에이전트와 규칙 엔진은 [`MarketAdapter`] trait만 안다.
//!
//! **주문 · 계좌 · 보유 API는 이 크레이트에 없다.** 토스증권에는 모의투자가 없어
//! 주문 API는 실제 돈을 움직인다. 웹소켓 실시간도 없다.
//!
//! 자격(client_id · client_secret)은 [`CredentialSource`]로 받는다. `pi-secrets`와
//! 잇는 일은 조립 단계(server)에서 한다.

mod adapter;
mod clock;
mod credentials;
mod error;
mod fake;
mod model;
mod provider;
mod stale;
mod status;
mod toss;
mod watch;

pub use adapter::MarketAdapter;
pub use clock::{Clock, SystemClock};
pub use credentials::{CredentialSource, Credentials};
pub use error::MarketError;
pub use fake::FakeAdapter;
pub use model::{
    Currency, Market, MarketCalendar, Quote, Session, SessionKind, Symbol, TradingDay,
};
pub use provider::{AssetClass, MarketRegistry, ProviderId};
pub use stale::{DEFAULT_STALE_AFTER, StalePolicy, is_stale};
pub use status::{
    StatusEstimate, StatusPolicy, StatusSource, SuspectReason, TradingStatus, estimate_status,
    resolve_status,
};
pub use toss::{
    DEFAULT_BASE_URL, DEFAULT_MAX_RESPONSE_BYTES, DEFAULT_RATE_LIMIT_RETRIES, DEFAULT_TIMEOUT,
    DEFAULT_TOKEN_FAILURE_COOLDOWN, DEFAULT_TOKEN_REFRESH_MARGIN, MAX_SYMBOLS_PER_REQUEST,
    TossAdapter, TossClient, TossConfig, verify,
};
pub use watch::{WatchEvent, watch};
