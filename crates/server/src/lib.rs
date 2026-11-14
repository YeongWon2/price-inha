//! price-inha 서버: docs/dev/modules/gate.md 의 "price-inha(실행 파일)가 만드는 것"을 따른다.
//!
//! 조립 로직은 여기에 있고 `main.rs`는 얇다: 설정을 읽고([`Settings::from_process`]),
//! 띄우고([`App::start`]), Ctrl+C를 기다렸다가 끈다([`App::shutdown`]).

mod app;
mod demo;
mod desk;
mod error;
mod feed;
mod keys;
mod page;
mod retry;
mod settings;
mod setup;
mod setup_http;

pub use app::{
    App, DB_FILE, FEED_BROKER_REJECTED, FEED_BROKER_UNREACHABLE, FEED_CONNECTING, FEED_DEMO,
    FEED_LIVE, FEED_NO_BROKER_KEY, FEED_REQUEST_REJECTED, FEED_SETUP_REQUIRED, KIND_FEED_STATUS,
    KIND_QUOTES, KIND_QUOTES_STALE, KIND_SYMBOLS_REJECTED, Wiring, announce_addr,
};
pub use error::{AppError, SettingsError, SetupRefusal};
pub use keys::{AiKeys, BrokerKeys, KeyVerifier, SecretsRedactor, Upstreams};
pub use settings::{
    DEFAULT_CALENDAR_RETRY, DEFAULT_QUOTE_INTERVAL, DEFAULT_REGISTER_ATTEMPTS,
    DEFAULT_REGISTER_WINDOW, DEFAULT_RETRY_INITIAL, DEFAULT_RETRY_MAX, DEFAULT_SYMBOLS_ATTEMPTS,
    DEFAULT_SYMBOLS_WINDOW, ENV_BIND, ENV_CALENDAR_RETRY_MS, ENV_HOME, ENV_MARKET, ENV_PORT,
    ENV_QUOTE_INTERVAL_MS, ENV_REGISTER_ATTEMPTS, ENV_REGISTER_WINDOW_MS, ENV_RETRY_INITIAL_MS,
    ENV_RETRY_MAX_MS, ENV_SYMBOLS, ENV_SYMBOLS_ATTEMPTS, ENV_SYMBOLS_WINDOW_MS, Settings,
};
pub use setup::{
    AttemptLimit, ITEM_AI, ITEM_BROKER, ITEM_DATA, ITEM_SYMBOLS, KEY_AI_API_KEY, KEY_AI_OAUTH,
    KEY_BROKER, KEY_DATA, KIND_SETUP, KeyRequest, MAX_WATCH_SYMBOLS, SETUP_ITEMS, SETUP_KEYS_PATH,
    SETUP_PATH, SETUP_REQUIRED, SETUP_STARTED, SETUP_SYMBOLS_PATH, SetupItem, SetupItemSpec,
    SetupStatus, SymbolsSource, WATCHLIST_FILE, load_watchlist, parse_symbols, save_watchlist,
};
