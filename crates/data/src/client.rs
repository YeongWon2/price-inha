//! 실제 출처(DART · ECOS · FRED)를 부르는 어댑터.
//!
//! - DART: `GET {base}/api/list.json`, `GET {base}/api/fnlttSinglAcnt.json`, 키는 쿼리 `crtfc_key`.
//!   HTTP는 늘 200이고 본문의 `status`로 결과를 가른다(`000` 정상, `013` 없음, `020` 한도 등).
//! - ECOS: `GET {base}/api/StatisticSearch/{키}/json/kr/{시작행}/{끝행}/{통계표}/{주기}/{시작}/{끝}/{항목}`.
//!   키가 경로에 실린다. 없음은 `{"RESULT":{"CODE":"INFO-200",..}}`.
//! - FRED: `GET {base}/fred/series/observations?series_id=..&api_key=..&file_type=json&observation_start=..&observation_end=..`.
//!
//! 오류 · 출처 주소에는 키를 가린 주소만 싣는다.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::{Clock, DataAdapter, DataError, FactRequest, Fetched, KeySource, SourceName};

/// DART 기본 주소.
pub const DEFAULT_DART_BASE_URL: &str = "https://opendart.fss.or.kr";
/// ECOS 기본 주소.
pub const DEFAULT_ECOS_BASE_URL: &str = "https://ecos.bok.or.kr";
/// FRED 기본 주소.
pub const DEFAULT_FRED_BASE_URL: &str = "https://api.stlouisfed.org";
/// 요청 하나의 기본 시간 한도.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

/// 어댑터 설정. 기본 주소는 생성자 인자로 받는다(테스트는 로컬 가짜 서버 주소를 준다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataConfig {
    /// DART 기본 주소.
    pub dart_base_url: String,
    /// ECOS 기본 주소.
    pub ecos_base_url: String,
    /// FRED 기본 주소.
    pub fred_base_url: String,
    /// 요청 하나의 시간 한도.
    pub timeout: Duration,
}

impl DataConfig {
    /// 세 출처의 기본 주소로 만든다. 시간 한도는 [`DEFAULT_TIMEOUT`].
    pub fn new(
        dart_base_url: impl Into<String>,
        ecos_base_url: impl Into<String>,
        fred_base_url: impl Into<String>,
    ) -> Self {
        Self {
            dart_base_url: dart_base_url.into(),
            ecos_base_url: ecos_base_url.into(),
            fred_base_url: fred_base_url.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// 공식 주소([`DEFAULT_DART_BASE_URL`] 등)로 만든다.
    pub fn official() -> Self {
        Self::new(
            DEFAULT_DART_BASE_URL,
            DEFAULT_ECOS_BASE_URL,
            DEFAULT_FRED_BASE_URL,
        )
    }

    /// 시간 한도를 바꾼다.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// 실제 출처를 부르는 자료 어댑터. reqwest(rustls)를 쓴다.
pub struct DataClient {
    config: DataConfig,
    keys: Arc<dyn KeySource>,
    clock: Arc<dyn Clock>,
}

impl DataClient {
    /// 만든다. 기본 주소가 `https`가 아니면(루프백 주소는 예외) [`DataError::InsecureBaseUrl`].
    pub fn new(
        config: DataConfig,
        keys: Arc<dyn KeySource>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, DataError> {
        let _ = (config, keys, clock);
        todo!()
    }

    /// 출처의 키가 지금 있는가.
    pub fn has_key(&self, source: SourceName) -> bool {
        let _ = (source, &self.config, &self.keys, &self.clock);
        todo!()
    }
}

#[async_trait]
impl DataAdapter for DataClient {
    async fn fetch(&self, request: &FactRequest) -> Result<Fetched, DataError> {
        let _ = request;
        todo!()
    }
}
