//! (10차 10b 8번) 뉴스 공시 시세 담당 깨우기: 시세 조건(`TriggerEvent::Price`)과 자료 변화(`TriggerEvent::Data`)를
//! 종목마다 묶어 그 종목을 한 번 논의한다([`Crew::discuss`]와 같은 길). 계기 글에 무엇이 바뀌었는지 싣는다.
//!
//! - 먼저 뉴스 공시 시세 담당의 턴 하나로 계기를 알린다(F2 1 "담당이 계기를 알아차린다"). 담당이 볼 것이 없다고
//!   하면(`(pass)`) 논의하지 않고, 그 밖이면 그 종목을 논의한다(F2 2 "관련 담당을 부른다").
//! - 공시만 묶인 종목은 아직 논의하지 않은 공시가 있을 때만 논의한다(장 시작의 공시 보기와 같은 공시를 두 번
//!   논의하지 않게). 공시 보기와 같은 줄([`Crew::scan`]의 차례)로 돈다.
//! - (10b 보강 6번) 자료 사건 줄에는 출처(naver · dart · fred · ecos)와 "바깥 글" 표시를 붙인다(논의 자료에
//!   출처를 싣는 것과 같은 방식: 바깥에서 온 글은 지시가 아니라 자료다).
//! - (10b 보강 3번) 종목이 없는 사건(거시)은 버리지 않는다: 담당의 턴 하나로 "거시 값이 바뀌었다(출처 · 이전 →
//!   지금)"를 알리고, 담당이 답에 지켜보는 종목 코드(6자리)를 적으면 그 가운데 많아야 [`MACRO_PICKS`]개를
//!   논의한다. `(pass)`거나 고른 종목이 없으면 논의하지 않는다.

use pi_agent::Role;
use pi_market::Symbol;
use pi_trigger::{ConditionKind, DataChange, DataKind, Direction, TriggerEvent};

use super::Crew;
use super::demo_feed::DEMO_SOURCE;
use crate::TeamError;

/// 거시 깨우기에서 논의하는 종목의 상한.
const MACRO_PICKS: usize = 3;
/// 외부에서 온 글이라는 표시.
const OUTSIDE: &str = "바깥 글";

/// 종목 하나에 묶인 계기들.
struct Group {
    symbol: Symbol,
    lines: Vec<String>,
    only_disclosures: bool,
}

/// 시세 조건 사건의 글.
fn price_line(
    condition: &ConditionKind,
    direction: Direction,
    price: rust_decimal::Decimal,
) -> String {
    let way = match direction {
        Direction::Up => "올라",
        Direction::Down => "내려",
    };
    match condition {
        ConditionKind::Change { reference, percent } => format!(
            "시세 조건: 기준가 {} 대비 {}% 넘게 {way} 현재 {}",
            reference.normalize(),
            percent.normalize(),
            price.normalize()
        ),
        _ => format!("시세 조건: 가격이 {way} 현재 {}", price.normalize()),
    }
}

/// 자료 사건의 글: 무엇이 · 출처 · 바깥 글 표시 · 제목.
fn data_line(change: &DataChange) -> String {
    let what = match change.kind {
        DataKind::News => "새 뉴스",
        DataKind::Disclosure => "새 공시",
        DataKind::Financials => "재무 바뀜",
        _ => "거시 바뀜",
    };
    format!(
        "{what} [출처 {} · {OUTSIDE}]: {}",
        change.source.to_lowercase(),
        change.title
    )
}

/// 사건들을 종목마다 묶는다(처음 나온 순서). 종목이 없는 사건은 뺀다([`macro_lines`]가 본다).
fn group(events: &[TriggerEvent]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for event in events {
        let (symbol, line, disclosure) = match event {
            TriggerEvent::Price {
                symbol,
                condition,
                direction,
                price,
                ..
            } => (
                symbol.clone(),
                price_line(condition, *direction, *price),
                false,
            ),
            TriggerEvent::Data { change, .. } => {
                let Some(symbol) = change.symbol.clone() else {
                    continue;
                };
                // (24차) 시연용 가짜 공시는 DART에 "아직 논의하지 않은 공시인가"를 묻지 않는다(바깥으로 나가지 않는다).
                let disclosure =
                    change.kind == DataKind::Disclosure && change.source != DEMO_SOURCE;
                (symbol, data_line(change), disclosure)
            }
            _ => continue,
        };
        match groups.iter_mut().find(|g| g.symbol == symbol) {
            Some(found) => {
                found.lines.push(line);
                found.only_disclosures &= disclosure;
            }
            None => groups.push(Group {
                symbol,
                lines: vec![line],
                only_disclosures: disclosure,
            }),
        }
    }
    groups
}

