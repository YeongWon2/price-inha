//! DART 오픈API 어댑터. 로컬 가짜 서버(wiremock)만 쓴다.
//!
//! 가짜 응답의 모양은 DART 오픈API 개발가이드를 따른다:
//! - 공시검색 `GET /api/list.json` (요청 `crtfc_key` · `corp_code` · `bgn_de` · `end_de`,
//!   응답 `status` · `message` · `list[]{corp_cls, corp_name, corp_code, stock_code, report_nm, rcept_no, flr_nm, rcept_dt, rm}`)
//! - 단일회사 주요계정 `GET /api/fnlttSinglAcnt.json` (요청 `crtfc_key` · `corp_code` · `bsns_year` · `reprt_code`,
//!   응답 `list[]{rcept_no, bsns_year, stock_code, reprt_code, account_nm, fs_div, fs_nm, sj_div, sj_nm,
//!   thstrm_nm, thstrm_dt, thstrm_amount, .., ord, currency}`)
//! - 메시지: `000` 정상, `010` 등록되지 않은 키, `011` 사용할 수 없는 키, `013` 조회된 데이타 없음,
//!   `020` 요청 제한 초과, `100` 필드의 부적절한 값, `800` 시스템 점검.

mod common;

use common::*;
use pi_data::{
    CorpCode, DataAdapter, DataError, FactKind, FactRequest, FactValue, Fetched, FsDiv, ReportCode,
    SourceName,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CORP: &str = "00126380";
const LIST_PATH: &str = "/api/list.json";
const FIN_PATH: &str = "/api/fnlttSinglAcnt.json";

fn corp() -> CorpCode {
    CorpCode::parse(CORP).unwrap()
}

fn disclosures() -> FactRequest {
    FactRequest::Disclosures {
        corp: corp(),
        from: day("2026-11-01"),
        to: day("2026-11-15"),
    }
}

fn financials(fs: FsDiv) -> FactRequest {
    FactRequest::Financials {
        corp: corp(),
        year: 2025,
        report: ReportCode::Annual,
        fs,
    }
}

fn list_body() -> Value {
    json!({
        "status": "000",
        "message": "정상",
        "page_no": 1,
        "page_count": 10,
        "total_count": 2,
        "total_page": 1,
        "list": [
            {
                "corp_cls": "Y",
                "corp_name": "시험전자",
                "corp_code": CORP,
                "stock_code": "005930",
                "report_nm": "주요사항보고서(자기주식취득결정)",
                "rcept_no": "20261114000123",
                "flr_nm": "시험전자",
                "rcept_dt": "20261114",
                "rm": "유"
            },
            {
                "corp_cls": "Y",
                "corp_name": "시험전자",
                "corp_code": CORP,
                "stock_code": "005930",
                "report_nm": "분기보고서 (2026.09)",
                "rcept_no": "20261113000456",
                "flr_nm": "시험전자",
                "rcept_dt": "20261113",
                "rm": ""
            }
        ]
    })
}

fn fin_line(fs_div: &str, fs_nm: &str, account: &str, amount: &str) -> Value {
    json!({
        "rcept_no": "20260310000789",
        "reprt_code": "11011",
        "bsns_year": "2025",
        "corp_code": CORP,
        "stock_code": "005930",
        "fs_div": fs_div,
        "fs_nm": fs_nm,
        "sj_div": "IS",
        "sj_nm": "손익계산서",
        "account_nm": account,
        "thstrm_nm": "제 57 기",
        "thstrm_dt": "2025.01.01 ~ 2025.12.31",
        "thstrm_amount": amount,
        "frmtrm_nm": "제 56 기",
        "frmtrm_dt": "2024.01.01 ~ 2024.12.31",
        "frmtrm_amount": "1,000",
        "ord": "21",
        "currency": "KRW"
    })
}

fn fin_body() -> Value {
    json!({
        "status": "000",
        "message": "정상",
        "list": [
            fin_line("CFS", "연결재무제표", "매출액", "300,870,903,000,000"),
            fin_line("CFS", "연결재무제표", "영업이익", "32,725,961,000,000"),
            fin_line("OFS", "재무제표", "매출액", "209,052,241,000,000")
        ]
    })
}

fn status_body(status: &str, message: &str) -> Value {
    json!({ "status": status, "message": message })
}

async fn mount_list(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path(LIST_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

async fn mount_fin(server: &MockServer, body: Value) {
    Mock::given(method("GET"))
        .and(path(FIN_PATH))
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

// 검증: C11 V33 P10
#[tokio::test]
async fn 공시_목록은_요청_인자를_보내고_공시마다_출처와_접수일과_알게_된_시각을_단다() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LIST_PATH))
        .and(query_param("crtfc_key", DART_KEY))
        .and(query_param("corp_code", CORP))
        .and(query_param("bgn_de", "20261101"))
        .and(query_param("end_de", "20261115"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list_body()))
        .expect(1)
        .mount(&server)
        .await;
    let clock = TestClock::at(START);
    let adapter = client(&server, TestKeys::all(), clock.clone());

    let list = facts(adapter.fetch(&disclosures()).await.unwrap());

    assert_eq!(list.len(), 2);
    let first = &list[0];
    assert_eq!(first.kind, FactKind::Disclosure);
    assert_eq!(first.subject, CORP);
    assert_eq!(first.label, "시험전자");
    assert_eq!(
        first.value,
        FactValue::Text("주요사항보고서(자기주식취득결정)".to_owned())
    );
    assert_eq!(first.reference.as_deref(), Some("20261114000123"));
    assert_eq!(first.published_on, Some(day("2026-11-14")));
    assert_eq!(first.period, None);
    assert_eq!(first.source.name, SourceName::Dart);
    assert!(first.source.url.starts_with(&server.uri()));
    assert!(first.source.url.contains(LIST_PATH));
    assert_eq!(first.known_at, ts(START));
    assert_eq!(list[1].published_on, Some(day("2026-11-13")));
    assert_eq!(list[1].known_at, ts(START));
}

// 검증: C11 V33 P10
#[tokio::test]
async fn 재무는_고른_재무제표_구분의_계정만_수로_돌려준다() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FIN_PATH))
        .and(query_param("crtfc_key", DART_KEY))
        .and(query_param("corp_code", CORP))
        .and(query_param("bsns_year", "2025"))
        .and(query_param("reprt_code", "11011"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fin_body()))
        .expect(1)
        .mount(&server)
        .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let list = facts(
        adapter
            .fetch(&financials(FsDiv::Consolidated))
            .await
            .unwrap(),
    );

    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|fact| fact.kind == FactKind::Financials));
    assert_eq!(list[0].label, "매출액");
    assert_eq!(list[0].value, FactValue::Number(dec("300870903000000")));
    assert_eq!(list[1].label, "영업이익");
    assert_eq!(list[1].value, FactValue::Number(dec("32725961000000")));
    assert_eq!(list[0].unit.as_deref(), Some("KRW"));
    assert_eq!(list[0].period.as_deref(), Some("2025.01.01 ~ 2025.12.31"));
    assert_eq!(list[0].reference.as_deref(), Some("20260310000789"));
    assert_eq!(list[0].source.name, SourceName::Dart);
    assert!(list[0].source.url.contains(FIN_PATH));
    assert_eq!(list[0].known_at, ts(START));
}

