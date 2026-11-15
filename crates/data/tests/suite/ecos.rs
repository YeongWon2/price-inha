//! 한국은행 ECOS 어댑터. 로컬 가짜 서버(wiremock)만 쓴다.
//!
//! 가짜 응답의 모양은 ECOS Open API `StatisticSearch`를 따른다(공식 서비스의 sample 키 응답으로 확인):
//! - 요청 `GET /api/StatisticSearch/{인증키}/json/kr/{시작행}/{끝행}/{통계표코드}/{주기}/{시작}/{끝}/{항목코드1}`
//! - 정상 `{"StatisticSearch":{"list_total_count":N,"row":[{STAT_CODE, STAT_NAME, ITEM_CODE1, ITEM_NAME1, .., UNIT_NAME, WGT, TIME, DATA_VALUE}]}}`
//! - 없음 `{"RESULT":{"CODE":"INFO-200","MESSAGE":"해당하는 데이터가 없습니다."}}`
//! - 키 무효 `{"RESULT":{"CODE":"INFO-100","MESSAGE":"인증키가 유효하지 않습니다. .."}}`
//!
//! 키가 주소 경로에 실리므로 오류 · 출처 주소에서 가려져야 한다.

use crate::common::*;
use pi_data::{
    DataAdapter, DataError, EcosCycle, EcosSeries, FactKind, FactRequest, FactValue, Fetched,
    SourceName,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const RATE_STAT: &str = "722Y001";
const RATE_ITEM: &str = "0101000";
const FX_STAT: &str = "731Y001";
const FX_ITEM: &str = "0000001";

fn rate_series() -> EcosSeries {
    EcosSeries {
        stat_code: RATE_STAT.to_owned(),
        item_code: RATE_ITEM.to_owned(),
        cycle: EcosCycle::Month,
        start: "202609".to_owned(),
        end: "202610".to_owned(),
    }
}

fn fx_series() -> EcosSeries {
    EcosSeries {
        stat_code: FX_STAT.to_owned(),
        item_code: FX_ITEM.to_owned(),
        cycle: EcosCycle::Day,
        start: "20261113".to_owned(),
        end: "20261113".to_owned(),
    }
}

fn row(
    stat: &str,
    stat_name: &str,
    item: &str,
    item_name: &str,
    unit: &str,
    time: &str,
    value: &str,
) -> Value {
    json!({
        "STAT_CODE": stat,
        "STAT_NAME": stat_name,
        "ITEM_CODE1": item,
        "ITEM_NAME1": item_name,
        "ITEM_CODE2": null, "ITEM_NAME2": null,
        "ITEM_CODE3": null, "ITEM_NAME3": null,
        "ITEM_CODE4": null, "ITEM_NAME4": null,
        "UNIT_NAME": unit,
        "WGT": null,
        "TIME": time,
        "DATA_VALUE": value
    })
}

fn rate_body() -> Value {
    json!({
        "StatisticSearch": {
            "list_total_count": 2,
            "row": [
                row(RATE_STAT, "1.3.1. 한국은행 기준금리 및 여수신금리", RATE_ITEM, "한국은행 기준금리", "연%", "202609", "2.5"),
                row(RATE_STAT, "1.3.1. 한국은행 기준금리 및 여수신금리", RATE_ITEM, "한국은행 기준금리", "연%", "202610", "2.25")
            ]
        }
    })
}

fn fx_body() -> Value {
    json!({
        "StatisticSearch": {
            "list_total_count": 1,
            "row": [
                row(FX_STAT, "3.1.1.1. 주요국 통화의 대원화환율", FX_ITEM, "원/미국달러(매매기준율)", "원", "20261113", "1391.5")
            ]
        }
    })
}

fn result_body(code: &str, message: &str) -> Value {
    json!({ "RESULT": { "CODE": code, "MESSAGE": message } })
}

/// 키 · 통계표 · 주기 · 기간 · 항목이 제 자리에 있는 경로.
fn ecos_path(key: &str, stat: &str, cycle: &str, start: &str, end: &str, item: &str) -> String {
    format!("^/api/StatisticSearch/{key}/json/kr/1/[0-9]+/{stat}/{cycle}/{start}/{end}/{item}$")
}

async fn mount_any(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path_regex("^/api/StatisticSearch/.*$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
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
async fn 금리는_경로에_키와_통계표를_싣고_시점마다_수와_단위를_돌려준다() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(ecos_path(
            ECOS_KEY, RATE_STAT, "M", "202609", "202610", RATE_ITEM,
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(rate_body()))
        .expect(1)
        .mount(&server)
        .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let list = facts(
        adapter
            .fetch(&FactRequest::KoreaRate(rate_series()))
            .await
            .unwrap(),
    );

    assert_eq!(list.len(), 2);
    assert_eq!(list[0].kind, FactKind::KoreaRate);
    assert_eq!(list[0].label, "한국은행 기준금리");
    assert_eq!(list[0].period.as_deref(), Some("202609"));
    assert_eq!(list[0].value, FactValue::Number(dec("2.5")));
    assert_eq!(list[1].period.as_deref(), Some("202610"));
    assert_eq!(list[1].value, FactValue::Number(dec("2.25")));
    assert_eq!(list[0].unit.as_deref(), Some("연%"));
    assert_eq!(list[0].source.name, SourceName::Ecos);
    assert!(list[0].source.url.starts_with(&server.uri()));
    assert!(list[0].source.url.contains("/api/StatisticSearch/"));
    assert_eq!(list[0].known_at, ts(START));
    assert_eq!(list[1].known_at, ts(START));
}

// 검증: C11 V34
#[tokio::test]
async fn 환율도_같은_모양으로_받는다() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(ecos_path(
            ECOS_KEY, FX_STAT, "D", "20261113", "20261113", FX_ITEM,
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fx_body()))
        .expect(1)
        .mount(&server)
        .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let list = facts(
        adapter
            .fetch(&FactRequest::KoreaFx(fx_series()))
            .await
            .unwrap(),
    );

    assert_eq!(list.len(), 1);
    assert_eq!(list[0].kind, FactKind::KoreaFx);
    assert_eq!(list[0].label, "원/미국달러(매매기준율)");
    assert_eq!(list[0].value, FactValue::Number(dec("1391.5")));
    assert_eq!(list[0].unit.as_deref(), Some("원"));
    assert_eq!(list[0].period.as_deref(), Some("20261113"));
}

// 검증: V34 E14
#[tokio::test]
async fn 경로에_실린_키는_출처_주소에서_가려진다() {
    let server = MockServer::start().await;
    mount_any(&server, rate_body()).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let list = facts(
        adapter
            .fetch(&FactRequest::KoreaRate(rate_series()))
            .await
            .unwrap(),
    );

    for fact in &list {
        assert!(!fact.source.url.contains(ECOS_KEY));
        assert!(!has_any_key(&format!("{fact:?}")));
        assert!(fact.source.url.contains(RATE_STAT));
    }
}

// 검증: C11
#[test]
fn 주기는_ecos의_글자다() {
    assert_eq!(EcosCycle::Annual.code(), "A");
    assert_eq!(EcosCycle::Quarter.code(), "Q");
    assert_eq!(EcosCycle::Month.code(), "M");
    assert_eq!(EcosCycle::Day.code(), "D");
}

// 검증: E32
#[tokio::test]
async fn 해당하는_데이터가_없으면_비어_있음이다() {
    let server = MockServer::start().await;
    mount_any(
        &server,
        result_body("INFO-200", "해당하는 데이터가 없습니다."),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    assert_eq!(
        adapter
            .fetch(&FactRequest::KoreaRate(rate_series()))
            .await
            .unwrap(),
        Fetched::Missing(FactKind::KoreaRate)
    );
    assert_eq!(
        adapter
            .fetch(&FactRequest::KoreaFx(fx_series()))
            .await
            .unwrap(),
        Fetched::Missing(FactKind::KoreaFx)
    );
}

// 검증: E14
#[tokio::test]
async fn 인증키가_유효하지_않으면_키_거절이다() {
    let server = MockServer::start().await;
    mount_any(
        &server,
        result_body(
            "INFO-100",
            "인증키가 유효하지 않습니다. 인증키를 확인하십시오! 인증키가 없는 경우 인증키를 신청하십시오!",
        ),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter
        .fetch(&FactRequest::KoreaRate(rate_series()))
        .await
        .unwrap_err();

    assert_eq!(
        err,
        DataError::KeyRejected {
            origin: SourceName::Ecos,
            code: "INFO-100".to_owned()
        }
    );
    assert!(!has_any_key(&format!("{err} {err:?}")));
}

// 검증: E14
#[tokio::test]
async fn 서버_오류면_닿지_않음이고_주소에서_경로의_키가_가려진다() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/StatisticSearch/.*$"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter
        .fetch(&FactRequest::KoreaRate(rate_series()))
        .await
        .unwrap_err();

    match &err {
        DataError::Unreachable { origin, url, .. } => {
            assert_eq!(*origin, SourceName::Ecos);
            assert!(url.contains("/api/StatisticSearch/"));
            assert!(url.contains(RATE_STAT));
            assert!(!url.contains(ECOS_KEY));
        }
        other => panic!("닿지 않음이어야 한다: {other:?}"),
    }
    assert!(!has_any_key(&format!("{err} {err:?}")));
}

// 검증: E14
#[tokio::test]
async fn 값이_수가_아니면_형식_오류다() {
    let server = MockServer::start().await;
    mount_any(
        &server,
        json!({
            "StatisticSearch": {
                "list_total_count": 1,
                "row": [row(RATE_STAT, "표", RATE_ITEM, "한국은행 기준금리", "연%", "202609", "수가-아님")]
            }
        }),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter
        .fetch(&FactRequest::KoreaRate(rate_series()))
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            DataError::Format {
                origin: SourceName::Ecos,
                ..
            }
        ),
        "{err:?}"
    );
    assert!(!has_any_key(&format!("{err} {err:?}")));
}

// 검증: E14
#[tokio::test]
async fn 아는_모양이_아닌_본문은_형식_오류다() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/StatisticSearch/.*$"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>점검</html>"))
        .mount(&server)
        .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter
        .fetch(&FactRequest::KoreaRate(rate_series()))
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            DataError::Format {
                origin: SourceName::Ecos,
                ..
            }
        ),
        "{err:?}"
    );
}
