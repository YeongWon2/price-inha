//! 3차(2차 검토의 참고): 띄운 깨우기의 패닉, 기다리지 않고 끝난 결과 가져가기, tick이 중간에 빠져도
//! 되살릴 회차를 잃지 않기, 실행 중인 작업의 회차를 대기 계산에서 빼기, 되살린 회차의 `pending_slot` 지우기,
//! 시세를 받는 동안 쓰기 잠금을 쥐지 않기.
//!
//! 실제로 잠들지 않는다. 오래 걸리는 깨우기 · 시세는 가짜가 붙잡아 두고, 기다림은 차례 넘기기 횟수로 막는다.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use chrono::NaiveDate;
use pi_market::{
    Clock, FakeAdapter, Market, MarketAdapter, MarketCalendar, MarketError, Quote, Symbol,
};
use pi_trigger::{
    ConditionKind, ConditionMemory, Direction, Error, JobId, JobState, MAX_TIMER, ReceiptStatus,
    ScheduleKind, Target, TickOutcome, Trigger, TriggerConfig, TriggerEvent, Wake, WakeError,
    Waker,
};
use tokio::sync::{Notify, watch};

use crate::support::{
    Harness, condition, config, d, kr_next_week_day, quote, schedule, secs, settled_tick, utc,
};

fn before_open() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 15, 23, 59, 0)
}

fn open_slot() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 16, 0, 0, 0)
}

fn next_open_slot() -> chrono::DateTime<chrono::Utc> {
    utc(2026, 11, 17, 0, 0, 0)
}

fn open_job() -> Vec<pi_trigger::ScheduleJob> {
    vec![schedule("open", ScheduleKind::MarketOpen, Target::Team)]
}

/// 조건이 참이 될 때까지 다른 작업에 차례를 넘긴다(실제로 잠들지 않는다).
async fn until<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..1_000 {
        if check().await {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("기다려도 되지 않았다: {what}");
}

/// 상태를 그 회차로 옮기고 발사하기로 적어 둔 모습(영수증은 아직 없다).
fn moved(job: &str, slot: chrono::DateTime<chrono::Utc>) -> JobState {
    let mut state = JobState::new(JobId::new(job), Some(slot));
    state.pending_slot = Some(slot);
    state
}

/// 깨운 사건들 가운데 그 작업 · 회차의 일정 사건 수.
fn schedule_events(wakes: &[Wake], job: &str, at: chrono::DateTime<chrono::Utc>) -> usize {
    wakes
        .iter()
        .flat_map(|wake| wake.events.iter())
        .filter(|event| {
            matches!(
                event,
                TriggerEvent::Schedule { job: id, slot, .. } if *id == JobId::new(job) && *slot == at
            )
        })
        .count()
}

/// 첫 깨우기에서 패닉하고, 그 뒤로는 성공하는 대상.
#[derive(Default)]
struct PanickingWaker {
    panicked: AtomicBool,
    calls: AtomicUsize,
}

#[async_trait]
impl Waker for PanickingWaker {
    async fn wake(&self, _wake: Wake) -> Result<(), WakeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.panicked.swap(true, Ordering::SeqCst) {
            panic!("가짜 깨우기가 패닉함");
        }
        Ok(())
    }
}

/// 시세 받기를 붙잡아 둘 수 있고, 달력 받기를 실패시킬 수 있는 시장(안은 [`FakeAdapter`]).
struct GatedMarket {
    inner: Arc<FakeAdapter>,
    hold_quotes: AtomicBool,
    fail_calendar: AtomicBool,
    entered: Notify,
    release: Notify,
}

impl GatedMarket {
    fn new(inner: Arc<FakeAdapter>) -> Self {
        Self {
            inner,
            hold_quotes: AtomicBool::new(false),
            fail_calendar: AtomicBool::new(false),
            entered: Notify::new(),
            release: Notify::new(),
        }
    }
}

#[async_trait]
impl MarketAdapter for GatedMarket {
    fn market(&self) -> Market {
        Market::Kr
    }

    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
        if self.hold_quotes.load(Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        self.inner.quotes(symbols).await
    }

    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError> {
        if self.fail_calendar.load(Ordering::SeqCst) {
            return Err(MarketError::Unavailable {
                status: 503,
                code: None,
            });
        }
        self.inner.calendar(date).await
    }
}

/// 하네스의 포트에 시장만 바꿔 끼운 엔진.
fn gated_trigger(h: &Harness, market: &Arc<GatedMarket>, config: TriggerConfig) -> Trigger {
    let mut ports = h.ports();
    let market: Arc<dyn MarketAdapter> = market.clone();
    ports.markets = vec![market];
    Trigger::new(config, ports).unwrap()
}

// ---------------------------------------------------------------------------------------------
// 3차 1: 띄운 깨우기가 패닉해도 영수증을 실패로 닫고 in_flight를 줄인다.

