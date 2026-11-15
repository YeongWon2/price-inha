//! 출처와 무관한 어댑터 규칙: 키 · 시계 · 기본 주소 · 연결 실패 · 시간 초과 · 종류 대응.
//! 로컬 가짜 서버(wiremock)만 쓴다.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use pi_data::{
    CorpCode, DEFAULT_DART_BASE_URL, DEFAULT_ECOS_BASE_URL, DEFAULT_FRED_BASE_URL, DEFAULT_TIMEOUT,
    DataAdapter, DataClient, DataConfig, DataError, EcosCycle, EcosSeries, FactKind, FactRequest,
    Fetched, FsDiv, KeySource, ReportCode, SourceName,
};
use pi_secrets::{Credentials, KeyKind, KeySlot, SecretStore, Verifier, VerifyFailure};
use serde_json::json;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn corp() -> CorpCode {
    CorpCode::parse("00126380").unwrap()
}

fn requests() -> Vec<FactRequest> {
    let series = EcosSeries {
        stat_code: "722Y001".to_owned(),
        item_code: "0101000".to_owned(),
        cycle: EcosCycle::Month,
        start: "202609".to_owned(),
        end: "202610".to_owned(),
    };
    vec![
        FactRequest::Disclosures {
            corp: corp(),
            from: day("2026-11-01"),
            to: day("2026-11-15"),
        },
        FactRequest::Financials {
            corp: corp(),
            year: 2025,
            report: ReportCode::Annual,
            fs: FsDiv::Consolidated,
        },
        FactRequest::KoreaRate(series.clone()),
        FactRequest::KoreaFx(series),
        FactRequest::UsRate {
            series_id: "DGS10".to_owned(),
            from: day("2026-11-12"),
            to: day("2026-11-13"),
        },
    ]
}

fn fred_request() -> FactRequest {
    requests().pop().unwrap()
}

fn fred_body() -> serde_json::Value {
    json!({
        "realtime_start": "2026-11-14", "realtime_end": "2026-11-14",
        "observation_start": "2026-11-12", "observation_end": "2026-11-13",
        "units": "lin", "output_type": 1, "file_type": "json",
        "order_by": "observation_date", "sort_order": "asc",
        "count": 1, "offset": 0, "limit": 100000,
        "observations": [
            { "realtime_start": "2026-11-14", "realtime_end": "2026-11-14", "date": "2026-11-13", "value": "4.09" }
        ]
    })
}

async fn mount_fred(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/fred/series/observations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fred_body()))
        .mount(server)
        .await;
}

