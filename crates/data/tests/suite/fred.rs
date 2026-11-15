//! FRED 어댑터. 로컬 가짜 서버(wiremock)만 쓴다.
//!
//! 가짜 응답의 모양은 FRED API `fred/series/observations` 공식 문서를 따른다:
//! - 요청 `GET /fred/series/observations?series_id=..&api_key=..&file_type=json&observation_start=YYYY-MM-DD&observation_end=YYYY-MM-DD`
//! - 응답 `{realtime_start, realtime_end, observation_start, observation_end, units, output_type, file_type,
//!   order_by, sort_order, count, offset, limit, observations:[{realtime_start, realtime_end, date, value}]}`
//! - 오류 `{"error_code":400,"error_message":"Bad Request. .."}` (HTTP 400)
//!
//! 키가 쿼리(`api_key`)에 실리므로 오류 · 출처 주소에서 가려져야 한다.

use crate::common::*;
use pi_data::{DataAdapter, DataError, FactKind, FactRequest, FactValue, Fetched, SourceName};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OBS_PATH: &str = "/fred/series/observations";
const SERIES: &str = "DGS10";

fn request() -> FactRequest {
    FactRequest::UsRate {
        series_id: SERIES.to_owned(),
        from: day("2026-11-12"),
        to: day("2026-11-13"),
    }
}

fn obs(date: &str, value: &str) -> Value {
    json!({
        "realtime_start": "2026-11-14",
        "realtime_end": "2026-11-14",
        "date": date,
        "value": value
    })
}

fn body(observations: Vec<Value>) -> Value {
    json!({
        "realtime_start": "2026-11-14",
        "realtime_end": "2026-11-14",
        "observation_start": "2026-11-12",
        "observation_end": "2026-11-13",
        "units": "lin",
        "output_type": 1,
        "file_type": "json",
        "order_by": "observation_date",
        "sort_order": "asc",
        "count": observations.len(),
        "offset": 0,
        "limit": 100000,
        "observations": observations
    })
}

async fn mount(server: &MockServer, template: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(OBS_PATH))
        .respond_with(template)
        .mount(server)
        .await;
}

fn facts(fetched: Fetched) -> Vec<pi_data::Fact> {
    match fetched {
        Fetched::Facts(list) => list,
        other => panic!("자료가 와야 한다: {other:?}"),
    }
}

// 검증: C11 V34 P10
#[tokio::test]
async fn 미국_금리는_요청_인자를_보내고_관측일마다_수와_공표일을_돌려준다() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(OBS_PATH))
        .and(query_param("series_id", SERIES))
        .and(query_param("api_key", FRED_KEY))
        .and(query_param("file_type", "json"))
        .and(query_param("observation_start", "2026-11-12"))
        .and(query_param("observation_end", "2026-11-13"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body(vec![
            obs("2026-11-12", "4.12"),
            obs("2026-11-13", "4.09"),
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let list = facts(adapter.fetch(&request()).await.unwrap());

    assert_eq!(list.len(), 2);
    assert_eq!(list[0].kind, FactKind::UsRate);
    assert_eq!(list[0].subject, SERIES);
    assert_eq!(list[0].period.as_deref(), Some("2026-11-12"));
    assert_eq!(list[0].value, FactValue::Number(dec("4.12")));
    assert_eq!(list[1].period.as_deref(), Some("2026-11-13"));
    assert_eq!(list[1].value, FactValue::Number(dec("4.09")));
    assert_eq!(list[0].published_on, Some(day("2026-11-14")));
    assert_eq!(list[0].source.name, SourceName::Fred);
    assert!(list[0].source.url.starts_with(&server.uri()));
    assert!(list[0].source.url.contains(OBS_PATH));
    assert!(!list[0].source.url.contains(FRED_KEY));
    assert!(!has_any_key(&format!("{list:?}")));
    assert_eq!(list[0].known_at, ts(START));
}

// 검증: E32
#[tokio::test]
async fn 관측값이_없으면_비어_있음이다() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_json(body(vec![])),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    assert_eq!(
        adapter.fetch(&request()).await.unwrap(),
        Fetched::Missing(FactKind::UsRate)
    );
}

// 검증: E14
#[tokio::test]
async fn 잘못된_요청은_거절이고_오류에_키가_없다() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(400).set_body_json(json!({
            "error_code": 400,
            "error_message": "Bad Request.  The series does not exist."
        })),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&request()).await.unwrap_err();

    assert_eq!(
        err,
        DataError::Rejected {
            origin: SourceName::Fred,
            code: "400".to_owned()
        }
    );
    assert!(!has_any_key(&format!("{err} {err:?}")));
}

// 검증: E14
#[tokio::test]
async fn http_429는_한도_오류다() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(429)).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&request()).await.unwrap_err();

    assert_eq!(
        err,
        DataError::RateLimited {
            origin: SourceName::Fred
        }
    );
}

// 검증: E14
#[tokio::test]
async fn 서버_오류면_닿지_않음이고_주소의_쿼리_키가_가려진다() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(500)).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&request()).await.unwrap_err();

    match &err {
        DataError::Unreachable { origin, url, .. } => {
            assert_eq!(*origin, SourceName::Fred);
            assert!(url.contains(OBS_PATH));
            assert!(url.contains(SERIES));
            assert!(!url.contains(FRED_KEY));
        }
        other => panic!("닿지 않음이어야 한다: {other:?}"),
    }
    assert!(!has_any_key(&format!("{err} {err:?}")));
}

// 검증: E14
#[tokio::test]
async fn 관측값이_수가_아닌_글이면_형식_오류다() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_json(body(vec![obs("2026-11-12", "수가-아님")])),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&request()).await.unwrap_err();

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
}

// 검증: E14
#[tokio::test]
async fn 관측값_칸이_없으면_형식_오류다() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_json(json!({ "count": 0 })),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&request()).await.unwrap_err();

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
}

// 검증: E14
#[tokio::test]
async fn 시작일이_끝일보다_늦으면_요청을_보내지_않는다() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_json(body(vec![])),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter
        .fetch(&FactRequest::UsRate {
            series_id: SERIES.to_owned(),
            from: day("2026-11-13"),
            to: day("2026-11-12"),
        })
        .await
        .unwrap_err();

    assert!(matches!(err, DataError::InvalidRequest { .. }), "{err:?}");
    assert_eq!(hits(&server).await, 0);
}