// 검증: E16
#[tokio::test]
async fn 띄운_깨우기가_패닉해도_영수증을_실패로_닫고_in_flight를_줄인다() {
    let h = Harness::new(before_open());
    let waker = Arc::new(PanickingWaker::default());
    let mut ports = h.ports();
    ports.waker = waker.clone();
    let trigger = Trigger::new(config(open_job(), vec![]), ports).unwrap();
    trigger.tick().await.unwrap();
    // 처음 tick은 상태만 만든다(아무도 깨우지 않는다).
    let _ = trigger.settle().await;
    assert_eq!(waker.calls.load(Ordering::SeqCst), 0);

    h.clock.set(open_slot());
    let report = trigger.tick().await.unwrap();
    assert_eq!(report.wakes.len(), 1);
    let _ = trigger.settle().await;
    until("패닉한 깨우기도 끝난 것으로 센다", || async {
        trigger.in_flight() == 0
    })
    .await;

    let receipt = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(
        receipt.status,
        ReceiptStatus::Error,
        "패닉한 깨우기의 영수증은 실패로 닫는다"
    );
    assert!(receipt.finished_at.is_some());
    assert!(
        h.store.running().await.is_empty(),
        "실행 중으로 남은 영수증이 없다"
    );

    // 그 작업이 막히지 않는다: 다음 회차는 그대로 나간다.
    h.market.set_calendar(kr_next_week_day());
    h.clock.set(utc(2026, 11, 17, 0, 0, 5));
    trigger.tick().await.unwrap();
    let _ = trigger.settle().await;
    assert_eq!(waker.calls.load(Ordering::SeqCst), 2);
    let next = h.store.receipt("open", next_open_slot()).await.unwrap();
    assert_eq!(next.status, ReceiptStatus::Ok);
    assert_eq!(trigger.in_flight(), 0);
}

// ---------------------------------------------------------------------------------------------
// 3차 2: 끝난 결과(finished · errors)를 기다리지 않고 가져가는 길, run()이 그것으로 비운다.

// 검증: E16
#[tokio::test]
async fn 끝난_결과는_기다리지_않고_가져가며_한_번만_나온다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(open_job(), vec![]));
    settled_tick(&trigger).await;

    h.waker.hold(true);
    h.clock.set(open_slot());
    let report = trigger.tick().await.unwrap();
    assert_eq!(report.wakes.len(), 1);
    h.waker.entered().await;

    // 아직 끝나지 않았다: 기다리지 않고 빈 것을 돌려준다.
    let drained = trigger.drain_finished();
    assert!(drained.finished.is_empty());
    assert!(drained.errors.is_empty());
    assert_eq!(trigger.in_flight(), 1);

    h.waker.release();
    until("붙잡힌 깨우기가 끝난다", || async {
        trigger.in_flight() == 0
    })
    .await;

    let drained = trigger.drain_finished();
    assert_eq!(drained.finished.len(), 1);
    assert!(drained.finished[0].ok);
    assert_eq!(drained.finished[0].wake, report.wakes[0]);
    assert!(drained.errors.is_empty());

    // 한 번 낸 것은 다시 내지 않는다(가져가기에도 settle에도).
    assert_eq!(trigger.drain_finished(), pi_trigger::Drained::default());
    let settled = trigger.settle().await.unwrap();
    assert!(settled.finished.is_empty());
}

// 검증: E16
#[tokio::test]
async fn 반영하다_생긴_저장소_오류도_기다리지_않고_가져가며_한_번만_나온다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(open_job(), vec![]));
    settled_tick(&trigger).await;

    h.waker.hold(true);
    h.clock.set(open_slot());
    trigger.tick().await.unwrap();
    h.waker.entered().await;
    h.store.fail_writes(true);
    h.waker.release();
    until("붙잡힌 깨우기가 끝난다", || async {
        trigger.in_flight() == 0
    })
    .await;

    let drained = trigger.drain_finished();
    assert_eq!(drained.finished.len(), 1);
    assert!(
        !drained.errors.is_empty(),
        "결과를 적지 못한 저장소 오류를 낸다"
    );
    let again = trigger.drain_finished();
    assert!(again.finished.is_empty());
    assert!(again.errors.is_empty(), "가져간 오류는 다시 나오지 않는다");

    h.store.fail_writes(false);
    let settled = trigger.settle().await;
    assert!(
        matches!(&settled, Ok(report) if report.finished.is_empty()),
        "가져간 오류 · 결과는 settle에 다시 나오지 않는다: {settled:?}"
    );
}

