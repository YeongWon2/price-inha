//! 토스증권 어댑터. `docs/dev/toss-api-notes.md`와 공식 명세(OpenAPI v1.2.28)를 따른다.
//!
//! 쓰는 것은 셋뿐이다: `POST /oauth2/token`, `GET /api/v1/prices`,
//! `GET /api/v1/market-calendar/{KR|US}`. 주문 · 계좌 · 보유는 부르지 않는다.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use tokio::sync::Mutex;

use crate::{
    Clock, CredentialSource, Credentials, Market, MarketAdapter, MarketCalendar, MarketError,
    Quote, Symbol,
};

mod http;
mod wire;

#[cfg(test)]
mod tests;

/// 현재가 경로.
const PRICES_PATH: &str = "/api/v1/prices";
/// 국내 장 시간 경로.
const CALENDAR_KR_PATH: &str = "/api/v1/market-calendar/KR";
/// 미국 장 시간 경로.
const CALENDAR_US_PATH: &str = "/api/v1/market-calendar/US";
/// 429에 `Retry-After`가 없을 때 기다리는 시간. 한도가 초당 기준이라 1초로 둔다.
const FALLBACK_RETRY_WAIT: Duration = Duration::from_secs(1);
/// `Retry-After`가 이것보다 길면 기다리지 않고 바로 한도 초과로 끝낸다
/// (부른 쪽을 오래 붙잡지 않게).
const MAX_RETRY_WAIT: Duration = Duration::from_secs(30);
/// 토큰이 무효가 됐다는 뜻의 오류 코드. 이때만 한 번 다시 발급한다.
const REISSUE_CODES: [&str; 2] = ["expired-token", "token-revoked"];

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
/// 토큰 발급이 인증 실패 · 허용 IP 아님으로 끝난 뒤 발급을 다시 보내지 않는 시간(기본값).
pub const DEFAULT_TOKEN_FAILURE_COOLDOWN: Duration = Duration::from_secs(60);
/// 읽는 응답 본문 크기의 한도 기본값(1 MiB). 현재가 200종목 응답이 넉넉히 들어간다.
pub const DEFAULT_MAX_RESPONSE_BYTES: usize = 1024 * 1024;

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
    /// 토큰 발급이 인증 실패([`MarketError::AuthFailed`]) · 허용 IP 아님
    /// ([`MarketError::IpNotAllowed`])으로 끝나면, 이 시간 동안 발급을 다시 보내지 않고
    /// 같은 오류를 돌려준다. 다른 실패(장애 · 연결 · 응답 모양)에는 쉬지 않는다.
    pub token_failure_cooldown: Duration,
    /// 응답 본문을 이 크기(바이트)까지만 읽는다. 넘으면 읽지 않고
    /// [`MarketError::UnexpectedResponse`]로 끝낸다(딱 이 크기는 읽는다).
    pub max_response_bytes: usize,
}

