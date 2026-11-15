//! 시세 흐름: 필수 설정이 갖춰지면 시작해, 받은 시세를 지나가는 이벤트로 보내고,
//! 늦은 시세(N6) · 시세 상태(V31) · 설정 알림을 알린다.
//!
//! - 필수 설정이 빠져 있으면 바깥으로 아무 요청도 보내지 않고 기다린다. 등록이 깨우면 바로 다시 본다.
//! - 증권사 요청이 실패하면 다시 시도하는 간격을 늘린다(물러서기). 같은 실패가 이어지는 동안에는
//!   시세 상태를 다시 알리지 않고, 기록도 상태가 바뀔 때와 [`LOG_EVERY`]마다만 남긴다.
//! - 받는 일([`fetch`])은 흐름의 상태를 쥐지 않고 따로 돈다. 요청이 오래 걸려도 늦은 시세는 제때 본다.

mod fetch;
mod late;
mod step;

use std::sync::Arc;
use std::time::Duration;

use pi_gate::Publisher;
use pi_market::{Clock, Market, MarketAdapter, MarketError, Symbol};
use serde_json::json;
use tokio::time::Instant;

use crate::desk::Desk;
use crate::retry::{Backoff, LOG_EVERY, LogGate};
use crate::{
    FEED_BROKER_REJECTED, FEED_CONNECTING, FEED_DEMO, FEED_LIVE, FEED_REQUEST_REJECTED,
    FEED_SETUP_REQUIRED, KIND_FEED_STATUS, KIND_QUOTES, KIND_QUOTES_STALE, KIND_SETUP,
    KIND_SYMBOLS_REJECTED, SETUP_REQUIRED, SETUP_STARTED,
};
use fetch::{Fetched, Plan};
use late::Board;
use step::{Rested, Step, failure_state, millis};

pub(crate) use step::{Pace, Source};

/// 필수 설정이 그대로인지 다시 보는 간격(키 파일만 읽는다. 바깥으로 나가지 않는다).
/// 화면에서 한 등록은 이 간격을 기다리지 않고 흐름을 바로 깨운다. 이것은 키 파일이
/// 바깥에서 지워지거나 채워지는 것을 알아채는 길이다.
const KEY_POLL: Duration = Duration::from_secs(2);
/// 기다리거나 받는 동안 늦은 시세를 다시 보는 간격.
const STALE_CHECK: Duration = Duration::from_millis(500);
/// 가장 짧은 시세 간격(0이 와도 쉬지 않고 돌지 않게).
const MIN_INTERVAL: Duration = Duration::from_millis(1);
/// 증권사가 거절한 종목을 다시 물어보기까지의 시간. 그 전에는 지켜보는 종목이 바뀔 때,
/// 등록이 흐름을 깨울 때, 흐름이 다시 시작할 때만 다시 묻는다.
const REJECTED_RECHECK: Duration = Duration::from_secs(600);
/// 증권사가 준 기다림(`Retry-After`)을 따르는 가장 긴 시간.
const MAX_ASKED_WAIT: Duration = Duration::from_secs(3600);

/// 시세 흐름 하나.
pub(crate) struct Feed {
    publisher: Publisher,
    clock: Arc<dyn Clock>,
    market: Market,
    desk: Arc<Desk>,
    pace: Pace,
    /// 지금 지켜보는 종목.
    symbols: Vec<Symbol>,
    board: Board,
    /// 증권사가 거절했다고 기억하는 종목과 처음 기억한 때.
    rejected: Vec<Symbol>,
    rejected_since: Option<Instant>,
    /// 다음 받기에서는 거절된 종목도 빼지 않고 다시 물어본다.
    recheck_rejected: bool,
    calendar_retry_at: Option<Instant>,
    /// 증권사가 준 기다림(`Retry-After`)이 끝나는 때. 흐름이 멈췄다 다시 시작해도 지킨다(4차).
    asked_until: Option<Instant>,
    /// 필수 설정을 마지막으로 다시 본 때.
    ready_checked: Instant,
    /// 마지막으로 알린 시세 상태. 같은 상태는 다시 알리지 않는다.
    status: Option<&'static str>,
    /// 마지막으로 알린 설정 알림(상태와 빠진 항목).
    setup: Option<(&'static str, Vec<&'static str>)>,
    /// 시세 받기 실패의 기록 묶기.
    quotes_log: LogGate,
    /// 달력 받기 실패의 기록 묶기.
    calendar_log: LogGate,
}