// 검증: E16
#[tokio::test]
async fn run은_끝난_결과를_비워_쌓아_두지_않는다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(open_job(), vec![]));
    settled_tick(&trigger).await;

    h.clock.set(open_slot());
    trigger.tick().await.unwrap();
    until("깨우기가 끝난다", || async {
        trigger.in_flight() == 0
    })
    .await;

    // 끝난 결과 하나가 가져가지 않은 채 있다. run이 돌면서 비운다.
    let (stop_tx, stop_rx) = watch::channel(false);
    h.clock.stop_after(2, stop_tx);
    trigger.run(stop_rx).await.unwrap();

    let settled = trigger.settle().await.unwrap();
    assert!(
        settled.finished.is_empty(),
        "run이 끝난 결과를 비워야 한다: {:?}",
        settled.finished
    );
}

// 검증: E16
#[tokio::test]
async fn run은_반영하다_생긴_저장소_오류도_비운다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(open_job(), vec![]));
    settled_tick(&trigger).await;

    h.waker.hold(true);
    h.clock.set(open_slot());
    trigger.tick().await.unwrap();
    h.waker.entered().await;
    h.store.fail_writes(true);
    h.waker.release();
    until("붙잡힌 깨우기가 끝난다", || async {
        trigger.in_flight() == 0
    })
    .await;
    h.store.fail_writes(false);
    h.waker.hold(false);

    let (stop_tx, stop_rx) = watch::channel(false);
    h.clock.stop_after(2, stop_tx);
    trigger.run(stop_rx).await.unwrap();

    let settled = trigger.settle().await;
    assert!(
        matches!(&settled, Ok(report) if report.finished.is_empty()),
        "run이 끝난 결과와 오류를 비워야 한다: {settled:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 3차 3: tick이 중간에 빠져도 꺼낸 되살릴 회차를 잃지 않는다(달력이 없을 때 포함).

// 검증: E16
#[tokio::test]
async fn tick이_중간에_저장소_오류로_빠져도_되살릴_회차를_잃지_않는다() {
    let now = utc(2026, 11, 16, 0, 5, 0);
    let h = Harness::new(now);
    h.store.inner_save(&moved("open", open_slot())).await;
    // 알린 뒤 반대로 0.5% 넘게 내려와 다시 걸리는 시세 조건: 그 상태 저장이 tick 중간에 실패한다.
    let mut px = JobState::new(JobId::new("px"), Some(now));
    px.condition = Some(ConditionMemory::Fired {
        notified: d("100"),
        direction: Direction::Up,
    });
    h.store.inner_save(&px).await;
    h.market.set_quote(quote("005930", "99", now));
    let trigger = h.trigger(config(
        open_job(),
        vec![condition("px", "005930", ConditionKind::Above(d("100")))],
    ));

    let started = trigger.start().await.unwrap();
    assert_eq!(started.revived.len(), 1);

    h.store.fail_writes(true);
    let failed = trigger.tick().await;
    assert!(
        matches!(failed, Err(Error::Store(_))),
        "tick이 중간에 저장소 오류로 빠진다: {failed:?}"
    );
    let _ = trigger.settle().await;

    h.store.fail_writes(false);
    h.clock.advance(secs(10));
    h.market.set_quote(quote("005930", "99", h.clock.now()));
    settled_tick(&trigger).await;

    assert_eq!(
        schedule_events(&h.waker.wakes(), "open", open_slot()),
        1,
        "되살린 회차는 다음 tick에 한 번 나간다: {:?}",
        h.waker.wakes()
    );
    let receipt = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Ok);
}

// 검증: E16
#[tokio::test]
async fn 달력을_받지_못한_tick_뒤에도_되살릴_회차를_잃지_않는다() {
    let now = utc(2026, 11, 16, 0, 5, 0);
    let h = Harness::new(now);
    h.store.inner_save(&moved("open", open_slot())).await;
    let market = Arc::new(GatedMarket::new(h.market.clone()));
    let trigger = gated_trigger(&h, &market, config(open_job(), vec![]));

    let started = trigger.start().await.unwrap();
    assert_eq!(started.revived.len(), 1);

    market.fail_calendar.store(true, Ordering::SeqCst);
    let _ = trigger.tick().await;
    let _ = trigger.settle().await;
    assert_eq!(h.waker.count(), 0, "달력이 없으면 일정은 나가지 않는다");

    market.fail_calendar.store(false, Ordering::SeqCst);
    h.clock.advance(secs(10));
    settled_tick(&trigger).await;

    assert_eq!(
        schedule_events(&h.waker.wakes(), "open", open_slot()),
        1,
        "달력이 돌아오면 되살린 회차가 한 번 나간다: {:?}",
        h.waker.wakes()
    );
    let receipt = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Ok);
}

// ---------------------------------------------------------------------------------------------
// 3차 4: 실행 중인 작업의 회차는 다음 대기 계산에서 뺀다(2초마다 tick이 돌지 않게).

