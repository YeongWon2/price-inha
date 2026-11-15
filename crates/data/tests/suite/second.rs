//! 2차 작업(`docs/dev/modules/data.md` 2차 1~5번). 로컬 가짜 서버(wiremock)만 쓴다.
//!
//! 1. 받을 양의 상한(ECOS 10000행, DART 20쪽)을 넘으면 잘렸다는 오류([`DataError::Truncated`]).
//! 2. 키 보관소 읽기 오류(입출력 · 손상)는 키 없음이 아니라 [`DataError::KeyUnreadable`].
//! 3. 확인기에서 404 · 3xx는 키 거절이 아니라 닿지 않음.
//! 4. FRED 관측값의 `realtime_start`는 실제 공표일이 아니다: 공표일 칸을 비운다.
//! 5. 리다이렉트를 따라가지 않고, 본문 상한을 넘으면 형식 오류다.
//!
//! 가짜 응답 모양은 각 출처 파일(`dart.rs` · `ecos.rs` · `fred.rs`)의 머리 주석과 같다
//! (DART 공시검색 `total_page`, ECOS `StatisticSearch.list_total_count`).

use std::sync::Arc;

use crate::client::AcceptAll;
use crate::common::*;
use pi_data::{
    CorpCode, DataAdapter, DataClient, DataError, EcosCycle, EcosSeries, FactRequest, Fetched,
    KeySource, SourceName, verify_key, verify_slot,
};
use pi_secrets::{Credentials, ENV_FILE, SecretStore, SecretString, VerifyFailure};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CORP: &str = "00126380";
const RATE_STAT: &str = "722Y001";
const RATE_ITEM: &str = "0101000";
/// ECOS 한 번에 받는 행 수의 상한.
const ECOS_MAX_ROWS: usize = 10_000;
/// DART 공시 목록에서 받는 쪽 수의 상한.
const DART_MAX_PAGES: u32 = 20;

fn disclosures() -> FactRequest {
    FactRequest::Disclosures {
        corp: CorpCode::parse(CORP).unwrap(),
        from: day("2026-11-01"),
        to: day("2026-11-15"),
    }
}

fn rate_request() -> FactRequest {
    FactRequest::KoreaRate(EcosSeries {
        stat_code: RATE_STAT.to_owned(),
        item_code: RATE_ITEM.to_owned(),
        cycle: EcosCycle::Month,
        start: "199901".to_owned(),
        end: "202610".to_owned(),
    })
}

fn fred_request() -> FactRequest {
    FactRequest::UsRate {
        series_id: "DGS10".to_owned(),
        from: day("2026-11-12"),
        to: day("2026-11-13"),
    }
}

/// 공시 목록 한 쪽(공시 1건). `total_page`는 출처가 알려 준 전체 쪽 수다.
fn dart_page(total_page: u32) -> Value {
    json!({
        "status": "000",
        "message": "정상",
        "page_no": 1,
        "page_count": 100,
        "total_count": total_page * 100,
        "total_page": total_page,
        "list": [{
            "corp_cls": "Y",
            "corp_name": "시험전자",
            "corp_code": CORP,
            "stock_code": "005930",
            "report_nm": "주요사항보고서(자기주식취득결정)",
            "rcept_no": "20261114000123",
            "flr_nm": "시험전자",
            "rcept_dt": "20261114",
            "rm": "유"
        }]
    })
}

/// ECOS 통계 응답: 행 `rows`개, 출처가 알려 준 전체 행 수 `total`.
fn ecos_body(rows: usize, total: usize) -> Value {
    let row = json!({
        "STAT_CODE": RATE_STAT,
        "STAT_NAME": "1.3.1. 한국은행 기준금리 및 여수신금리",
        "ITEM_CODE1": RATE_ITEM,
        "ITEM_NAME1": "한국은행 기준금리",
        "ITEM_CODE2": null, "ITEM_NAME2": null,
        "ITEM_CODE3": null, "ITEM_NAME3": null,
        "ITEM_CODE4": null, "ITEM_NAME4": null,
        "UNIT_NAME": "연%",
        "WGT": null,
        "TIME": "202610",
        "DATA_VALUE": "2.25"
    });
    json!({
        "StatisticSearch": {
            "list_total_count": total,
            "row": vec![row; rows]
        }
    })
}

