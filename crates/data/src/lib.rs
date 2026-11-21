//! pi-data: 자료 어댑터(공시 · 재무 · 거시). 구성 요소 A10.
//!
//! 작업 지시서: `docs/dev/modules/data.md` (1차: DART · ECOS · FRED, 4차: 뉴스 공급자 · 네이버 뉴스).
//!
//! 판단 쪽은 [`DataAdapter`] trait만 안다. 받은 자료([`Fact`])마다 출처 · 주소 · 공표일 ·
//! 알게 된 시각(주입한 [`Clock`])이 붙는다(P10). 자료 키는 [`KeySource`]로 받는다.

mod adapter;
mod client;
mod clock;
mod dart;
mod ecos;
pub mod error;
mod fred;
mod http;
mod keys;
mod model;
mod naver;
mod news;
mod verify;

pub use adapter::{DataAdapter, FakeDataAdapter};
pub use client::{
    DEFAULT_DART_BASE_URL, DEFAULT_ECOS_BASE_URL, DEFAULT_FRED_BASE_URL, DEFAULT_TIMEOUT,
    DataClient, DataConfig,
};
pub use clock::Clock;
pub use error::{DataError, NewsError};
pub use keys::KeySource;
pub use model::{
    CorpCode, EcosCycle, EcosSeries, Fact, FactKind, FactRequest, FactValue, Fetched, FsDiv,
    ReportCode, Source, SourceName,
};
pub use naver::{
    DEFAULT_NAVER_NEWS_BASE_URL, DEFAULT_NEWS_MAX_BODY_BYTES, NAVER_NEWS_KEYS, NaverNews,
    NaverNewsConfig, NewsKeySource, verify_news_keys,
};
pub use news::{
    FakeNewsProvider, NewsFetched, NewsItem, NewsKeys, NewsProvider, NewsProviderId, NewsRegistry,
    NewsRequest, NewsSort, clean_news_text,
};
pub use verify::{verify_key, verify_slot};