// 검증: C11 V33
#[tokio::test]
async fn 개별_재무제표를_고르면_개별_계정만_온다() {
    let server = MockServer::start().await;
    mount_fin(&server, fin_body()).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let list = facts(adapter.fetch(&financials(FsDiv::Separate)).await.unwrap());

    assert_eq!(list.len(), 1);
    assert_eq!(list[0].label, "매출액");
    assert_eq!(list[0].value, FactValue::Number(dec("209052241000000")));
}

// 검증: C11
#[test]
fn 보고서_코드와_재무제표_구분은_dart의_글자다() {
    assert_eq!(ReportCode::FirstQuarter.code(), "11013");
    assert_eq!(ReportCode::HalfYear.code(), "11012");
    assert_eq!(ReportCode::ThirdQuarter.code(), "11014");
    assert_eq!(ReportCode::Annual.code(), "11011");
    assert_eq!(FsDiv::Consolidated.code(), "CFS");
    assert_eq!(FsDiv::Separate.code(), "OFS");
}

// 검증: E32
#[tokio::test]
async fn 조회된_데이타가_없으면_오류가_아니라_비어_있음이다() {
    let server = MockServer::start().await;
    mount_list(&server, status_body("013", "조회된 데이타가 없습니다.")).await;
    mount_fin(&server, status_body("013", "조회된 데이타가 없습니다.")).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    assert_eq!(
        adapter.fetch(&disclosures()).await.unwrap(),
        Fetched::Missing(FactKind::Disclosure)
    );
    assert_eq!(
        adapter
            .fetch(&financials(FsDiv::Consolidated))
            .await
            .unwrap(),
        Fetched::Missing(FactKind::Financials)
    );
}