/// 아무 요청에나 정상 같은 답을 주는 서버(키 없음 시험에서 요청이 나갔는지만 센다).
async fn mount_everything(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path_regex(".*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "status": "013" })))
        .mount(server)
        .await;
}

// 검증: C11
#[test]
fn 종류마다_출처가_정해져_있다() {
    assert_eq!(FactKind::Disclosure.source(), SourceName::Dart);
    assert_eq!(FactKind::Financials.source(), SourceName::Dart);
    assert_eq!(FactKind::KoreaRate.source(), SourceName::Ecos);
    assert_eq!(FactKind::KoreaFx.source(), SourceName::Ecos);
    assert_eq!(FactKind::UsRate.source(), SourceName::Fred);
    let kinds: Vec<FactKind> = requests().iter().map(FactRequest::kind).collect();
    assert_eq!(kinds, FactKind::ALL.to_vec());
}

// 검증: C11
#[test]
fn 출처마다_보관소의_칸과_보이는_이름이_정해져_있다() {
    assert_eq!(SourceName::Dart.slot(), KeySlot::Dart);
    assert_eq!(SourceName::Ecos.slot(), KeySlot::Ecos);
    assert_eq!(SourceName::Fred.slot(), KeySlot::Fred);
    assert_eq!(SourceName::Dart.to_string(), "DART");
    assert_eq!(SourceName::Ecos.to_string(), "ECOS");
    assert_eq!(SourceName::Fred.to_string(), "FRED");
}

// 검증: E14
#[tokio::test]
async fn 키가_없으면_요청을_보내지_않고_그_출처의_키_없음이다() {
    let server = MockServer::start().await;
    mount_everything(&server).await;
    let adapter = client(&server, TestKeys::none(), TestClock::at(START));

    for request in requests() {
        let err = adapter.fetch(&request).await.unwrap_err();
        assert_eq!(
            err,
            DataError::NoKey {
                origin: request.kind().source()
            }
        );
    }
    assert_eq!(hits(&server).await, 0);
    for source in SourceName::ALL {
        assert!(!adapter.has_key(source));
    }
}

// 검증: E14
#[tokio::test]
async fn 한_출처의_키만_있으면_다른_출처는_여전히_키_없음이다() {
    let server = MockServer::start().await;
    mount_fred(&server).await;
    let keys = TestKeys::none();
    keys.put(SourceName::Fred, FRED_KEY);
    let adapter = client(&server, keys, TestClock::at(START));

    assert!(matches!(
        adapter.fetch(&fred_request()).await.unwrap(),
        Fetched::Facts(_)
    ));
    let err = adapter.fetch(&requests()[0]).await.unwrap_err();
    assert_eq!(
        err,
        DataError::NoKey {
            origin: SourceName::Dart
        }
    );
    assert!(adapter.has_key(SourceName::Fred));
    assert!(!adapter.has_key(SourceName::Dart));
}

// 검증: E14
#[tokio::test]
async fn 나중에_넣은_키는_다시_만들지_않아도_바로_쓴다() {
    let server = MockServer::start().await;
    mount_fred(&server).await;
    let keys = TestKeys::none();
    let adapter = client(&server, keys.clone(), TestClock::at(START));

    assert_eq!(
        adapter.fetch(&fred_request()).await.unwrap_err(),
        DataError::NoKey {
            origin: SourceName::Fred
        }
    );
    keys.put(SourceName::Fred, FRED_KEY);

    assert!(matches!(
        adapter.fetch(&fred_request()).await.unwrap(),
        Fetched::Facts(_)
    ));
}

// 검증: V36 P10
#[tokio::test]
async fn 알게_된_시각은_받을_때의_가짜_시계_값이다() {
    let server = MockServer::start().await;
    mount_fred(&server).await;
    let clock = TestClock::at(START);
    let adapter = client(&server, TestKeys::all(), clock.clone());

    let first = match adapter.fetch(&fred_request()).await.unwrap() {
        Fetched::Facts(list) => list,
        other => panic!("{other:?}"),
    };
    clock.set("2026-11-16T15:45:00+09:00");
    let second = match adapter.fetch(&fred_request()).await.unwrap() {
        Fetched::Facts(list) => list,
        other => panic!("{other:?}"),
    };

    assert_eq!(first[0].known_at, ts(START));
    assert_eq!(second[0].known_at, ts("2026-11-16T15:45:00+09:00"));
}

// 검증: V36 P10
#[tokio::test]
async fn 판단_시각보다_늦게_안_자료는_그_판단에_쓸_수_없다() {
    let server = MockServer::start().await;
    mount_fred(&server).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let fact = match adapter.fetch(&fred_request()).await.unwrap() {
        Fetched::Facts(mut list) => list.remove(0),
        other => panic!("{other:?}"),
    };

    assert!(fact.known_by(ts(START)));
    assert!(fact.known_by(ts("2026-11-16T09:30:01+09:00")));
    assert!(!fact.known_by(ts("2026-11-16T09:29:59+09:00")));
}

// 검증: E14
#[tokio::test]
async fn 정해진_시간_안에_답이_없으면_시간_초과이고_주소에_키가_없다() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fred/series/observations"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fred_body())
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&server)
        .await;
    let adapter = DataClient::new(
        config(&server).with_timeout(Duration::from_millis(200)),
        TestKeys::all(),
        TestClock::at(START),
    )
    .unwrap();

    let err = adapter.fetch(&fred_request()).await.unwrap_err();

    match &err {
        DataError::Timeout { origin, url } => {
            assert_eq!(*origin, SourceName::Fred);
            assert!(url.contains("/fred/series/observations"));
            assert!(!url.contains(FRED_KEY));
        }
        other => panic!("시간 초과여야 한다: {other:?}"),
    }
    assert!(!has_any_key(&format!("{err} {err:?}")));
}

