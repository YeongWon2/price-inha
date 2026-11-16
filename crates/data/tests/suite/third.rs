//! 3차 작업(후보 일괄 처리, `docs/dev/modules/data.md` 3차 후보 1~2번). 로컬 가짜 서버(wiremock)만 쓴다.
//!
//! 1. ECOS `list_total_count`가 없거나 숫자가 아니면, 받은 행 수가 상한과 같을 때 잘림으로 본다.
//! 2. 다시 해도 안 될 실패(404 · 3xx 주소 오류, 거절, 형식 오류 …)는 [`DataError::is_retryable`]이 `false`다.

use crate::common::*;
use pi_data::{
    CorpCode, DataAdapter, DataError, EcosCycle, EcosSeries, FactRequest, Fetched, SourceName,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// ECOS 한 번에 받는 행 수의 상한.
const ECOS_MAX_ROWS: usize = 10_000;

fn rate_request() -> FactRequest {
    FactRequest::KoreaRate(EcosSeries {
        stat_code: "722Y001".to_owned(),
        item_code: "0101000".to_owned(),
        cycle: EcosCycle::Month,
        start: "199901".to_owned(),
        end: "202610".to_owned(),
    })
}

fn requests() -> [(SourceName, FactRequest); 3] {
    [
        (
            SourceName::Dart,
            FactRequest::Disclosures {
                corp: CorpCode::parse("00126380").unwrap(),
                from: day("2026-11-01"),
                to: day("2026-11-15"),
            },
        ),
        (SourceName::Ecos, rate_request()),
        (
            SourceName::Fred,
            FactRequest::UsRate {
                series_id: "DGS10".to_owned(),
                from: day("2026-11-12"),
                to: day("2026-11-13"),
            },
        ),
    ]
}

/// ECOS 응답: 행 `rows`개. `total`이 `None`이면 `list_total_count` 칸이 없다.
fn ecos_body(rows: usize, total: Option<Value>) -> Value {
    let row = json!({
        "STAT_CODE": "722Y001",
        "STAT_NAME": "1.3.1. 한국은행 기준금리 및 여수신금리",
        "ITEM_CODE1": "0101000",
        "ITEM_NAME1": "한국은행 기준금리",
        "ITEM_CODE2": null, "ITEM_NAME2": null,
        "ITEM_CODE3": null, "ITEM_NAME3": null,
        "ITEM_CODE4": null, "ITEM_NAME4": null,
        "UNIT_NAME": "연%",
        "WGT": null,
        "TIME": "202610",
        "DATA_VALUE": "2.25"
    });
    let mut table = json!({ "row": vec![row; rows] });
    if let Some(total) = total {
        table["list_total_count"] = total;
    }
    json!({ "StatisticSearch": table })
}

async fn mount_ecos(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path_regex("^/api/StatisticSearch/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

async fn mount_all(server: &MockServer, template: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path_regex("^/(api|fred)/"))
        .respond_with(template)
        .mount(server)
        .await;
}

async fn ecos_fetch(body: Value) -> Result<Fetched, DataError> {
    let server = MockServer::start().await;
    mount_ecos(&server, body).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));
    adapter.fetch(&rate_request()).await
}

// 검증: C11 E14
#[tokio::test]
async fn ecos_전체_행_수가_없거나_숫자가_아니어도_행이_상한과_같으면_잘림이다() {
    let unknown: [Option<Value>; 4] = [
        None,
        Some(json!("많음")),
        Some(json!(null)),
        Some(json!(-1)),
    ];
    for total in unknown {
        let err = ecos_fetch(ecos_body(ECOS_MAX_ROWS, total.clone()))
            .await
            .unwrap_err();
        assert_eq!(
            err,
            DataError::Truncated {
                origin: SourceName::Ecos
            },
            "{total:?}"
        );
        assert!(!has_any_key(&format!("{err:?} {err}")));
    }
}

