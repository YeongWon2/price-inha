//! 자료 잇기(T10): 종목 하나의 판단 자료를 받아 저장하고 논의 자료(`pi_agent::DataItem`)로 옮긴다.
//!
//! 받는 것: 공시 · 재무(DART, 고유번호가 있는 종목만), 국내 금리 · 환율(ECOS), (10b) 미국 금리(FRED, 키가 있을 때). 받은 자료는 저장소에 쌓고
//! (`Store::save_facts`), 판단된 뒤에 매매에 붙인다. 비었거나(`Missing`) 받지 못한 자료는 내용이 없는
//! 자료 항목이 된다(실행기가 지켜보기로 둔다, E32).

use chrono::{DateTime, Datelike, Duration as Days, NaiveDate, Utc};
use pi_agent::DataItem;
use pi_data::{
    DataAdapter, DataError, EcosCycle, EcosSeries, Fact, FactKind, FactRequest, FactValue, Fetched,
    FsDiv, ReportCode,
};
use pi_store::{FactId, NewFact, Store, StoreError, UnixMillis};

use super::TeamWiring;

/// 받은 자료 묶음.
pub(super) struct Gathered {
    /// 논의 자료(받은 순서).
    pub(super) items: Vec<DataItem>,
    /// 저장한 자료 번호와 알게 된 시각(판단 뒤 매매에 붙인다).
    pub(super) facts: Vec<(FactId, UnixMillis)>,
    /// (10차 10a 2번 · 보강 3번) 키가 있는 출처에 물은 자료 수와 그 가운데 받지 못한 수. 모두 받지 못했으면
    /// 자료 이상 신호다. 키가 없어 묻지 못한 것(`DataError::NoKey`)은 세지 않는다.
    pub(super) asked: u32,
    pub(super) failed: u32,
}

impl Gathered {
    /// 물은 자료가 하나도 답하지 않았나(모두 오류). 물은 것이 하나도 없으면 거짓이다.
    pub(super) fn all_failed(&self) -> bool {
        self.asked > 0 && self.failed == self.asked
    }
}

/// (10차 10b 9번) 미국 금리로 묻는 FRED 계열(미국 10년 국채 금리).
const US_RATE_SERIES: &str = "DGS10";
/// 미국 금리를 묻는 기간(오늘을 포함해 이만큼 앞부터).
const US_RATE_DAYS: i64 = 10;

/// 저장하는 자료 종류 이름.
pub(super) fn kind_name(kind: FactKind) -> &'static str {
    match kind {
        FactKind::Disclosure => "disclosure",
        FactKind::Financials => "financials",
        FactKind::KoreaRate => "korea_rate",
        FactKind::KoreaFx => "korea_fx",
        FactKind::UsRate => "us_rate",
    }
}

fn millis(at: DateTime<Utc>) -> UnixMillis {
    UnixMillis(at.timestamp_millis())
}

/// 그날 기준으로 기간을 채운 ECOS 계열(월: 석 달 전 ~ 이번 달, 그 밖: 열흘 전 ~ 오늘).
fn dated(series: &EcosSeries, today: NaiveDate) -> EcosSeries {
    let (start, end) = match series.cycle {
        EcosCycle::Month => {
            let back = today
                .checked_sub_months(chrono::Months::new(3))
                .unwrap_or(today);
            (
                back.format("%Y%m").to_string(),
                today.format("%Y%m").to_string(),
            )
        }
        _ => (
            (today - Days::days(10)).format("%Y%m%d").to_string(),
            today.format("%Y%m%d").to_string(),
        ),
    };
    EcosSeries {
        start,
        end,
        ..series.clone()
    }
}

/// 공시를 볼 요청(오늘을 포함해 `days`일).
pub(super) fn disclosures(
    wiring: &TeamWiring,
    symbol: &str,
    today: NaiveDate,
) -> Option<FactRequest> {
    let corp = wiring
        .corp_codes
        .get(&pi_market::Symbol::new(symbol))?
        .clone();
    let back = i64::from(wiring.disclosure_days.max(1) - 1);
    Some(FactRequest::Disclosures {
        corp,
        from: today - Days::days(back),
        to: today,
    })
}

