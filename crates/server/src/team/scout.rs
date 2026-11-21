//! (10차 10b 8번) 뉴스 공시 시세 담당 깨우기: 시세 조건(`TriggerEvent::Price`)과 자료 변화(`TriggerEvent::Data`)를
//! 종목마다 묶어 그 종목을 한 번 논의한다([`Crew::discuss`]와 같은 길). 계기 글에 무엇이 바뀌었는지 싣는다.
//!
//! - 먼저 뉴스 공시 시세 담당의 턴 하나로 계기를 알린다(F2 1 "담당이 계기를 알아차린다"). 담당이 볼 것이 없다고
//!   하면(`(pass)`) 논의하지 않고, 그 밖이면 그 종목을 논의한다(F2 2 "관련 담당을 부른다").
//! - 공시만 묶인 종목은 아직 논의하지 않은 공시가 있을 때만 논의한다(장 시작의 공시 보기와 같은 공시를 두 번
//!   논의하지 않게). 공시 보기와 같은 줄([`Crew::scan`]의 차례)로 돈다.
//! - 종목이 없는 사건(거시 · 시장 뉴스)은 지금 논의할 종목이 없어 건너뛴다.

use pi_agent::Role;
use pi_market::Symbol;
use pi_trigger::{ConditionKind, DataKind, Direction, TriggerEvent};

use super::Crew;
use crate::TeamError;

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

/// 자료 사건의 글.
fn data_line(kind: DataKind, title: &str) -> String {
    let what = match kind {
        DataKind::News => "새 뉴스",
        DataKind::Disclosure => "새 공시",
        DataKind::Financials => "재무 바뀜",
        _ => "거시 바뀜",
    };
    format!("{what}: {title}")
}

/// 사건들을 종목마다 묶는다(처음 나온 순서).
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
                let disclosure = change.kind == DataKind::Disclosure;
                (symbol, data_line(change.kind, &change.title), disclosure)
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
        Ok(())
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
        let config = self.role(Role::Scout);
        let limits = pi_agent::TimeLimits::MARKET;
        match self
            .team
            .runner()
            .run_turn(config, limits, &title, &input)
            .await
        {
            Ok(outcome) => {
                let text = outcome.text.trim().to_lowercase();
                !matches!(text.as_str(), "" | "pass" | "(pass)" | "(pass).")
            }
            Err(error) => {
                tracing::warn!(%error, "뉴스 공시 시세 담당이 깨우기에 답하지 못했다");
                false
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
    }
}