async fn mount_dart_list(server: &MockServer, template: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/api/list.json"))
        .respond_with(template)
        .mount(server)
        .await;
}

async fn mount_ecos(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path_regex("^/api/StatisticSearch/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// 다른 곳(`/elsewhere`): 따라가면 받아들이는 응답을 준다. 몇 번 불렸는지 본다.
async fn mount_elsewhere(server: &MockServer) -> String {
    Mock::given(method("GET"))
        .and(path("/elsewhere"))
        .respond_with(ResponseTemplate::new(200).set_body_json(dart_page(1)))
        .mount(server)
        .await;
    format!("{}/elsewhere", server.uri())
}

async fn elsewhere_hits(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/elsewhere")
        .count()
}

/// 확인기가 묻는 곳(세 출처 모두) 어디로 가든 같은 응답을 준다.
async fn mount_probe(server: &MockServer, template: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path_regex("^/(api|fred)/"))
        .respond_with(template)
        .mount(server)
        .await;
}

fn source_key(source: SourceName) -> &'static str {
    match source {
        SourceName::Dart => DART_KEY,
        SourceName::Ecos => ECOS_KEY,
        SourceName::Fred => FRED_KEY,
    }
}

fn creds(source: SourceName) -> Credentials {
    Credentials::new().with(source.slot().fields()[0], source_key(source))
}

/// 보관소 폴더의 키 파일을 깨진 글(UTF-8 아님)로 만든다.
fn corrupt_env(dir: &std::path::Path) {
    std::fs::write(dir.join(ENV_FILE), [0xff_u8, 0xfe, 0x00, 0x80]).unwrap();
}

fn facts_len(fetched: Fetched) -> usize {
    match fetched {
        Fetched::Facts(list) => list.len(),
        other => panic!("자료가 와야 한다: {other:?}"),
    }
}

// ---- 1. 받을 양의 상한 ----

// 검증: C11 E14
#[tokio::test]
async fn dart_공시가_20쪽을_넘으면_잘린_목록을_돌려주지_않고_잘림_오류다() {
    let server = MockServer::start().await;
    mount_dart_list(
        &server,
        ResponseTemplate::new(200).set_body_json(dart_page(DART_MAX_PAGES + 1)),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&disclosures()).await.unwrap_err();

    assert_eq!(
        err,
        DataError::Truncated {
            origin: SourceName::Dart
        }
    );
    assert!(hits(&server).await <= DART_MAX_PAGES as usize);
    assert!(!has_any_key(&format!("{err:?} {err}")));
}

// 검증: C11
#[tokio::test]
async fn dart_공시가_딱_20쪽이면_모두_받아_돌려준다() {
    let server = MockServer::start().await;
    mount_dart_list(
        &server,
        ResponseTemplate::new(200).set_body_json(dart_page(DART_MAX_PAGES)),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let got = adapter.fetch(&disclosures()).await.unwrap();

    assert_eq!(facts_len(got), DART_MAX_PAGES as usize);
    assert_eq!(hits(&server).await, DART_MAX_PAGES as usize);
}

// 검증: C11 E14
#[tokio::test]
async fn ecos_전체_행이_10000행을_넘으면_잘린_결과를_돌려주지_않고_잘림_오류다() {
    let server = MockServer::start().await;
    mount_ecos(&server, ecos_body(ECOS_MAX_ROWS, ECOS_MAX_ROWS + 1)).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&rate_request()).await.unwrap_err();

    assert_eq!(
        err,
        DataError::Truncated {
            origin: SourceName::Ecos
        }
    );
    assert!(!has_any_key(&format!("{err:?} {err}")));
}

// 검증: C11
#[tokio::test]
async fn ecos_전체_행이_딱_10000행이면_모두_돌려준다() {
    let server = MockServer::start().await;
    mount_ecos(&server, ecos_body(ECOS_MAX_ROWS, ECOS_MAX_ROWS)).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let got = adapter.fetch(&rate_request()).await.unwrap();

    assert_eq!(facts_len(got), ECOS_MAX_ROWS);
}