impl Default for TossConfig {
    /// [`DEFAULT_BASE_URL`] · [`DEFAULT_TIMEOUT`] · [`DEFAULT_TOKEN_REFRESH_MARGIN`] ·
    /// [`DEFAULT_RATE_LIMIT_RETRIES`] · [`DEFAULT_TOKEN_FAILURE_COOLDOWN`] ·
    /// [`DEFAULT_MAX_RESPONSE_BYTES`].
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout: DEFAULT_TIMEOUT,
            token_refresh_margin: DEFAULT_TOKEN_REFRESH_MARGIN,
            rate_limit_retries: DEFAULT_RATE_LIMIT_RETRIES,
            token_failure_cooldown: DEFAULT_TOKEN_FAILURE_COOLDOWN,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
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
/// - 토큰 유효 시간(`expires_in`)이 `token_refresh_margin` 이하이면 받지 않고
///   [`MarketError::UnexpectedResponse`]로 끝낸다(요청마다 다시 발급해 토큰을 깨지 않게).
/// - 발급이 인증 실패 · 허용 IP 아님이면 `token_failure_cooldown` 동안 발급을 다시 보내지
///   않고 같은 오류를 돌려준다(국내 · 미국 어댑터가 함께 쉰다).
/// - 기본 주소가 `https`가 아니면(루프백 주소 `127.0.0.0/8` · `::1`만 예외) 요청을 보내지
///   않고 [`MarketError::InsecureBaseUrl`]을 돌려준다. 응답 본문은 `max_response_bytes`까지만 읽는다.
#[derive(Clone)]
pub struct TossClient {
    inner: Arc<Inner>,
}

/// 쥐고 있는 토큰. `Debug`를 만들지 않는다(토큰 글자가 기록으로 나가지 않게).
struct CachedToken {
    value: String,
    expires_at: DateTime<Utc>,
    /// 이 토큰을 받을 때 쓴 자격. 자격이 바뀌면 이 토큰을 버린다.
    owner: Credentials,
}

struct Inner {
    config: TossConfig,
    /// 만들지 못했으면 요청할 때 그 오류를 돌려준다.
    http: Result<reqwest::Client, MarketError>,
    credentials: Arc<dyn CredentialSource>,
    clock: Arc<dyn Clock>,
    /// 발급하는 동안 쥐고 있어서, 동시에 온 요청들은 기다렸다가 같은 토큰을 쓴다.
    token: Mutex<Option<CachedToken>>,
}

impl Inner {
    /// 쓸 수 있는 토큰. 없거나 만료가 가까우면 발급한다.
    async fn token(
        &self,
        http: &reqwest::Client,
        credentials: &Credentials,
    ) -> Result<String, MarketError> {
        let mut slot = self.token.lock().await;
        let now = self.clock.now();
        if let Some(cached) = slot.as_ref() {
            let remaining = cached.expires_at.signed_duration_since(now).to_std();
            let fresh = remaining.is_ok_and(|left| left >= self.config.token_refresh_margin);
            if fresh && cached.owner.same_as(credentials) {
                return Ok(cached.value.clone());
            }
        }
        *slot = None;
        let issued = http::issue_token(http, &self.config, credentials).await?;
        // 유효 시간은 요청을 보내기 전 시각부터 센다(실제보다 늦게 만료로 보지 않게).
        let expires_at = i64::try_from(issued.expires_in)
            .ok()
            .and_then(chrono::Duration::try_seconds)
            .and_then(|life| now.checked_add_signed(life))
            .ok_or_else(|| http::unexpected("토큰"))?;
        *slot = Some(CachedToken {
            value: issued.value.clone(),
            expires_at,
            owner: credentials.clone(),
        });
        Ok(issued.value)
    }

    /// 무효가 된 토큰을 버린다. 그 사이 다른 요청이 이미 새 토큰을 받았으면 그대로 둔다.
    async fn discard_token(&self, used: &str) {
        let mut slot = self.token.lock().await;
        if slot.as_ref().is_some_and(|cached| cached.value == used) {
            *slot = None;
        }
    }

    /// 토큰을 실어 GET 하나를 보내고 본문을 돌려준다. 자격 확인 · 무효 토큰 다시 발급 ·
    /// 429 다시 시도를 여기서 한다.
    async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<String, MarketError> {
        let Some(credentials) = self.credentials.credentials() else {
            // 키가 지워졌으면 그 키로 받은 토큰도 더 쓰지 않는다.
            *self.token.lock().await = None;
            return Err(MarketError::NoCredentials);
        };
        let http = self.http.as_ref().map_err(Clone::clone)?;
        let url = http::endpoint(&self.config, path, query)?;
        let mut retries_left = self.config.rate_limit_retries;
        let mut reissued = false;
        loop {
            let token = self.token(http, &credentials).await?;
            let request = http.get(url.clone()).bearer_auth(&token);
            match http::send(request).await {
                Err(MarketError::AuthFailed { code })
                    if !reissued && REISSUE_CODES.contains(&code.as_str()) =>
                {
                    reissued = true;
                    self.discard_token(&token).await;
                }
                Err(MarketError::RateLimited { retry_after }) if retries_left > 0 => {
                    let wait = retry_after.unwrap_or(FALLBACK_RETRY_WAIT);
                    if wait > MAX_RETRY_WAIT {
                        return Err(MarketError::RateLimited { retry_after });
                    }
                    retries_left -= 1;
                    self.clock.sleep(wait).await;
                }
                other => return other,
            }
        }
    }
}

impl TossClient {
    /// 연결을 만든다. 이때는 바깥으로 요청을 보내지 않는다.
    pub fn new(
        config: TossConfig,
        credentials: Arc<dyn CredentialSource>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let http = http::build_client(&config);
        Self {
            inner: Arc::new(Inner {
                config,
                http,
                credentials,
                clock,
                token: Mutex::new(None),
            }),
        }
    }

