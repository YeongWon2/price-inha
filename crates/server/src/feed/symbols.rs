//! 시세 흐름이 무엇을 물을지: 지켜보는 종목 + (10차 10b 0번 나) 보유 종목.
//!
//! 보유 종목은 지켜보는 목록에 없어도(빼도) 보유가 남아 있는 동안 함께 받는다. 그래야 그 보유의 손절 감시가
//! 시세를 받고, 시세가 끊기면 이상 감지가 알아챈다. 시연 모드는 시세가 지어낸 값이라 보유 종목을 더하지 않는다
//! (손으로 넣은 보유의 손절을 지어낸 값이 건드리지 않게).

use tokio::time::Instant;

use super::fetch::Plan;
use super::{Feed, REJECTED_RECHECK, late};

impl Feed {
    /// 지켜보는 종목이 바뀌었으면 받아들인다. 빠진 종목의 것은 버리고, 거절 기억은 처음부터다.
    /// (7차 3번) 시연인데 지켜볼 종목이 없으면 시연 종목 하나를 쓴다(저장하지 않는다).
    /// (10b) 시연이 아니면 보유 종목도 뒤에 더한다.
    pub(super) fn adopt_symbols(&mut self) {
        let mut symbols = self.desk.symbols();
        if symbols.is_empty() && self.desk.demo() {
            symbols.push(crate::demo::demo_symbol(self.market));
        }
        if let Some(trading) = self.trading.as_ref().filter(|_| !self.desk.demo()) {
            for held in trading.held_symbols() {
                if !symbols.contains(&held) {
                    symbols.push(held);
                }
            }
        }
        if symbols != self.symbols {
            self.symbols = symbols;
            self.board.keep_only(&self.symbols);
            self.remember_rejected(Vec::new());
        }
    }

    /// 이번에 무엇을 물을지 정한다.
    pub(super) fn plan(&mut self) -> Plan {
        // 거절된 종목도 가끔은 다시 물어본다(새로 생긴 종목 · 잘못 가려낸 종목).
        // 다시 물어본 결과가 올 때까지 이 표시는 남는다(받기가 실패하면 다음에 또 다시 묻는다).
        self.recheck_rejected |= self
            .rejected_since
            .is_some_and(|since| since.elapsed() >= REJECTED_RECHECK);
        let skip = if self.recheck_rejected {
            Vec::new()
        } else {
            self.rejected.clone()
        };
        let today = late::market_date(self.market, self.clock.now());
        let waiting = self.calendar_retry_at.is_some_and(|at| Instant::now() < at);
        let calendar = (!self.board.has_calendar_for(today) && !waiting).then_some(today);
        Plan {
            symbols: self.symbols.clone(),
            skip,
            calendar,
        }
    }
}
