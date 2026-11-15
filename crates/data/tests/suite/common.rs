//! 테스트 공용: 테스트가 정하는 시계와 키, 로컬 가짜 서버로 만든 어댑터.
//! 실제 출처 주소는 쓰지 않는다. 키는 진짜처럼 보이지 않는 글자다.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use pi_data::{Clock, DataClient, DataConfig, KeySource, SourceName};
use pi_secrets::SecretString;
use rust_decimal::Decimal;
use wiremock::MockServer;

pub const DART_KEY: &str = "test-dart-key-not-real-0001";
pub const ECOS_KEY: &str = "test-ecos-key-not-real-0001";
pub const FRED_KEY: &str = "test-fred-key-not-real-0001";
pub const START: &str = "2026-11-16T09:30:00+09:00";

pub fn ts(text: &str) -> DateTime<Utc> {
    text.parse::<DateTime<Utc>>().unwrap()
}

pub fn day(text: &str) -> NaiveDate {
    text.parse::<NaiveDate>().unwrap()
}

pub fn dec(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

/// 테스트가 정하는 시계.
pub struct TestClock(Mutex<DateTime<Utc>>);

impl TestClock {
    pub fn at(text: &str) -> Arc<Self> {
        Arc::new(Self(Mutex::new(ts(text))))
    }

    pub fn set(&self, text: &str) {
        *self.0.lock().unwrap() = ts(text);
    }
}

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

/// 테스트가 정하는 키. 처음에 없을 수 있고 나중에 넣을 수 있다.
#[derive(Default)]
pub struct TestKeys(Mutex<HashMap<SourceName, String>>);

impl TestKeys {
    pub fn all() -> Arc<Self> {
        let keys = Self::default();
        keys.put(SourceName::Dart, DART_KEY);
        keys.put(SourceName::Ecos, ECOS_KEY);
        keys.put(SourceName::Fred, FRED_KEY);
        Arc::new(keys)
    }

    pub fn none() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn put(&self, source: SourceName, value: &str) {
        self.0.lock().unwrap().insert(source, value.to_owned());
    }
}

impl KeySource for TestKeys {
    fn key(&self, source: SourceName) -> Option<SecretString> {
        self.0
            .lock()
            .unwrap()
            .get(&source)
            .map(|value| SecretString::new(value.clone()))
    }
}

/// 세 출처 모두 같은 로컬 가짜 서버를 가리키는 설정.
pub fn config(server: &MockServer) -> DataConfig {
    DataConfig::new(server.uri(), server.uri(), server.uri()).with_timeout(Duration::from_secs(5))
}

pub fn client(server: &MockServer, keys: Arc<TestKeys>, clock: Arc<TestClock>) -> DataClient {
    DataClient::new(config(server), keys, clock).unwrap()
}

/// 받은 요청 수.
pub async fn hits(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

/// 글 안에 세 키 중 하나라도 있는가.
pub fn has_any_key(text: &str) -> bool {
    [DART_KEY, ECOS_KEY, FRED_KEY]
        .iter()
        .any(|key| text.contains(key))
}