// 검증: E14
#[tokio::test]
async fn 서버가_없으면_닿지_않음이고_주소에_키가_없다() {
    let server = MockServer::start().await;
    let gone = config(&server);
    drop(server);
    let adapter = DataClient::new(gone, TestKeys::all(), TestClock::at(START)).unwrap();

    for request in [requests().remove(0), requests().remove(2), fred_request()] {
        let err = adapter.fetch(&request).await.unwrap_err();
        match &err {
            DataError::Unreachable { origin, .. } => {
                assert_eq!(*origin, request.kind().source());
            }
            other => panic!("닿지 않음이어야 한다: {other:?}"),
        }
        assert!(!has_any_key(&format!("{err} {err:?}")), "{err:?}");
    }
}

// 검증: E14
#[test]
fn 기본_주소가_https가_아니면_만들지_않는다() {
    let plain = "http://192.0.2.10";
    let cases = [
        (
            DataConfig::new(plain, "https://192.0.2.11", "https://192.0.2.12"),
            SourceName::Dart,
        ),
        (
            DataConfig::new("https://192.0.2.10", plain, "https://192.0.2.12"),
            SourceName::Ecos,
        ),
        (
            DataConfig::new("https://192.0.2.10", "https://192.0.2.11", plain),
            SourceName::Fred,
        ),
    ];
    for (config, source) in cases {
        let result = DataClient::new(config, TestKeys::all(), TestClock::at(START));
        assert!(
            matches!(result, Err(DataError::InsecureBaseUrl { origin }) if origin == source),
            "{source}"
        );
    }
    assert!(
        DataClient::new(
            DataConfig::new("http://127.0.0.1:1", "http://localhost:1", "http://[::1]:1"),
            TestKeys::all(),
            TestClock::at(START),
        )
        .is_ok()
    );
}

// 검증: C11
#[test]
fn 공식_설정은_https_기본_주소와_기본_시간_한도를_쓴다() {
    let official = DataConfig::official();
    for url in [
        DEFAULT_DART_BASE_URL,
        DEFAULT_ECOS_BASE_URL,
        DEFAULT_FRED_BASE_URL,
    ] {
        assert!(url.starts_with("https://"), "{url}");
    }
    assert_eq!(official.dart_base_url, DEFAULT_DART_BASE_URL);
    assert_eq!(official.ecos_base_url, DEFAULT_ECOS_BASE_URL);
    assert_eq!(official.fred_base_url, DEFAULT_FRED_BASE_URL);
    assert_eq!(official.timeout, DEFAULT_TIMEOUT);
    assert!(DataClient::new(official, TestKeys::none(), TestClock::at(START)).is_ok());
}

/// 칸 확인을 언제나 받아들이는 확인기(보관소에 칸을 등록하려고).
struct AcceptAll;

impl Verifier for AcceptAll {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_slot(
        &self,
        _slot: KeySlot,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

// 검증: E14
#[tokio::test]
async fn 보관소의_출처별_칸에서_키를_읽는다() {
    let dir = tempfile::tempdir().unwrap();
    let store = SecretStore::open(dir.path()).unwrap();
    for source in SourceName::ALL {
        assert!(store.key(source).is_none());
    }

    store
        .register_slot(
            KeySlot::Fred,
            Credentials::new().with(KeySlot::Fred.fields()[0], FRED_KEY),
            &AcceptAll,
        )
        .await
        .unwrap();

    assert_eq!(store.key(SourceName::Fred).unwrap().expose(), FRED_KEY);
    assert!(store.key(SourceName::Dart).is_none());
    assert!(store.key(SourceName::Ecos).is_none());
}

// 검증: E14
#[tokio::test]
async fn 보관소를_키_주는_곳으로_끼우면_등록한_키로_요청한다() {
    let server = MockServer::start().await;
    mount_fred(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let store = SecretStore::open(dir.path()).unwrap();
    store
        .register_slot(
            KeySlot::Fred,
            Credentials::new().with(KeySlot::Fred.fields()[0], FRED_KEY),
            &AcceptAll,
        )
        .await
        .unwrap();
    let keys: Arc<dyn KeySource> = Arc::new(store);
    let adapter = DataClient::new(config(&server), keys, TestClock::at(START)).unwrap();

    assert!(matches!(
        adapter.fetch(&fred_request()).await.unwrap(),
        Fetched::Facts(_)
    ));
    assert_eq!(
        adapter.fetch(&requests()[0]).await.unwrap_err(),
        DataError::NoKey {
            origin: SourceName::Dart
        }
    );
    let seen = server.received_requests().await.unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].url.query().unwrap_or_default().contains(FRED_KEY));
}