// 검증: E32
#[tokio::test]
async fn 정상인데_목록이_비었으면_비어_있음이다() {
    let server = MockServer::start().await;
    mount_list(
        &server,
        json!({
            "status": "000", "message": "정상",
            "page_no": 1, "page_count": 10, "total_count": 0, "total_page": 0,
            "list": []
        }),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    assert_eq!(
        adapter.fetch(&disclosures()).await.unwrap(),
        Fetched::Missing(FactKind::Disclosure)
    );
}

// 검증: E32
#[tokio::test]
async fn 고른_재무제표_구분의_계정이_하나도_없으면_비어_있음이다() {
    let server = MockServer::start().await;
    mount_fin(
        &server,
        json!({
            "status": "000", "message": "정상",
            "list": [fin_line("OFS", "재무제표", "매출액", "1,000")]
        }),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    assert_eq!(
        adapter
            .fetch(&financials(FsDiv::Consolidated))
            .await
            .unwrap(),
        Fetched::Missing(FactKind::Financials)
    );
}

// 검증: E14
#[tokio::test]
async fn 요청_제한_초과는_한도_오류다() {
    let server = MockServer::start().await;
    mount_list(&server, status_body("020", "요청 제한을 초과하였습니다.")).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&disclosures()).await.unwrap_err();

    assert_eq!(
        err,
        DataError::RateLimited {
            origin: SourceName::Dart
        }
    );
}

// 검증: E14
#[tokio::test]
async fn 등록되지_않은_키와_쓸_수_없는_키는_키_거절이다() {
    for code in ["010", "011"] {
        let server = MockServer::start().await;
        mount_list(&server, status_body(code, "키 오류")).await;
        let adapter = client(&server, TestKeys::all(), TestClock::at(START));

        let err = adapter.fetch(&disclosures()).await.unwrap_err();

        assert_eq!(
            err,
            DataError::KeyRejected {
                origin: SourceName::Dart,
                code: code.to_owned()
            }
        );
        assert!(!has_any_key(&format!("{err} {err:?}")));
    }
}

// 검증: E14
#[tokio::test]
async fn 시스템_점검은_닿지_않음이고_주소에_키가_없다() {
    let server = MockServer::start().await;
    mount_list(
        &server,
        status_body("800", "시스템 점검으로 인한 서비스가 중지 중입니다."),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&disclosures()).await.unwrap_err();

    match &err {
        DataError::Unreachable { origin, url, .. } => {
            assert_eq!(*origin, SourceName::Dart);
            assert!(url.contains(LIST_PATH));
            assert!(!url.contains(DART_KEY));
        }
        other => panic!("닿지 않음이어야 한다: {other:?}"),
    }
    assert!(!has_any_key(&format!("{err} {err:?}")));
}

// 검증: E14
#[tokio::test]
async fn 필드의_부적절한_값은_거절이다() {
    let server = MockServer::start().await;
    mount_list(&server, status_body("100", "필드의 부적절한 값입니다.")).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&disclosures()).await.unwrap_err();

    assert_eq!(
        err,
        DataError::Rejected {
            origin: SourceName::Dart,
            code: "100".to_owned()
        }
    );
}

// 검증: E14
#[tokio::test]
async fn 금액이_수가_아니면_형식_오류다() {
    let server = MockServer::start().await;
    mount_fin(
        &server,
        json!({
            "status": "000", "message": "정상",
            "list": [fin_line("CFS", "연결재무제표", "매출액", "수가-아님")]
        }),
    )
    .await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter
        .fetch(&financials(FsDiv::Consolidated))
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            DataError::Format {
                origin: SourceName::Dart,
                ..
            }
        ),
        "{err:?}"
    );
}

// 검증: E14
#[tokio::test]
async fn 상태_칸이_없는_본문은_형식_오류다() {
    let server = MockServer::start().await;
    mount_list(&server, json!({ "뭔가": "다른 모양" })).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter.fetch(&disclosures()).await.unwrap_err();

    assert!(
        matches!(
            err,
            DataError::Format {
                origin: SourceName::Dart,
                ..
            }
        ),
        "{err:?}"
    );
}

// 검증: C11 E14
#[test]
fn 고유번호는_여덟_자리_숫자만_받는다() {
    for bad in [
        "",
        "0012638",
        "001263800",
        "0012638a",
        "005930",
        "００１２６３８０",
    ] {
        assert!(
            matches!(CorpCode::parse(bad), Err(DataError::InvalidRequest { .. })),
            "{bad:?}"
        );
    }
    assert_eq!(CorpCode::parse(CORP).unwrap().as_str(), CORP);
}

// 검증: E14
#[tokio::test]
async fn 시작일이_끝일보다_늦으면_요청을_보내지_않는다() {
    let server = MockServer::start().await;
    mount_list(&server, list_body()).await;
    let adapter = client(&server, TestKeys::all(), TestClock::at(START));

    let err = adapter
        .fetch(&FactRequest::Disclosures {
            corp: corp(),
            from: day("2026-11-15"),
            to: day("2026-11-01"),
        })
        .await
        .unwrap_err();

    assert!(matches!(err, DataError::InvalidRequest { .. }), "{err:?}");
    assert_eq!(hits(&server).await, 0);
}