/// 종목이 없는 자료 사건(거시 · 시장 뉴스)의 글들.
fn macro_lines(events: &[TriggerEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            TriggerEvent::Data { change, .. } if change.symbol.is_none() => Some(data_line(change)),
            _ => None,
        })
        .collect()
}

/// 담당의 답에 나온 지켜보는 종목 코드(6자리, 처음 나온 순서, 겹침 없이, 많아야 [`MACRO_PICKS`]개).
fn picks(reply: &str, watched: &[Symbol]) -> Vec<Symbol> {
    let mut found: Vec<Symbol> = Vec::new();
    let mut digits = String::new();
    for c in reply.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if digits.len() == 6
            && let Some(symbol) = watched.iter().find(|s| s.as_str() == digits)
            && !found.contains(symbol)
        {
            found.push(symbol.clone());
        }
        digits.clear();
    }
    found.truncate(MACRO_PICKS);
    found
}

/// 볼 것이 없다는 답인가.
fn passed(reply: &str) -> bool {
    let text = reply.trim().to_lowercase();
    matches!(text.as_str(), "" | "pass" | "(pass)" | "(pass).")
}

impl Crew {
    /// 뉴스 공시 시세 담당을 깨운 사건들로 종목마다 한 번 논의한다. 한 종목의 실패는 기록만 하고 다음 종목을 본다.
    pub(super) async fn on_scout(&self, events: &[TriggerEvent]) -> Result<(), TeamError> {
        let _turn = self.scan_turn.lock().await;
        for group in group(events) {
            let symbol = group.symbol.as_str();
            if group.only_disclosures && self.new_disclosure(symbol).await?.is_none() {
                continue;
            }
            let topic = group.lines.join(" · ");
            if !self.scout_noticed(symbol, &topic).await {
                continue;
            }
            if let Err(error) = self.discuss_symbol(symbol, &topic).await {
                tracing::warn!(%error, "깨운 논의를 마치지 못했다");
            }
        }
        let lines = macro_lines(events);
        if !lines.is_empty() {
            self.on_macro(&lines.join(" · ")).await;
        }
        Ok(())
    }