// ---- 2. 키 보관소 읽기 오류 ----

// 검증: E14
#[tokio::test]
async fn 보관소_키_파일이_깨졌으면_키_없음이_아니라_읽기_오류이고_요청을_보내지_않는다() {
    let server = MockServer::start().await;
    mount_probe(&server, ResponseTemplate::new(200)).await;
    let dir = tempfile::tempdir().unwrap();
    let store = SecretStore::open(dir.path()).unwrap();
    corrupt_env(dir.path());
    let keys: Arc<dyn KeySource> = Arc::new(store);
    let adapter = DataClient::new(config(&server), keys, TestClock::at(START)).unwrap();

    let err = adapter.fetch(&fred_request()).await.unwrap_err();

    assert_eq!(
        err,
        DataError::KeyUnreadable {
            origin: SourceName::Fred
        }
    );
    assert_eq!(hits(&server).await, 0);
}

// 검증: E14
#[test]
fn 보관소의_키_읽기는_깨짐과_입출력_오류를_읽기_오류로_낸다() {
    let broken = tempfile::tempdir().unwrap();
    let store = SecretStore::open(broken.path()).unwrap();
    corrupt_env(broken.path());
    for source in SourceName::ALL {
        let err = store.read_key(source).unwrap_err();
        assert_eq!(err, DataError::KeyUnreadable { origin: source });
        assert!(!format!("{err:?} {err}").contains(&broken.path().display().to_string()));
    }

    // 키 파일 자리에 폴더가 있으면 읽다가 입출력 오류가 난다.
    let io = tempfile::tempdir().unwrap();
    let store = SecretStore::open(io.path()).unwrap();
    std::fs::create_dir(io.path().join(ENV_FILE)).unwrap();
    assert_eq!(
        store.read_key(SourceName::Dart).unwrap_err(),
        DataError::KeyUnreadable {
            origin: SourceName::Dart
        }
    );
}

// 검증: E14
#[tokio::test]
async fn 보관소의_키_읽기는_없으면_없음이고_있으면_그_키다() {
    let dir = tempfile::tempdir().unwrap();
    let store = SecretStore::open(dir.path()).unwrap();
    for source in SourceName::ALL {
        assert!(store.read_key(source).unwrap().is_none());
    }

    store
        .register_slot(SourceName::Ecos.slot(), creds(SourceName::Ecos), &AcceptAll)
        .await
        .unwrap();

    assert_eq!(
        store.read_key(SourceName::Ecos).unwrap().unwrap().expose(),
        ECOS_KEY
    );
    assert!(store.read_key(SourceName::Dart).unwrap().is_none());
    assert!(store.read_key(SourceName::Fred).unwrap().is_none());
}

/// 읽기 오류가 없는 키 주는 곳(테스트용 `TestKeys`)은 `read_key`가 `key`와 같다.
// 검증: E14
#[test]
fn 읽기_오류가_없는_키_주는_곳은_read_key가_key와_같다() {
    let keys = TestKeys::none();
    keys.put(SourceName::Dart, DART_KEY);

    assert_eq!(
        keys.read_key(SourceName::Dart)
            .unwrap()
            .map(|key| key.expose().to_owned()),
        Some(DART_KEY.to_owned())
    );
    assert!(keys.read_key(SourceName::Fred).unwrap().is_none());
}

// ---- 3. 확인기의 404 · 3xx ----

fn not_key_rejection(err: &DataError) -> bool {
    !matches!(
        err,
        DataError::KeyRejected { .. } | DataError::Rejected { .. }
    )
}

// 검증: E14
#[tokio::test]
async fn 확인기에서_404는_키_거절이_아니라_닿지_않음이다() {
    for source in SourceName::ALL {
        let server = MockServer::start().await;
        mount_probe(&server, ResponseTemplate::new(404)).await;
        let config = config(&server);

        let err = verify_key(&config, source, &SecretString::new(source_key(source)))
            .await
            .unwrap_err();
        assert!(not_key_rejection(&err), "{source}: {err:?}");
        assert!(!has_any_key(&format!("{err:?} {err}")));

        assert_eq!(
            verify_slot(&config, source.slot(), &creds(source)).await,
            Err(VerifyFailure::Unreachable),
            "{source}"
        );
    }
}