/// 그 종목의 재무 요청(지난해 사업보고서 · 연결). 고유번호가 없으면 없음.
pub(super) fn financials(
    wiring: &TeamWiring,
    symbol: &str,
    today: NaiveDate,
) -> Option<FactRequest> {
    let corp = wiring
        .corp_codes
        .get(&pi_market::Symbol::new(symbol))?
        .clone();
    let year = u16::try_from(today.year() - 1).unwrap_or(0);
    Some(FactRequest::Financials {
        corp,
        year,
        report: ReportCode::Annual,
        fs: FsDiv::Consolidated,
    })
}

/// 종목이 없는 거시 요청들: 국내 금리 · 환율(ECOS), 미국 금리(FRED).
pub(super) fn macros(wiring: &TeamWiring, today: NaiveDate) -> [FactRequest; 3] {
    [
        FactRequest::KoreaRate(dated(&wiring.korea_rate, today)),
        FactRequest::KoreaFx(dated(&wiring.korea_fx, today)),
        FactRequest::UsRate {
            series_id: US_RATE_SERIES.to_string(),
            from: today - Days::days(US_RATE_DAYS),
            to: today,
        },
    ]
}

/// 종목 하나의 판단 자료 요청들. 고유번호가 없으면 그 자리는 `Err(종류)`(받을 수 없음)이다.
fn requests(
    wiring: &TeamWiring,
    symbol: &str,
    today: NaiveDate,
) -> Vec<Result<FactRequest, FactKind>> {
    let mut list = vec![
        disclosures(wiring, symbol, today).ok_or(FactKind::Disclosure),
        financials(wiring, symbol, today).ok_or(FactKind::Financials),
    ];
    list.extend(macros(wiring, today).into_iter().map(Ok));
    list
}

/// 기간 안에 공시가 없을 때의 자료 내용(사용자 결정 18).
const NO_DISCLOSURE: &str = "공시 없음";

/// 하루 상한에 닿아 묻지 않은 자료의 글.
fn limited(kind: FactKind) -> &'static str {
    match kind {
        FactKind::Financials => "재무 없음(한도)",
        _ => "공시 없음(한도)",
    }
}

/// 받은 자료 하나를 저장할 모양으로.
pub(super) fn new_fact(fact: &Fact) -> NewFact {
    let value = match &fact.value {
        FactValue::Number(number) => pi_store::FactValue::Number(number.to_string()),
        FactValue::Text(text) => pi_store::FactValue::Text(text.clone()),
    };
    NewFact {
        kind: kind_name(fact.kind).to_string(),
        subject: fact.subject.clone(),
        label: fact.label.clone(),
        period: fact.period.clone(),
        value,
        unit: fact.unit.clone(),
        reference: fact.reference.clone(),
        source_name: fact.source.name.as_str().to_string(),
        source_url: fact.source.url.clone(),
        published_on: fact.published_on.map(|d| d.format("%Y-%m-%d").to_string()),
        known_at: millis(fact.known_at),
    }
}