impl Feed {
    pub(crate) fn new(
        publisher: Publisher,
        clock: Arc<dyn Clock>,
        market: Market,
        desk: Arc<Desk>,
        pace: Pace,
    ) -> Self {
        Self {
            publisher,
            clock,
            market,
            desk,
            pace,
            symbols: Vec::new(),
            board: Board::default(),
            rejected: Vec::new(),
            rejected_since: None,
            recheck_rejected: false,
            calendar_retry_at: None,
            asked_until: None,
            ready_checked: Instant::now(),
            status: None,
            setup: None,
            quotes_log: LogGate::new(LOG_EVERY),
            calendar_log: LogGate::new(LOG_EVERY),
        }
    }

    /// 시세 상태를 알린다. 마지막으로 알린 것과 같으면 다시 알리지 않는다.
    fn status(&mut self, state: &'static str) {
        if self.status != Some(state) {
            self.status = Some(state);
            self.publisher
                .publish_transient(KIND_FEED_STATUS, json!({ "state": state }));
        }
    }

    /// 설정 알림을 보낸다. 마지막으로 알린 것과 같으면 다시 알리지 않는다.
    fn announce_setup(&mut self, state: &'static str, missing: Vec<&'static str>) {
        let notice = (state, missing);
        if self.setup.as_ref() != Some(&notice) {
            self.publisher.publish_transient(
                KIND_SETUP,
                json!({ "state": notice.0, "missing": notice.1, "demo": self.desk.demo() }),
            );
            self.setup = Some(notice);
        }
    }

    /// 끝없이 돈다. 멈추려면 이 future를 버린다.
    pub(crate) async fn run(mut self, source: Source) {
        match source {
            Source::Demo(adapter) => {
                self.desk.set_started(true);
                self.announce_setup(SETUP_STARTED, Vec::new());
                self.status(FEED_DEMO);
                loop {
                    self.pump(&adapter, true).await;
                    // 가짜 어댑터는 키를 묻지 않는다. 그래도 돌아오면 쉬었다가 다시 돈다.
                    tokio::time::sleep(KEY_POLL).await;
                }
            }
            Source::Broker(adapter) => loop {
                self.wait_until_ready().await;
                // 시세를 실제로 받기 전에는 "증권사 시세"라고 하지 않는다.
                self.status(FEED_CONNECTING);
                self.pump(&adapter, false).await;
                self.halt();
                if self.desk.ready() {
                    // 필수 설정은 있다는데 어댑터가 키가 없다고 했다. 곧바로 다시 돌지 않는다.
                    self.nap(KEY_POLL).await;
                }
            },
        }
    }

    /// `wait`만큼, 또는 등록이 깨울 때까지 기다린다.
    async fn nap(&self, wait: Duration) {
        tokio::select! {
            () = self.desk.woken() => {}
            () = tokio::time::sleep(wait) => {}
        }
    }

    /// 필수 설정이 모두 등록될 때까지 기다린다. 그동안 바깥으로 아무 요청도 보내지 않는다(V31).
    /// 무엇이 빠졌는지 알리고, 다 갖춰지면 시작했다고 알린다.
    async fn wait_until_ready(&mut self) {
        loop {
            let setup = self.desk.status();
            if setup.ready {
                break;
            }
            self.desk.set_started(false);
            // 나중에 연결한 화면이 "증권사 시세"를 보지 않게 시세 상태부터 바꾼다.
            self.status(FEED_SETUP_REQUIRED);
            self.announce_setup(SETUP_REQUIRED, setup.missing());
            self.nap(KEY_POLL).await;
        }
        self.ready_checked = Instant::now();
        self.desk.set_started(true);
        self.announce_setup(SETUP_STARTED, Vec::new());
    }

    /// 시세 받기를 멈췄다: 간직한 것을 버리고, 늦은 종목 · 거절된 종목 알림을 거둔다.
    fn halt(&mut self) {
        self.board.forget();
        self.announce_stale();
        self.remember_rejected(Vec::new());
        self.calendar_retry_at = None;
        self.quotes_log.clear();
    }

    /// 필수 설정을 다시 볼 때가 됐고, 보니 빠져 있는가.
    fn no_longer_ready(&mut self) -> bool {
        if self.ready_checked.elapsed() < KEY_POLL {
            return false;
        }
        self.ready_checked = Instant::now();
        !self.desk.ready()
    }

