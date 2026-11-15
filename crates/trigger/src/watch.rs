//! 종목마다의 늦은 시세 감시(N6). 메모리 안에만 둔다(다시 켜면 처음부터 잰다).

use chrono::{DateTime, Utc};
use pi_market::{Market, MarketCalendar, Quote, StalePolicy, Symbol, is_stale};

use crate::TriggerEvent;

/// 종목 하나의 감시 상태.
#[derive(Debug, Clone, Default)]
pub(crate) struct QuoteWatch {
    /// 마지막으로 받은 시세.
    last: Option<Quote>,
    /// 장이 열린 동안 처음 받으려 한 시각(한 번도 못 받았을 때 여기서 잰다).
    first_attempt: Option<DateTime<Utc>>,
    /// 늦어진 동안 이미 깨웠는가.
    alerted: bool,
}

impl QuoteWatch {
    /// 장이 닫혔다. 늦은 것으로 보지 않고, 못 받은 채 잰 시각도 버린다.
    pub(crate) fn closed(&mut self) {
        self.alerted = false;
        self.first_attempt = None;
    }

    /// 장이 열린 동안 받으려 한 결과를 본다.
    ///
    /// 돌려주는 것: (늦지 않았으면 이번에 받은 시세, 늦어져 처음 깨울 때의 사건).
    pub(crate) fn observe(
        &mut self,
        market: Market,
        symbol: &Symbol,
        received: Option<Quote>,
        calendar: &MarketCalendar,
        now: DateTime<Utc>,
        policy: &StalePolicy,
    ) -> (Option<Quote>, Option<TriggerEvent>) {
        if self.first_attempt.is_none() {
            self.first_attempt = Some(now);
        }
        if let Some(quote) = &received
            && self
                .last
                .as_ref()
                .is_none_or(|last| quote.received_at >= last.received_at)
        {
            self.last = Some(quote.clone());
        }
        let stale = match &self.last {
            Some(last) => is_stale(last, calendar, now, policy),
            None => self.first_attempt.is_some_and(|first| {
                now.signed_duration_since(first)
                    .to_std()
                    .is_ok_and(|age| age > policy.max_age)
            }),
        };
        if !stale {
            self.alerted = false;
            return (received.and(self.last.clone()), None);
        }
        if self.alerted {
            return (None, None);
        }
        self.alerted = true;
        let event = TriggerEvent::StaleQuote {
            market,
            symbol: symbol.clone(),
            last_received: self.last.as_ref().map(|last| last.received_at),
        };
        (None, Some(event))
    }
}