// 검증: C11
#[tokio::test]
async fn ecos_전체_행_수를_몰라도_행이_상한보다_적으면_모두_돌려준다() {
    for total in [None, Some(json!("많음")), Some(json!(null))] {
        let got = ecos_fetch(ecos_body(ECOS_MAX_ROWS - 1, total.clone()))
            .await
            .unwrap();
        match got {
            Fetched::Facts(list) => assert_eq!(list.len(), ECOS_MAX_ROWS - 1, "{total:?}"),
            other => panic!("자료가 와야 한다: {other:?}"),
        }
    }
}

// 검증: C11
#[tokio::test]
async fn ecos_전체_행_수를_알려_주면_그_수를_따른다() {
    let got = ecos_fetch(ecos_body(ECOS_MAX_ROWS, Some(json!(ECOS_MAX_ROWS))))
        .await
        .unwrap();
    assert!(matches!(got, Fetched::Facts(list) if list.len() == ECOS_MAX_ROWS));
}

// 검증: E14
#[tokio::test]
async fn 주소_오류_404와_3xx는_다시_시도_대상이_아니다() {
    for status in [404_u16, 301, 302, 307, 308] {
        for (source, request) in requests() {
            let server = MockServer::start().await;
            mount_all(&server, ResponseTemplate::new(status)).await;
            let adapter = client(&server, TestKeys::all(), TestClock::at(START));

            let err = adapter.fetch(&request).await.unwrap_err();

            assert!(!err.is_retryable(), "{status} {source}: {err:?}");
        }
    }
}

// 검증: E14
#[tokio::test]
async fn 서버_오류와_한도와_시간_초과는_다시_시도_대상이다() {
    for status in [500_u16, 502, 503, 504, 429] {
        for (source, request) in requests() {
            let server = MockServer::start().await;
            mount_all(&server, ResponseTemplate::new(status)).await;
            let adapter = client(&server, TestKeys::all(), TestClock::at(START));

            let err = adapter.fetch(&request).await.unwrap_err();

            assert!(err.is_retryable(), "{status} {source}: {err:?}");
        }
    }
    let slow = MockServer::start().await;
    mount_all(
        &slow,
        ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(3)),
    )
    .await;
    let config = config(&slow).with_timeout(std::time::Duration::from_millis(200));
    let adapter = pi_data::DataClient::new(config, TestKeys::all(), TestClock::at(START)).unwrap();
    let err = adapter.fetch(&rate_request()).await.unwrap_err();
    assert!(matches!(err, DataError::Timeout { .. }), "{err:?}");
    assert!(err.is_retryable());
}

// 검증: E14
#[test]
fn 키와_요청과_형식과_잘림_같은_실패는_다시_해도_같아서_시도_대상이_아니다() {
    let origin = SourceName::Ecos;
    let url = "https://example.invalid/api".to_owned();
    let never = [
        DataError::NoKey { origin },
        DataError::KeyRejected {
            origin,
            code: "INFO-100".to_owned(),
        },
        DataError::Rejected {
            origin,
            code: "400".to_owned(),
        },
        DataError::Format {
            origin,
            detail: "JSON이 아닙니다".to_owned(),
        },
        DataError::InvalidRequest {
            detail: "기간 순서".to_owned(),
        },
        DataError::InsecureBaseUrl { origin },
        DataError::Truncated { origin },
        DataError::KeyUnreadable { origin },
    ];
    for err in never {
        assert!(!err.is_retryable(), "{err:?}");
    }

    let retry = [
        DataError::RateLimited { origin },
        DataError::Timeout {
            origin,
            url: url.clone(),
        },
        DataError::Unreachable {
            origin,
            url: url.clone(),
            detail: "연결 실패".to_owned(),
        },
        DataError::Unreachable {
            origin,
            url: url.clone(),
            detail: "HTTP 503".to_owned(),
        },
    ];
    for err in retry {
        assert!(err.is_retryable(), "{err:?}");
    }
    let address = [
        DataError::Unreachable {
            origin,
            url: url.clone(),
            detail: "HTTP 404".to_owned(),
        },
        DataError::Unreachable {
            origin,
            url,
            detail: "HTTP 301".to_owned(),
        },
    ];
    for err in address {
        assert!(!err.is_retryable(), "{err:?}");
    }
}