    /// 받아 보내기를 되풀이한다. 성공하면 시세 간격만큼, 실패하면 물러서며 기다린다.
    /// 필수 설정이 빠지면(증권사 키가 없다고 하면) 돌아온다.
    async fn pump(&mut self, adapter: &Arc<dyn MarketAdapter>, demo: bool) {
        let mut backoff = Backoff::new(self.pace.retry_initial, self.pace.retry_max);
        loop {
            if !demo && self.no_longer_ready() {
                return;
            }
            match self.honor_asked_wait(demo).await {
                Rested::Done => {}
                Rested::NotReady => return,
                Rested::Woken => {
                    backoff.reset();
                    self.recheck_rejected = true;
                }
            }
            self.adopt_symbols();
            let plan = self.plan();
            self.board.asked(&plan.asking(), self.clock.now());
            let fetched = self.fetch_watching(adapter, plan).await;
            let Step::Wait { wait, not_before } = self.settle(fetched, demo, &mut backoff) else {
                return;
            };
            match self.rest(wait, not_before, demo).await {
                Rested::Done => {}
                Rested::NotReady => return,
                Rested::Woken => {
                    // 등록이 깨웠다: 처음부터 다시 시도한다(거절된 종목도 다시 물어본다).
                    backoff.reset();
                    self.recheck_rejected = true;
                }
            }
        }
    }

    /// 증권사가 준 기다림이 아직 남았으면(흐름이 다시 시작한 뒤라도) 끝날 때까지 쉰다.
    async fn honor_asked_wait(&mut self, demo: bool) -> Rested {
        match self.asked_until {
            Some(until) if Instant::now() < until => {
                let left = until - Instant::now();
                self.rest(left, Some(until), demo).await
            }
            _ => Rested::Done,
        }
    }

    /// 지켜보는 종목이 바뀌었으면 받아들인다. 빠진 종목의 것은 버리고, 거절 기억은 처음부터다.
    /// (7차 3번) 시연인데 지켜볼 종목이 없으면 시연 종목 하나를 쓴다(저장하지 않는다).
    fn adopt_symbols(&mut self) {
        let mut symbols = self.desk.symbols();
        if symbols.is_empty() && self.desk.demo() {
            symbols.push(crate::demo::demo_symbol(self.market));
        }
        if symbols != self.symbols {
            self.symbols = symbols;
            self.board.keep_only(&self.symbols);
            self.remember_rejected(Vec::new());
        }
    }

