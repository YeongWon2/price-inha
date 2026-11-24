//! (24차 2 · 3번, D14 · R2-3) 시연용 가짜 공시: 실행 파일의 시연 조립에서 간격([`crate::DemoWiring::disclosure_every`],
//! 시계 기준)마다 지켜보는 종목을 돌아가며 "시연" 공시 한 건을 만든다. 깨우기 엔진의 자료 사건 길
//! ([`super::poll::Poller`] → `Trigger::push_data` → 뉴스 공시 시세 담당 → 종목 논의)로 넣는다.
//!
//! - 제목에 [`crate::DEMO_DISCLOSURE_MARK`]를 적고 실제 출처 주소를 쓰지 않는다(출처 이름은 [`DEMO_SOURCE`]).
//! - 바깥에 묻지 않는다. 그래서 "아직 논의하지 않은 공시인가"를 DART에 묻지 않고 바로 논의한다
//!   ([`super::scout`]가 [`DEMO_SOURCE`]를 공시만 묶인 것으로 보지 않는다).

use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::Symbol;
use pi_trigger::{DataChange, DataKind};

use super::Crew;
use super::poll::short;
use crate::DEMO_DISCLOSURE_MARK;
use crate::demo::demo_symbol;

/// 시연용 가짜 공시의 출처 이름(실제 출처가 아니다).
pub(super) const DEMO_SOURCE: &str = "demo";

/// 가짜 공시를 내는 차례.
pub(super) struct DemoDisclosures {
    every: Duration,
    /// 지난번에 낸(처음에는 세기 시작한) 시각.
    last: Option<DateTime<Utc>>,
    /// 지금까지 낸 수(종목을 돌아가며 고르고 번호를 붙인다).
    count: u64,
}

impl DemoDisclosures {
    pub(super) fn new(every: Duration) -> DemoDisclosures {
        DemoDisclosures {
            every,
            last: None,
            count: 0,
        }
    }

    /// 간격이 지났으면 다음 가짜 공시 하나. 처음 부르면 세기만 시작한다(한 간격 뒤에 첫 건을 낸다).
    /// 시계가 뒤로 가면 지금부터 다시 센다.
    pub(super) fn next(&mut self, crew: &Crew, now: DateTime<Utc>) -> Option<DataChange> {
        let Some(last) = self.last.filter(|last| *last <= now) else {
            self.last = Some(now);
            return None;
        };
        let gone = now.signed_duration_since(last).to_std().unwrap_or_default();
        if gone < self.every {
            return None;
        }
        self.last = Some(now);
        let symbol = self.pick(crew);
        self.count += 1;
        Some(self.change(crew, symbol, now))
    }

    /// 지켜보는 종목을 돌아가며 고른다. 없으면 시연 종목 하나.
    fn pick(&self, crew: &Crew) -> Symbol {
        let symbols = crew.desk.symbols();
        let turn = usize::try_from(self.count).unwrap_or(0);
        match symbols.len() {
            0 => demo_symbol(crew.trading.market()),
            len => symbols[turn % len].clone(),
        }
    }

    fn change(&self, crew: &Crew, symbol: Symbol, now: DateTime<Utc>) -> DataChange {
        let mark = DEMO_DISCLOSURE_MARK;
        let title = format!(
            "[{mark}] {} {mark}용 가짜 공시 {}번 — {mark} 모드에서 만든 글이며 실제 공시가 아닙니다",
            symbol.as_str(),
            self.count
        );
        DataChange {
            kind: DataKind::Disclosure,
            source: DEMO_SOURCE.to_string(),
            id: format!(
                "{DEMO_SOURCE}-{}-{}",
                symbol.as_str(),
                now.timestamp_millis()
            ),
            revision: None,
            market: crew.trading.market(),
            symbol: Some(symbol),
            title: short(crew, &title),
            published_at: now,
            learned_at: now,
        }
    }
}
