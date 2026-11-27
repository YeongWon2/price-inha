//! (10차 10b 보강 3번, 8번 그대로, 결정 13) 재무 · 거시 값 바뀜으로 깨우기.
//!
//! 주기 가져오기가 장 시간 안에서 하루 한 번(장 시작 뒤) 재무(지켜보는 종목, DART) · 거시(ECOS 금리 · 환율,
//! FRED 금리)를 받아, **지난번 값과 다르면** `Trigger::push_data`(`DataKind::Financials` · `Macro`)로 넘긴다.
//!
//! - 지난번 값은 본 것 기억과 같은 저장소 칸(`knowledge_meta`)에 둔다(다시 켜도 잇는다). 처음 받은 값은 지난번
//!   값으로만 둔다(설치한 날 깨우지 않는다).
//! - 키가 없는 출처 · 하루 상한에 닿은 DART는 건너뛴다. 받지 못한 것은 기록만 한다(자료 이상 신호는 논의의 자료
//!   받기만 센다 — 주기 가져오기와 같은 규칙).
//! - 비교는 값만 본다(관측 날짜가 바뀌어도 값이 같으면 바뀌지 않은 것이다). 재무는 계정마다, 거시는 가장
//!   최근 관측 하나.
//! - 깨우기 엔진이 받지 않은 것(속도 제한)은 지난번 값을 되돌려 다음 날 다시 넘긴다.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use pi_data::{DataError, Fact, FactKind, FactRequest, FactValue, Fetched};
use pi_market::Symbol;
use pi_trigger::{DataChange, DataKind, DataPush, Trigger};

use super::{Crew, facts, poll, quota};

/// 지난번 값을 남기는 저장소 칸의 머리.
const LAST_KEY: &str = "data_poll.last";
/// 사건 번호에 싣는 값 글의 상한(글자).
const ID_MAX: usize = 120;

/// 이름(계정 · 계열) → 값 글.
type Values = BTreeMap<String, String>;

/// 하루 한 번 재무 · 거시 값을 보는 기억(그날 본 날짜).
#[derive(Default)]
pub(super) struct Changes {
    last_day: Option<NaiveDate>,
    /// (round64) 참이면 다음 하루 보기는 지난번 값만 새로 두고 넘기지 않는다(공시 구독을 다시 켠 뒤).
    rebase: bool,
}

impl Changes {
    /// 오늘 아직 보지 않았으면 재무 · 거시 값을 받아 바뀐 것을 넘긴다.
    pub(super) async fn daily(&mut self, crew: &Crew, trigger: &Trigger, now: DateTime<Utc>) {
        let today = crew.trading.date_of(now);
        if self.last_day == Some(today) {
            return;
        }
        self.last_day = Some(today);
        let push = !std::mem::take(&mut self.rebase);
        for symbol in crew.desk.symbols() {
            if let Some(request) = facts::financials(&crew.wiring, symbol.as_str(), today) {
                check(crew, trigger, &request, (Some(&symbol), push), now).await;
            }
        }
        for request in facts::macros(&crew.wiring, today) {
            check(crew, trigger, &request, (None, push), now).await;
        }
    }

    /// (round64) 공시 구독을 다시 켰다: 꺼진 사이 바뀐 재무 · 거시 값으로 깨우지 않게, 다음 하루 보기는 지난번 값만
    /// 새로 둔다. 오늘 이미 봤으면 다음 날 보기가 그렇게 한다.
    pub(super) fn rebase(&mut self) {
        self.rebase = true;
    }
}

/// 값 글(숫자는 끝의 0을 뺀다: `4.10`과 `4.1`은 같은 값이다).
fn value_text(value: &FactValue) -> String {
    match value {
        FactValue::Number(number) => number.normalize().to_string(),
        FactValue::Text(text) => text.clone(),
    }
}

/// 비교할 값들: 재무는 계정마다, 거시는 가장 최근 관측 하나.
fn snapshot(kind: FactKind, facts: &[Fact]) -> Values {
    if kind == FactKind::Financials {
        return facts
            .iter()
            .map(|fact| (fact.label.clone(), value_text(&fact.value)))
            .collect();
    }
    facts
        .iter()
        .max_by(|a, b| a.period.cmp(&b.period).then(a.known_at.cmp(&b.known_at)))
        .map(|fact| (fact.label.clone(), value_text(&fact.value)))
        .into_iter()
        .collect()
}

