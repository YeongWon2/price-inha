//! 자료 잇기(T10): 종목 하나의 판단 자료를 받아 저장하고 논의 자료(`pi_agent::DataItem`)로 옮긴다.
//!
//! 받는 것: 공시 · 재무(DART, 고유번호가 있는 종목만), 국내 금리 · 환율(ECOS). 받은 자료는 저장소에 쌓고
//! (`Store::save_facts`), 판단된 뒤에 매매에 붙인다. 비었거나(`Missing`) 받지 못한 자료는 내용이 없는
//! 자료 항목이 된다(실행기가 지켜보기로 둔다, E32).

use chrono::{DateTime, Datelike, Duration as Days, NaiveDate, Utc};
use pi_agent::DataItem;
use pi_data::{
    DataAdapter, EcosCycle, EcosSeries, Fact, FactKind, FactRequest, FactValue, Fetched, FsDiv,
    ReportCode,
};
use pi_store::{FactId, NewFact, Store, StoreError, UnixMillis};

use super::TeamWiring;

/// 받은 자료 묶음.
pub(super) struct Gathered {
    /// 논의 자료(받은 순서).
    pub(super) items: Vec<DataItem>,
    /// 저장한 자료 번호와 알게 된 시각(판단 뒤 매매에 붙인다).
    pub(super) facts: Vec<(FactId, UnixMillis)>,
}

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

/// 종목 하나의 판단 자료 요청들. 고유번호가 없으면 그 자리는 `Err(종류)`(받을 수 없음)이다.
fn requests(
    wiring: &TeamWiring,
    symbol: &str,
    today: NaiveDate,
) -> Vec<Result<FactRequest, FactKind>> {
    let corp = wiring
        .corp_codes
        .get(&pi_market::Symbol::new(symbol))
        .cloned();
    let year = u16::try_from(today.year() - 1).unwrap_or(0);
    let financials = corp.map(|corp| FactRequest::Financials {
        corp,
        year,
        report: ReportCode::Annual,
        fs: FsDiv::Consolidated,
    });
    vec![
        disclosures(wiring, symbol, today).ok_or(FactKind::Disclosure),
        financials.ok_or(FactKind::Financials),
        Ok(FactRequest::KoreaRate(dated(&wiring.korea_rate, today))),
        Ok(FactRequest::KoreaFx(dated(&wiring.korea_fx, today))),
    ]
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
    };
    for request in requests(wiring, symbol, today) {
        let (kind, fetched) = match request {
            Ok(request) => (request.kind(), adapter.fetch(&request).await),
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
            _ => gathered.items.push(DataItem {
                name: name.to_string(),
                content: None,
                known_at: millis(now),
            }),
        }
    }
    Ok(gathered)
}
