//! 토스증권 어댑터. `docs/dev/toss-api-notes.md`와 공식 명세(OpenAPI v1.2.28)를 따른다.
//!
//! 쓰는 것은 셋뿐이다: `POST /oauth2/token`, `GET /api/v1/prices`,
//! `GET /api/v1/market-calendar/{KR|US}`. 주문 · 계좌 · 보유는 부르지 않는다.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::NaiveDate;

use crate::{
    Clock, CredentialSource, Credentials, Market, MarketAdapter, MarketCalendar, MarketError,
    Quote, Symbol,
};

/// 토스증권 Open API 기본 주소.
pub const DEFAULT_BASE_URL: &str = "https://openapi.tossinvest.com";
/// 요청 하나의 시간 제한 기본값.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// 토큰 만료 이만큼 전부터 미리 다시 발급한다(기본값).
pub const DEFAULT_TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(60);
/// 429를 받았을 때 다시 시도하는 횟수 기본값.
pub const DEFAULT_RATE_LIMIT_RETRIES: u32 = 2;
/// 현재가 요청 하나에 담는 종목 수의 한도(공식 명세).
pub const MAX_SYMBOLS_PER_REQUEST: usize = 200;

/// 토스증권 어댑터 설정.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TossConfig {
    /// 기본 주소(끝에 `/` 없음). 테스트는 가짜 서버 주소를 넣는다.
    pub base_url: String,
    /// 요청 하나의 시간 제한. 넘으면 [`MarketError::Connection`].
    pub timeout: Duration,
    /// 토큰의 남은 시간이 이것보다 적으면 미리 다시 발급한다.
    pub token_refresh_margin: Duration,
    /// 429를 받았을 때 `Retry-After`만큼 기다렸다가 다시 시도하는 횟수.
    /// 2면 요청은 많아야 3번 나간다.
    pub rate_limit_retries: u32,
}

impl Default for TossConfig {
    /// [`DEFAULT_BASE_URL`] · [`DEFAULT_TIMEOUT`] · [`DEFAULT_TOKEN_REFRESH_MARGIN`] ·
    /// [`DEFAULT_RATE_LIMIT_RETRIES`].
    fn default() -> Self {
        todo!()
    }
}

/// 토스증권 연결. 토큰 하나를 쥐고 여러 어댑터가 함께 쓴다.
///
/// 토스증권은 client마다 유효한 토큰이 1개뿐이고 다시 발급하면 이전 토큰이 무효가
/// 된다. 그래서 국내 · 미국 어댑터는 같은 `TossClient`에서 만들어 토큰을 함께 쓴다.
/// `clone`한 것도 같은 토큰을 쓴다.
///
/// 토큰 규칙:
/// - 받은 토큰을 만료 전까지 다시 쓴다. 필요할 때만 다시 발급한다.
/// - 남은 시간이 `token_refresh_margin`보다 적으면 미리 다시 발급한다.
/// - `expired-token`이나 `token-revoked`(401)를 받으면 한 번만 다시 발급해 다시 시도한다.
/// - 동시에 여러 요청이 와도 토큰 발급은 한 번만 나간다.
/// - 자격이 없으면([`CredentialSource::credentials`]가 `None`) 요청을 보내지 않고
///   [`MarketError::NoCredentials`]를 돌려준다(V31).
#[derive(Clone)]
pub struct TossClient {
    _private: (),
}

impl TossClient {
    /// 연결을 만든다. 이때는 바깥으로 요청을 보내지 않는다.
    pub fn new(
        config: TossConfig,
        credentials: Arc<dyn CredentialSource>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        todo!()
    }

    /// 그 시장의 어댑터. 토큰은 이 연결의 것을 함께 쓴다.
    pub fn adapter(&self, market: Market) -> TossAdapter {
        todo!()
    }
}

/// 토스증권의 시장 하나(국내 또는 미국).
pub struct TossAdapter {
    _private: (),
}

#[async_trait]
impl MarketAdapter for TossAdapter {
    fn market(&self) -> Market {
        todo!()
    }

    /// `GET /api/v1/prices?symbols=a,b,c`. 200개가 넘으면 200개씩 나눠 부르고 순서대로 잇는다.
    /// `lastPrice` 문자열을 십진수로 읽는다. 받은 시각은 [`Clock::now`].
    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
        todo!()
    }

    /// `GET /api/v1/market-calendar/KR?date=YYYY-MM-DD`(국내) 또는 `/US`(미국).
    /// null인 세션은 뺀다. 국내 `integrated`가 null이면 그날 세션이 없다.
    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError> {
        todo!()
    }
}

/// 이 자격으로 토큰이 발급되는지 확인한다(토큰 발급 한 번).
///
/// 주의: 토스증권은 다시 발급하면 이전 토큰이 무효가 된다. 돌고 있는 [`TossClient`]가
/// 같은 자격을 쓰고 있었다면 그쪽은 `token-revoked`를 받고 한 번 다시 발급한다.
pub async fn verify(config: &TossConfig, credentials: &Credentials) -> Result<(), MarketError> {
    todo!()
}