/// 바뀐 것들의 글(`이름 이전 → 지금`).
fn describe(before: &Values, now: &Values) -> String {
    now.iter()
        .filter(|(name, value)| before.get(*name) != Some(*value))
        .map(|(name, value)| {
            let old = before.get(name).map_or("-", String::as_str);
            format!("{name} {old} → {value}")
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// 저장소에 남긴 지난번 값.
fn load(crew: &Crew, key: &str) -> Option<Values> {
    let saved = crew.store.knowledge_meta(key).ok().flatten()?;
    serde_json::from_str(&saved).ok()
}

fn save(crew: &Crew, key: &str, values: &Values) {
    let text = serde_json::to_string(values).unwrap_or_else(|_| "{}".to_string());
    if let Err(error) = crew.store.set_knowledge_meta(key, &text) {
        tracing::warn!(%error, "지난번 자료 값을 저장소에 남기지 못했다");
    }
}

/// 받은 자료(키 없음 · 한도 · 빈 것은 없음).
async fn fetch(crew: &Crew, request: &FactRequest) -> Option<Vec<Fact>> {
    match crew.data.fetch(request).await {
        Ok(Fetched::Facts(found)) if !found.is_empty() => Some(found),
        Ok(_) | Err(DataError::NoKey { .. }) => None,
        Err(error) if quota::over_limit(&error) => None,
        Err(error) => {
            tracing::warn!(%error, "재무 · 거시 값을 받지 못했다");
            None
        }
    }
}

/// 요청 하나를 받아 지난번 값과 견주고, 다르면 넘긴다.
async fn check(
    crew: &Crew,
    trigger: &Trigger,
    request: &FactRequest,
    (symbol, push): (Option<&Symbol>, bool),
    now: DateTime<Utc>,
) {
    let Some(found) = fetch(crew, request).await else {
        return;
    };
    let kind = request.kind();
    let values = snapshot(kind, &found);
    let subject = symbol.map_or("-", Symbol::as_str);
    let key = format!("{LAST_KEY}.{}.{subject}", facts::kind_name(kind));
    let before = load(crew, &key);
    if before.as_ref() == Some(&values) {
        return;
    }
    save(crew, &key, &values);
    // 처음 받은 값은 지난번 값으로만 둔다. (round64) 구독을 다시 켠 뒤 첫 보기도 그렇다.
    let Some(before) = before.filter(|_| push) else {
        return;
    };
    let source = found[0].source.name.as_str().to_lowercase();
    let (data_kind, what) = match kind {
        FactKind::Financials => (DataKind::Financials, "재무 값이 바뀌었다"),
        _ => (DataKind::Macro, "거시 값이 바뀌었다"),
    };
    let diff = describe(&before, &values);
    let id: String = format!("{}:{subject}:{diff}", facts::kind_name(kind))
        .chars()
        .take(ID_MAX)
        .collect();
    let change = DataChange {
        kind: data_kind,
        source,
        id,
        revision: None,
        market: crew.trading.market(),
        symbol: symbol.cloned(),
        title: poll::short(crew, &format!("{what}({diff})")),
        published_at: now,
        learned_at: now,
    };
    if trigger.push_data(change) == DataPush::RateLimited {
        // 받지 않았으면 지난번 값을 되돌려 다음 날 다시 넘긴다.
        save(crew, &key, &before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> Values {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    #[test]
    fn 바뀐_것만_이전과_지금을_적는다() {
        let before = values(&[("매출액", "100"), ("영업이익", "10")]);
        let now = values(&[("매출액", "120"), ("영업이익", "10")]);
        assert_eq!(describe(&before, &now), "매출액 100 → 120");
    }

    #[test]
    fn 숫자는_끝의_0을_빼고_견준다() {
        let a = value_text(&FactValue::Number("4.10".parse().unwrap()));
        let b = value_text(&FactValue::Number("4.1".parse().unwrap()));
        assert_eq!(a, b);
    }
}