// 검증: E16 E18
#[tokio::test]
async fn 같은_작업이_실행_중이라_남은_회차는_다음_대기_계산에서_뺀다() {
    let h = Harness::new(before_open());
    let trigger = h.trigger(config(open_job(), vec![]));
    settled_tick(&trigger).await;

    h.waker.hold(true);
    h.clock.set(open_slot());
    let first = trigger.tick().await.unwrap();
    assert_eq!(first.wakes.len(), 1);
    h.waker.entered().await;

    // 다음 날 회차가 기한이 되었는데 앞 회차가 아직 실행 중이다.
    h.market.set_calendar(kr_next_week_day());
    h.clock.set(utc(2026, 11, 17, 0, 0, 5));
    let blocked = trigger.tick().await.unwrap();
    assert_eq!(blocked.outcome, TickOutcome::Ran);
    assert!(
        blocked.wakes.is_empty(),
        "같은 작업이 실행 중이라 깨우지 않는다"
    );
    assert_eq!(
        blocked.next_delay, MAX_TIMER,
        "실행 중인 작업의 회차로 2초마다 돌지 않는다"
    );

    // 앞 회차가 끝나면 남은 회차는 그대로 나간다(빼기만 했지 잃지 않는다).
    h.waker.hold(false);
    h.waker.release();
    trigger.settle().await.unwrap();
    h.clock.advance(secs(5));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 2);
    let next = h.store.receipt("open", next_open_slot()).await.unwrap();
    assert_eq!(next.status, ReceiptStatus::Ok);
}

// ---------------------------------------------------------------------------------------------
// 3차 5: 되살린 회차가 발사되면 pending_slot을 지운다.

// 검증: E16
#[tokio::test]
async fn 되살린_회차가_나가면_pending_slot을_지워_영수증이_정리돼도_다시_되살리지_않는다() {
    let now = utc(2026, 11, 16, 0, 5, 0);
    let h = Harness::new(now);
    h.store.inner_save(&moved("open", open_slot())).await;
    let trigger = h.trigger(config(open_job(), vec![]));
    assert_eq!(trigger.start().await.unwrap().revived.len(), 1);
    settled_tick(&trigger).await;
    assert_eq!(schedule_events(&h.waker.wakes(), "open", open_slot()), 1);

    let state = h.store.job("open").await;
    assert_eq!(state.pending_slot, None, "되살린 회차가 나갔으면 지운다");
    assert_eq!(state.last_slot, Some(open_slot()));

    // 영수증이 정리된 저장소(작업 상태만 남음)로 다시 켜도 옛 회차를 되살리지 않는다.
    let fresh = Harness::new(h.clock.now());
    fresh.store.inner_save(&state).await;
    let again = fresh.trigger(config(open_job(), vec![]));
    assert!(again.start().await.unwrap().revived.is_empty());
    settled_tick(&again).await;
    assert_eq!(fresh.waker.count(), 0);
}

// ---------------------------------------------------------------------------------------------
// 3차 6: 시세를 받는 동안에는 쓰기 잠금을 쥐지 않는다.

// 검증: N6 E16
#[tokio::test]
async fn 시세를_받는_동안에도_끝난_깨우기의_결과가_반영된다() {
    let h = Harness::new(before_open());
    let market = Arc::new(GatedMarket::new(h.market.clone()));
    let trigger = Arc::new(gated_trigger(
        &h,
        &market,
        config(
            open_job(),
            vec![condition(
                "px",
                "005930",
                ConditionKind::Above(d("1000000")),
            )],
        ),
    ));
    settled_tick(&trigger).await;

    h.waker.hold(true);
    h.clock.set(open_slot());
    h.market.set_quote(quote("005930", "100", open_slot()));
    let first = trigger.tick().await.unwrap();
    assert_eq!(first.wakes.len(), 1);
    h.waker.entered().await;

    // 다음 tick이 시세를 받다가 붙잡힌다.
    market.hold_quotes.store(true, Ordering::SeqCst);
    h.clock.advance(secs(10));
    h.market.set_quote(quote("005930", "100", h.clock.now()));
    let ticking = Arc::clone(&trigger);
    let second = tokio::spawn(async move { ticking.tick().await });
    market.entered.notified().await;

    // 그동안 앞 깨우기가 끝나면 영수증이 닫힌다(쓰기 잠금이 시세 받기에 묶이지 않는다).
    h.waker.release();
    until(
        "시세를 받는 동안에도 끝난 깨우기가 반영된다",
        || async { trigger.in_flight() == 0 },
    )
    .await;
    let receipt = h.store.receipt("open", open_slot()).await.unwrap();
    assert_eq!(receipt.status, ReceiptStatus::Ok);

    market.hold_quotes.store(false, Ordering::SeqCst);
    market.release.notify_one();
    let report = second.await.unwrap().unwrap();
    assert_eq!(report.outcome, TickOutcome::Ran);
    trigger.settle().await.unwrap();
}
