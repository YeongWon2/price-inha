//! price-inha 서버: docs/dev/modules/gate.md 의 "price-inha(실행 파일)가 만드는 것"을 따른다.
//!
//! 조립 로직은 여기에 있고 `main.rs`는 얇다: 설정을 읽고([`Settings::from_process`]),
//! 띄우고([`App::start`]), Ctrl+C를 기다렸다가 끈다([`App::shutdown`]).

mod ai_link;
mod app;
mod demo;
mod desk;
mod error;
mod feed;
mod keys;
mod log;
mod page;
mod retry;
mod settings;
mod setup;
mod setup_http;
mod trading;

pub use ai_link::{
    AI_CHOICE_FILE, AiChoice, AiMethod, AiStatus, CLI_CHECKING, CLI_COMMON_LOCATIONS,
    CLI_CONCURRENCY, CLI_ENV_ALLOWLIST, CLI_LOGGED_IN, CLI_NOT_INSTALLED, CLI_NOT_LOGGED_IN,
    CLI_NOT_SUBSCRIPTION, CLI_TERMS_NOTICE, CLI_TOO_OLD, CLI_UNCHECKED, CLI_USER_LOCATION,
    CLI_WORK_DIR, CLI_WORK_NOT_EMPTY, LEGACY_AUTH_FILE, LEGACY_AUTH_NOTICE, SETUP_AI_PATH,
    cli_base_env, cli_config, cli_refusal, cli_state, find_claude, legacy_auth_file,
    load_ai_choice, prepare_cli_work_dir, save_ai_choice,
};
pub use app::{
    App, DB_FILE, FEED_BROKER_REJECTED, FEED_BROKER_UNREACHABLE, FEED_CONNECTING, FEED_DEMO,
    FEED_LIVE, FEED_NO_BROKER_KEY, FEED_REQUEST_REJECTED, FEED_SETUP_REQUIRED, KIND_FEED_STATUS,
    KIND_QUOTES, KIND_QUOTES_STALE, KIND_SYMBOLS_REJECTED, Wiring, announce_addr,
};
pub use error::{AppError, SettingsError, SetupRefusal, TradingError};
pub use keys::{AiKeys, BrokerKeys, KeyVerifier, SecretsRedactor, Upstreams};
pub use log::{RedactedLine, RedactingWriter};
pub use settings::{
    DEFAULT_CALENDAR_RETRY, DEFAULT_QUOTE_INTERVAL, DEFAULT_REGISTER_ATTEMPTS,
    DEFAULT_REGISTER_WINDOW, DEFAULT_RETRY_INITIAL, DEFAULT_RETRY_MAX, DEFAULT_SYMBOLS_ATTEMPTS,
    DEFAULT_SYMBOLS_WINDOW, ENV_BIND, ENV_CALENDAR_RETRY_MS, ENV_HOME, ENV_MARKET, ENV_PORT,
    ENV_QUOTE_INTERVAL_MS, ENV_REGISTER_ATTEMPTS, ENV_REGISTER_WINDOW_MS, ENV_RETRY_INITIAL_MS,
    ENV_RETRY_MAX_MS, ENV_SYMBOLS, ENV_SYMBOLS_ATTEMPTS, ENV_SYMBOLS_WINDOW_MS, Settings,
};
pub use setup::{
    AttemptLimit, ITEM_AI, ITEM_BROKER, ITEM_DATA, ITEM_SYMBOLS, KEY_AI_API_KEY, KEY_BROKER,
    KEY_DATA, KIND_SETUP, KeyRequest, MAX_WATCH_SYMBOLS, SETUP_ITEMS, SETUP_KEYS_PATH, SETUP_PATH,
    SETUP_REQUIRED, SETUP_STARTED, SETUP_SYMBOLS_PATH, SetupItem, SetupItemSpec, SetupStatus,
    SymbolsSource, WATCHLIST_FILE, load_watchlist, parse_symbols, save_watchlist,
};
pub use trading::{
    ANSWER_CONTENT_CHANGED, ANSWER_FINGERPRINT_MISMATCH, ANSWER_NOT_FOUND, APPROVAL_ANSWER_PATH,
    CalendarFn, DayPlan, KIND_DAY_CLOSE, KIND_DAY_PLAN, KIND_DAY_START, OrderRequest, Placed,
    RESTART_KEYS_REQUIRED, RESTART_NOT_STOPPED, TEAM_ANOMALY_HALT, TEAM_DAILY_LIMIT_HALT,
    TEAM_FULL_STOP, TEAM_OPERATING, TEAM_PATH, TEAM_RESTART_PATH, TEAM_RESTING,
    TEMPLATE_ADJUST_PATH, TEMPLATE_OVER_CAP, TEMPLATE_PATH, Trading, TradingWiring,
};
