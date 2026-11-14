//! price-inha 서버: docs/dev/modules/gate.md 의 "price-inha(실행 파일)가 만드는 것"을 따른다.
//!
//! 조립 로직은 여기에 있고 `main.rs`는 얇다: 설정을 읽고([`Settings::from_process`]),
//! 띄우고([`App::start`]), Ctrl+C를 기다렸다가 끈다([`App::shutdown`]).

mod app;
mod demo;
mod error;
mod feed;
mod keys;
mod page;
mod settings;

pub use app::{
    App, DB_FILE, FEED_BROKER_REJECTED, FEED_BROKER_UNREACHABLE, FEED_CONNECTING, FEED_DEMO,
    FEED_LIVE, FEED_NO_BROKER_KEY, KIND_FEED_STATUS, KIND_QUOTES, KIND_QUOTES_STALE, Wiring,
    announce_addr,
};
pub use error::{AppError, SettingsError};
pub use keys::{AiKeys, BrokerKeys, KeyVerifier, SecretsRedactor, Upstreams};
pub use settings::{
    DEFAULT_QUOTE_INTERVAL, DEFAULT_RETRY_INITIAL, DEFAULT_RETRY_MAX, ENV_BIND, ENV_HOME,
    ENV_MARKET, ENV_PORT, ENV_QUOTE_INTERVAL_MS, ENV_RETRY_INITIAL_MS, ENV_RETRY_MAX_MS,
    ENV_SYMBOLS, Settings,
};