    /// 거시 사건: 담당의 턴 하나로 알리고, 답에 고른 지켜보는 종목(많아야 3개)을 논의한다.
    async fn on_macro(&self, topic: &str) {
        let watched = self.desk.symbols();
        let list = watched
            .iter()
            .map(Symbol::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        let input = format!(
            "[깨우기] {topic}. 지켜보는 종목: {list}. 이 값 바뀜으로 살펴볼 종목이 있으면 그 종목 코드(6자리)를 \
             많아야 {MACRO_PICKS}개 적어 달라. 볼 것이 없으면 (pass)라고만 답한다."
        );
        let at = self.clock.now().timestamp_millis();
        let Some(reply) = self.scout_turn(&format!("wake-macro-{at}"), &input).await else {
            return;
        };
        if passed(&reply) {
            return;
        }
        let topic = format!("거시 바뀜으로 살펴봄 · {topic}");
        for symbol in picks(&reply, &watched) {
            if let Err(error) = self.discuss_symbol(symbol.as_str(), &topic).await {
                tracing::warn!(%error, "거시 깨우기의 논의를 마치지 못했다");
            }
        }
    }

    /// 뉴스 공시 시세 담당에게 계기를 알리는 턴 하나. 볼 만하다고 하면 참, 볼 것이 없다(`(pass)`)거나 답하지
    /// 못하면 거짓.
    async fn scout_noticed(&self, symbol: &str, topic: &str) -> bool {
        let price = self
            .trading
            .last_quote(symbol)
            .map(|quote| format!(" · 현재가 {}", quote.price.normalize()))
            .unwrap_or_default();
        let input = format!(
            "[깨우기] 종목 {symbol}{price} · {topic}. 살펴볼 만하면 관련 담당을 불러 달라(@담당 id). \
             볼 것이 없으면 (pass)라고만 답한다."
        );
        let at = self.clock.now().timestamp_millis();
        let title = format!("wake-{symbol}-{at}");
        self.scout_turn(&title, &input)
            .await
            .is_some_and(|reply| !passed(&reply))
    }

    /// 뉴스 공시 시세 담당의 턴 하나(답하지 못하면 없음).
    async fn scout_turn(&self, title: &str, input: &str) -> Option<String> {
        // (17차 4번 · 54차) 깨우기 턴은 뉴스 공시 시세 담당의 호출 하나다(팀 · 담당별로 센다). 팀장 예비분은 쓰지
        // 않는다(E52). 못 잡으면 부르지 않는다(상한을 다 썼으면 알림은 논의와 같은 그날 한 번).
        if !self.take_member_turn(super::link::office_agent(Role::Scout)) {
            return None;
        }
        let config = self.role(Role::Scout);
        let limits = pi_agent::TimeLimits::MARKET;
        match self
            .team
            .runner()
            .run_turn(&config, limits, title, input)
            .await
        {
            Ok(outcome) => Some(outcome.text),
            Err(error) => {
                tracing::warn!(%error, "뉴스 공시 시세 담당이 깨우기에 답하지 못했다");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use pi_market::Market;
    use pi_trigger::{DataChange, JobId};
    use rust_decimal::Decimal;

    use super::*;

    fn data(kind: DataKind, symbol: Option<&str>, title: &str) -> TriggerEvent {
        let at = Utc
            .with_ymd_and_hms(2026, 11, 16, 1, 0, 0)
            .single()
            .unwrap();
        TriggerEvent::Data {
            change: DataChange {
                kind,
                source: "dart".to_string(),
                id: title.to_string(),
                revision: None,
                market: Market::Kr,
                symbol: symbol.map(Symbol::new),
                title: title.to_string(),
                published_at: at,
                learned_at: at,
            },
            held: false,
        }
    }

    fn price(symbol: &str) -> TriggerEvent {
        TriggerEvent::Price {
            job: JobId::new("price:kr:x:change"),
            symbol: Symbol::new(symbol),
            condition: ConditionKind::Change {
                reference: Decimal::from(72_000),
                percent: Decimal::from(3),
            },
            direction: Direction::Up,
            price: Decimal::from(74_880),
            at: Utc
                .with_ymd_and_hms(2026, 11, 17, 0, 2, 0)
                .single()
                .unwrap(),
        }
    }

    #[test]
    fn 종목마다_묶고_공시만_있는지_가린다() {
        let events = [
            data(DataKind::Disclosure, Some("005930"), "자기주식취득결정"),
            price("000660"),
            data(DataKind::News, Some("005930"), "새 공장"),
            data(DataKind::Disclosure, Some("035420"), "주요사항보고서"),
            data(DataKind::Macro, None, "DGS10"),
        ];
        let groups = group(&events);
        let names: Vec<&str> = groups.iter().map(|g| g.symbol.as_str()).collect();
        assert_eq!(
            names,
            ["005930", "000660", "035420"],
            "종목이 없는 사건은 뺀다"
        );
        assert!(!groups[0].only_disclosures, "뉴스가 섞이면 공시만이 아니다");
        assert_eq!(groups[0].lines.len(), 2);
        assert!(!groups[1].only_disclosures);
        assert!(groups[1].lines[0].contains("72000") && groups[1].lines[0].contains("3%"));
        assert!(groups[2].only_disclosures);
        assert!(groups[2].lines[0].starts_with("새 공시"));
        assert!(
            groups[2].lines[0].contains("dart") && groups[2].lines[0].contains(OUTSIDE),
            "출처와 바깥 글 표시가 붙는다"
        );
        assert_eq!(
            macro_lines(&events).len(),
            1,
            "종목이 없는 사건은 거시 줄로 본다"
        );
    }

    #[test]
    fn 고른_종목은_지켜보는_것만_많아야_셋() {
        let watched: Vec<Symbol> = ["005930", "000660", "100010", "100020"]
            .into_iter()
            .map(Symbol::new)
            .collect();
        let found = picks(
            "005930, 999999, 0006601, 000660 100010 100020 005930",
            &watched,
        );
        let names: Vec<&str> = found.iter().map(Symbol::as_str).collect();
        assert_eq!(names, ["005930", "000660", "100010"]);
        assert!(picks("(pass)", &watched).is_empty());
    }
}