    /// 이번에 무엇을 물을지 정한다.
    fn plan(&mut self) -> Plan {
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

    /// 받는다. 받는 동안에도 늦은 시세를 본다(요청이 끝나길 기다리느라 늦어지지 않는다).
    async fn fetch_watching(
        &mut self,
        adapter: &Arc<dyn MarketAdapter>,
        plan: Plan,
    ) -> Result<Fetched, MarketError> {
        let work = fetch::fetch(Arc::clone(adapter), plan);
        tokio::pin!(work);
        loop {
            tokio::select! {
                fetched = &mut work => return fetched,
                () = tokio::time::sleep(STALE_CHECK) => self.announce_stale(),
            }
        }
    }

    /// 받은 결과를 알리고 다음에 할 일을 정한다.
    fn settle(
        &mut self,
        fetched: Result<Fetched, MarketError>,
        demo: bool,
        backoff: &mut Backoff,
    ) -> Step {
        let fetched = match fetched {
            Ok(fetched) => fetched,
            Err(MarketError::NoCredentials) => return Step::Stop,
            Err(error) => return self.failed(&error, demo, backoff),
        };
        self.remember_rejected(fetched.rejected);
        if fetched.quotes.is_empty() {
            return self.nothing_came(demo, backoff);
        }
        self.board.hold(fetched.quotes);
        match fetched.calendar {
            Some((date, Ok(calendar))) => {
                self.board.set_calendar(date, calendar);
                self.calendar_retry_at = None;
                self.calendar_log.clear();
            }
            Some((_, Err(MarketError::NoCredentials))) => return Step::Stop,
            // 달력의 인증 실패도 키를 받지 않는 것이다: 시세 실패처럼 알리고 물러선다.
            Some((_, Err(error))) if failure_state(&error) == FEED_BROKER_REJECTED => {
                return self.failed(&error, demo, backoff);
            }
            Some((_, Err(error))) => self.calendar_failed(&error),
            None => {}
        }
        backoff.reset();
        if self.quotes_log.clear() {
            tracing::info!("시세를 다시 받습니다");
        }
        // 증권사 시세는 실제로 받은 이때에야 "증권사 시세"다.
        if !demo {
            self.status(FEED_LIVE);
        }
        self.publisher
            .publish_transient(KIND_QUOTES, self.board.quotes_payload(&self.symbols, demo));
        Step::wait(self.pace.interval.max(MIN_INTERVAL))
    }

    /// 거절된 종목을 기억하고, 바뀌었으면 화면에 알린다(없어지면 빈 목록을 알린다).
    fn remember_rejected(&mut self, rejected: Vec<Symbol>) {
        if rejected != self.rejected {
            let symbols: Vec<&str> = rejected.iter().map(Symbol::as_str).collect();
            self.publisher
                .publish_transient(KIND_SYMBOLS_REJECTED, json!({ "symbols": symbols }));
            if !rejected.is_empty() {
                tracing::warn!(
                    count = rejected.len(),
                    "증권사가 거절한 종목이 있습니다. 그 종목은 빼고 받습니다(종목을 확인하세요)"
                );
            }
        }
        if rejected.is_empty() {
            self.rejected_since = None;
        } else if self.rejected_since.is_none() || self.recheck_rejected {
            self.rejected_since = Some(Instant::now());
        }
        self.recheck_rejected = false;
        self.rejected = rejected;
    }

    /// 요청은 됐는데 시세가 하나도 오지 않았다: 성공이 아니다. 물러선다.
    fn nothing_came(&mut self, demo: bool, backoff: &mut Backoff) -> Step {
        if demo {
            // 시연에서 종목이 없으면 보낼 것이 없을 뿐이다.
            return Step::wait(self.pace.interval.max(MIN_INTERVAL));
        }
        // 거절을 봤고 시세가 하나도 없으면, 다 가려내기 전이라도 요청 거절이다(4차).
        let state = if !self.rejected.is_empty() {
            FEED_REQUEST_REJECTED
        } else {
            FEED_CONNECTING
        };
        self.status(state);
        let wait = backoff.next_delay();
        if self.quotes_log.should_log(state, std::time::Instant::now()) {
            tracing::warn!(
                state,
                retry_in_ms = millis(wait),
                "증권사가 시세를 하나도 주지 않았습니다. 간격을 늘려 다시 시도합니다(같은 일은 1분에 한 번만 기록합니다)"
            );
        }
        Step::wait(wait)
    }

    /// 시세(또는 달력의 인증)를 받지 못했다: 까닭을 알리고(바뀌었을 때만), 기록은 묶어서 남긴다.
    /// 증권사가 기다림(`Retry-After`)을 줬으면 그보다 빨리 다시 시도하지 않는다.
    fn failed(&mut self, error: &MarketError, demo: bool, backoff: &mut Backoff) -> Step {
        let state = failure_state(error);
        if !demo {
            self.status(state);
        }
        let asked = match error {
            MarketError::RateLimited { retry_after } => retry_after.map(|d| d.min(MAX_ASKED_WAIT)),
            _ => None,
        };
        let wait = backoff.next_delay().max(asked.unwrap_or_default());
        if let Some(asked) = asked {
            self.asked_until = Some(Instant::now() + asked);
        }
        if self.quotes_log.should_log(state, std::time::Instant::now()) {
            tracing::warn!(
                %error,
                retry_in_ms = millis(wait),
                "시세를 받지 못했습니다. 간격을 늘려 다시 시도합니다(같은 실패는 1분에 한 번만 기록합니다)"
            );
        }
        Step::Wait {
            wait,
            not_before: asked.map(|asked| Instant::now() + asked),
        }
    }

    /// 달력을 받지 못했다(키 문제는 아니다): 시세는 이어 받고, 달력은 정한 간격 뒤에 다시 받는다.
    fn calendar_failed(&mut self, error: &MarketError) {
        if self
            .calendar_log
            .should_log("calendar", std::time::Instant::now())
        {
            tracing::warn!(%error, "장 달력을 받지 못했습니다(같은 실패는 1분에 한 번만 기록합니다)");
        }
        self.calendar_retry_at = Some(Instant::now() + self.pace.calendar_retry);
    }

    /// `wait`만큼 쉰다. 쉬는 동안에도 늦은 시세와 필수 설정을 본다.
    /// 등록이 깨우면 남은 시간을 기다리지 않는다(`not_before`보다 빠르지는 않다).
    async fn rest(&mut self, wait: Duration, not_before: Option<Instant>, demo: bool) -> Rested {
        let mut until = Instant::now() + wait;
        let mut woken = false;
        loop {
            self.announce_stale();
            let now = Instant::now();
            if now >= until {
                return if woken { Rested::Woken } else { Rested::Done };
            }
            if !demo && self.no_longer_ready() {
                return Rested::NotReady;
            }
            let desk = Arc::clone(&self.desk);
            tokio::select! {
                () = tokio::time::sleep((until - now).min(STALE_CHECK)) => {}
                () = desk.woken(), if !woken => {
                    woken = true;
                    until = not_before.unwrap_or(now).min(until);
                }
            }
        }
    }

    /// 늦은 종목이 바뀌었으면 알린다(N6).
    fn announce_stale(&mut self) {
        let now = self.clock.now();
        let notice = self.board.stale_notice(&self.symbols, &self.rejected, now);
        if let Some(notice) = notice {
            self.publisher.publish_transient(KIND_QUOTES_STALE, notice);
        }
    }
}