    /// 그 시장의 어댑터. 토큰은 이 연결의 것을 함께 쓴다.
    pub fn adapter(&self, market: Market) -> TossAdapter {
        TossAdapter {
            inner: Arc::clone(&self.inner),
            market,
        }
    }
}

/// 토스증권의 시장 하나(국내 또는 미국).
pub struct TossAdapter {
    inner: Arc<Inner>,
    market: Market,
}

#[async_trait]
impl MarketAdapter for TossAdapter {
    fn market(&self) -> Market {
        self.market
    }

    /// `GET /api/v1/prices?symbols=a,b,c`. 200개가 넘으면 200개씩 나눠 부르고 순서대로 잇는다.
    /// `lastPrice` 문자열을 십진수로 읽는다. 받은 시각은 [`Clock::now`].
    ///
    /// 부르기 전에 모든 종목을 [`Symbol::parse`]의 규칙으로 확인한다. 틀린 종목이 하나라도
    /// 있으면 아무 요청(토큰 발급 포함)도 보내지 않고 [`MarketError::InvalidSymbol`]이다.
    /// 0 이하의 가격은 [`MarketError::UnexpectedResponse`]다. 요청하지 않은 종목은 버린다.
    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
        let mut quotes = Vec::with_capacity(symbols.len());
        for chunk in symbols.chunks(MAX_SYMBOLS_PER_REQUEST) {
            let joined = chunk
                .iter()
                .map(Symbol::as_str)
                .collect::<Vec<_>>()
                .join(",");
            let body = self.inner.get(PRICES_PATH, &[("symbols", &joined)]).await?;
            quotes.extend(wire::parse_quotes(&body, self.inner.clock.now())?);
        }
        Ok(quotes)
    }

    /// `GET /api/v1/market-calendar/KR?date=YYYY-MM-DD`(국내) 또는 `/US`(미국).
    /// null인 세션은 뺀다. 국내 `integrated`가 null이면 그날 세션이 없다.
    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError> {
        let path = match self.market {
            Market::Kr => CALENDAR_KR_PATH,
            Market::Us => CALENDAR_US_PATH,
        };
        let date = date.format("%Y-%m-%d").to_string();
        let body = self.inner.get(path, &[("date", &date)]).await?;
        wire::parse_calendar(&body, self.market)
    }
}

/// 이 자격으로 토큰이 발급되는지 확인한다(토큰 발급 한 번).
///
/// 주의: 토스증권은 다시 발급하면 이전 토큰이 무효가 된다. 돌고 있는 [`TossClient`]가
/// 같은 자격을 쓰고 있었다면 그쪽은 `token-revoked`를 받고 한 번 다시 발급한다.
///
/// 기본 주소 규칙(`https`, 루프백만 예외) · 토큰 유효 시간 규칙 · 응답 크기 한도는
/// [`TossClient`]와 같다.
///
/// 한 번만 보낸다: 429여도 기다렸다가 다시 보내지 않고 [`MarketError::RateLimited`]를 돌려준다.
pub async fn verify(config: &TossConfig, credentials: &Credentials) -> Result<(), MarketError> {
    let http = http::build_client(config)?;
    http::issue_token(&http, config, credentials).await?;
    Ok(())
}