/// 자료들을 논의에 보일 글로.
fn content(facts: &[Fact]) -> String {
    facts
        .iter()
        .map(|fact| {
            let value = match &fact.value {
                FactValue::Number(number) => number.to_string(),
                FactValue::Text(text) => text.clone(),
            };
            let period = fact.period.as_deref().unwrap_or("-");
            let unit = fact.unit.as_deref().unwrap_or("");
            format!("{} ({period}): {value}{unit}", fact.label)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// 종목 하나의 판단 자료를 받아 저장한다. 받지 못한 것은 내용 없는 항목이 된다.
pub(super) async fn gather(
    adapter: &dyn DataAdapter,
    store: &Store,
    wiring: &TeamWiring,
    symbol: &str,
    (today, now): (NaiveDate, DateTime<Utc>),
) -> Result<Gathered, StoreError> {
    let mut gathered = Gathered {
        items: Vec::new(),
        facts: Vec::new(),
        asked: 0,
        failed: 0,
    };
    for request in requests(wiring, symbol, today) {
        let (kind, fetched) = match request {
            Ok(request) => {
                let fetched = adapter.fetch(&request).await;
                // (10b 보강 1번) DART 하루 상한에 닿아 묻지 않은 것은 "없음(한도)"이고 자료 이상으로 세지 않는다.
                if matches!(&fetched, Err(error) if super::quota::over_limit(error)) {
                    gathered.items.push(DataItem {
                        name: format!(
                            "{} · {}",
                            kind_name(request.kind()),
                            limited(request.kind())
                        ),
                        content: None,
                        known_at: millis(now),
                    });
                    continue;
                }
                let no_key = matches!(fetched, Err(DataError::NoKey { .. }));
                // (10b 9번) FRED 키는 새로 더한 선택 키다: 키가 없으면 자료에 넣지 않는다(받지 못한 자료로 세어
                // 논의를 지켜보기로 바꾸지 않는다).
                if no_key && request.kind() == FactKind::UsRate {
                    continue;
                }
                // 자료 키는 선택이다: 키가 없어 바깥에 묻지 못한 것은 물은 것도 실패한 것도 아니다.
                if !no_key {
                    gathered.asked += 1;
                    gathered.failed += u32::from(fetched.is_err());
                }
                (request.kind(), fetched)
            }
            Err(kind) => (kind, Ok(Fetched::Missing(kind))),
        };
        let name = kind_name(kind);
        match fetched {
            Ok(Fetched::Facts(facts)) if !facts.is_empty() => {
                let saved = store.save_facts(&facts.iter().map(new_fact).collect::<Vec<_>>())?;
                gathered
                    .facts
                    .extend(saved.iter().map(|fact| (fact.id, fact.known_at)));
                let known = facts.iter().map(|f| f.known_at).max().unwrap_or(now);
                let source = &facts[0].source;
                gathered.items.push(DataItem {
                    name: format!("{name} · {} · {}", source.name.as_str(), source.url),
                    content: Some(content(&facts)),
                    known_at: millis(known),
                });
            }
            // (사용자 결정 18) 기간 안에 공시가 없다(DART "조회된 데이터 없음" 013)는 것도 자료다: 빈 자료가 아니다.
            Ok(Fetched::Missing(FactKind::Disclosure)) => gathered.items.push(DataItem {
                name: name.to_string(),
                content: Some(NO_DISCLOSURE.to_string()),
                known_at: millis(now),
            }),
            _ => gathered.items.push(DataItem {
                name: name.to_string(),
                content: None,
                known_at: millis(now),
            }),
        }
    }
    Ok(gathered)
}

#[cfg(test)]
mod tests {
    use pi_data::{FakeDataAdapter, SourceName};

    use super::*;

    async fn disclosure_item(fake: &FakeDataAdapter) -> DataItem {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("facts.db")).unwrap();
        let wiring = TeamWiring::official();
        let today = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let now = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 11, 16, 1, 0, 0).unwrap();
        let gathered = gather(fake, &store, &wiring, "005930", (today, now))
            .await
            .unwrap();
        gathered
            .items
            .into_iter()
            .find(|item| item.name.starts_with("disclosure"))
            .unwrap()
    }

    #[tokio::test]
    async fn 기간_안에_공시가_없으면_공시_없음_자료다() {
        let fake = FakeDataAdapter::new();
        fake.set(
            FactKind::Disclosure,
            Ok(Fetched::Missing(FactKind::Disclosure)),
        );
        let item = disclosure_item(&fake).await;
        assert_eq!(item.content.as_deref(), Some(NO_DISCLOSURE));
    }

    #[tokio::test]
    async fn 공시_요청이_실패하면_빈_자료다() {
        let fake = FakeDataAdapter::new();
        let failed = DataError::Unreachable {
            origin: SourceName::Dart,
            url: "http://127.0.0.1/api/list.json".to_string(),
            detail: "connect".to_string(),
        };
        fake.set(FactKind::Disclosure, Err(failed));
        let item = disclosure_item(&fake).await;
        assert!(item.content.is_none());
    }

    #[tokio::test]
    async fn 하루_상한이면_공시_없음_한도로_적고_빈_자료로_둔다() {
        let fake = FakeDataAdapter::new();
        let limited = DataError::RateLimited {
            origin: SourceName::Dart,
        };
        fake.set(FactKind::Disclosure, Err(limited));
        let item = disclosure_item(&fake).await;
        assert!(item.name.contains("공시 없음(한도)") && item.content.is_none());
    }
}
