//! 가짜 자료 어댑터: 정한 Fact와 오류를 돌려주고 받은 요청을 적는다.

mod common;

use common::*;
use pi_data::{
    CorpCode, DataAdapter, DataError, Fact, FactKind, FactRequest, FactValue, FakeDataAdapter,
    Fetched, Source, SourceName,
};

fn fred_request() -> FactRequest {
    FactRequest::UsRate {
        series_id: "DGS10".to_owned(),
        from: day("2026-11-12"),
        to: day("2026-11-13"),
    }
}

fn disclosures() -> FactRequest {
    FactRequest::Disclosures {
        corp: CorpCode::parse("00126380").unwrap(),
        from: day("2026-11-01"),
        to: day("2026-11-15"),
    }
}

fn fact() -> Fact {
    Fact {
        kind: FactKind::UsRate,
        subject: "DGS10".to_owned(),
        label: "DGS10".to_owned(),
        period: Some("2026-11-13".to_owned()),
        value: FactValue::Number(dec("4.09")),
        unit: None,
        reference: None,
        source: Source {
            name: SourceName::Fred,
            url: "http://127.0.0.1:9/fred/series/observations?series_id=DGS10".to_owned(),
        },
        published_on: Some(day("2026-11-14")),
        known_at: ts(START),
    }
}

// 검증: C11
#[tokio::test]
async fn 정한_자료를_그대로_돌려주고_요청을_적는다() {
    let fake = FakeDataAdapter::new();
    fake.set(FactKind::UsRate, Ok(Fetched::Facts(vec![fact()])));

    let got = fake.fetch(&fred_request()).await.unwrap();

    assert_eq!(got, Fetched::Facts(vec![fact()]));
    assert_eq!(fake.calls(), vec![fred_request()]);
}

// 검증: E14
#[tokio::test]
async fn 정한_오류를_돌려준다() {
    let fake = FakeDataAdapter::new();
    fake.set(
        FactKind::Disclosure,
        Err(DataError::RateLimited {
            origin: SourceName::Dart,
        }),
    );

    assert_eq!(
        fake.fetch(&disclosures()).await.unwrap_err(),
        DataError::RateLimited {
            origin: SourceName::Dart
        }
    );
}

// 검증: E32
#[tokio::test]
async fn 정하지_않은_종류는_비어_있음이다() {
    let fake = FakeDataAdapter::new();
    fake.set(FactKind::UsRate, Ok(Fetched::Facts(vec![fact()])));

    assert_eq!(
        fake.fetch(&disclosures()).await.unwrap(),
        Fetched::Missing(FactKind::Disclosure)
    );
    assert_eq!(fake.calls(), vec![disclosures()]);
}

// 검증: C11
#[tokio::test]
async fn 다시_정하면_바뀐_결과를_돌려주고_요청은_차례대로_쌓인다() {
    let fake = FakeDataAdapter::new();
    fake.set(FactKind::UsRate, Ok(Fetched::Facts(vec![fact()])));
    let first = fake.fetch(&fred_request()).await.unwrap();
    fake.set(FactKind::UsRate, Ok(Fetched::Missing(FactKind::UsRate)));
    let second = fake.fetch(&fred_request()).await.unwrap();

    assert_eq!(first, Fetched::Facts(vec![fact()]));
    assert_eq!(second, Fetched::Missing(FactKind::UsRate));
    assert_eq!(fake.calls(), vec![fred_request(), fred_request()]);
}

// 검증: V33 V34 V36 P10
#[tokio::test]
async fn 가짜가_준_자료도_출처와_알게_된_시각을_그대로_가진다() {
    let fake = FakeDataAdapter::new();
    fake.set(FactKind::UsRate, Ok(Fetched::Facts(vec![fact()])));

    let list = match fake.fetch(&fred_request()).await.unwrap() {
        Fetched::Facts(list) => list,
        other => panic!("{other:?}"),
    };

    assert_eq!(list[0].source.name, SourceName::Fred);
    assert_eq!(list[0].known_at, ts(START));
    assert!(list[0].known_by(ts(START)));
}