// 검증: E14
#[tokio::test]
async fn 확인기에서_3xx는_따라가지_않고_닿지_않음이다() {
    for status in [301_u16, 302, 307, 308] {
        for source in SourceName::ALL {
            let server = MockServer::start().await;
            let elsewhere = mount_elsewhere(&server).await;
            mount_probe(
                &server,
                ResponseTemplate::new(status).insert_header("Location", elsewhere.as_str()),
            )
            .await;
            let config = config(&server);

            let err = verify_key(&config, source, &SecretString::new(source_key(source)))
                .await
                .unwrap_err();
            assert!(not_key_rejection(&err), "{status} {source}: {err:?}");

            assert_eq!(
                verify_slot(&config, source.slot(), &creds(source)).await,
                Err(VerifyFailure::Unreachable),
                "{status} {source}"
            );
            assert_eq!(elsewhere_hits(&server).await, 0, "{status} {source}");
        }
    }
}

// ---- 4. FRED 공표일 ----

// 검증: V33 V34 V36 P10
#[tokio::test]
async fn fred_관측값의_real_time_시작일은_공표일로_쓰지_않는다() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fred/series/observations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "realtime_start": "2026-11-16",
            "realtime_end": "2026-11-16",
            "observation_start": "2026-11-12",
            "observation_end": "2026-11-13",
            "units": "lin",
            "output_type": 1,
            "file_type": "json",
            "order_by": "observation_date",
            "sort_order": "asc",
            "count": 1,
            "offset": 0,
            "limit": 100000,
            "observations": [
                {"realtime_start": "2026-11-16", "realtime_end": "2026-11-16", "date": "2026-11-13", "value": "4.09"}
            ]
        })))
        .mount(&server)
        .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let list = match adapter.fetch(&fred_request()).await.unwrap() {
        Fetched::Facts(list) => list,
        other => panic!("자료가 와야 한다: {other:?}"),
    };

    assert_eq!(list.len(), 1);
    assert_eq!(list[0].published_on, None);
    assert_eq!(list[0].period.as_deref(), Some("2026-11-13"));
    assert_eq!(list[0].known_at, ts(START));
    assert!(list[0].known_by(ts(START)));
}

// ---- 5. 리다이렉트 · 본문 상한 ----

// 검증: E14
#[tokio::test]
async fn 자료_요청은_리다이렉트를_따라가지_않고_키를_다른_곳으로_보내지_않는다() {
    for status in [301_u16, 302, 307, 308] {
        let server = MockServer::start().await;
        let elsewhere = mount_elsewhere(&server).await;
        mount_dart_list(
            &server,
            ResponseTemplate::new(status).insert_header("Location", elsewhere.as_str()),
        )
        .await;
        let adapter = client(&server, TestKeys::all(), TestClock::at(START));

        let err = adapter.fetch(&disclosures()).await.unwrap_err();

        assert_eq!(elsewhere_hits(&server).await, 0, "{status}");
        assert_eq!(hits(&server).await, 1, "{status}");
        assert!(!has_any_key(&format!("{err:?} {err}")), "{status}");
    }
}

// 검증: E14
#[tokio::test]
async fn 본문이_상한보다_크면_형식_오류이고_본문과_키를_싣지_않는다() {
    let server = MockServer::start().await;
    let mut big = vec![b' '; 17 * 1024 * 1024];
    big.extend_from_slice(FRED_KEY.as_bytes());
    Mock::given(method("GET"))
        .and(path("/fred/series/observations"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(big, "application/json"))
        .mount(&server)
        .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&fred_request()).await.unwrap_err();

    assert!(
        matches!(
            err,
            DataError::Format {
                origin: SourceName::Fred,
                ..
            }
        ),
        "{err:?}"
    );
    let text = format!("{err:?} {err}");
    assert!(!has_any_key(&text));
    assert!(text.len() < 1024);
}
