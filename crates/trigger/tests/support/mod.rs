//! 시험용 가짜들: 시계 · 정지 표시 · 깨울 대상 · 실패하는 저장소 · 달력.
//! 실제로 잠들지 않는다. 시계의 `sleep`은 시각만 앞으로 옮긴다.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{
    Clock, Currency, FakeAdapter, Market, MarketAdapter, MarketCalendar, Quote, Session,
    SessionKind, Symbol, TradingDay,
};
use pi_trigger::{
    ConditionJob, ConditionKind, JobId, JobState, MemoryStore, Monotonic, OpenReceipt, Ports,
    Receipt, ReceiptKey, ReceiptStatus, ScheduleJob, ScheduleKind, ScheduleTimes, StopSignal,
    StoreError, Target, Trigger, TriggerConfig, TriggerStore, Wake, WakeError, Waker,
};
use rust_decimal::Decimal;
use tokio::sync::{Notify, watch};

/// 십진수.
pub fn d(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

/// UTC 시각.
pub fn utc(y: i32, mo: u32, da: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, da, h, mi, s).unwrap()
}

/// 날짜.
pub fn date(y: i32, mo: u32, da: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, mo, da).unwrap()
}

/// 국내 영업일: 정규장 09:00~15:30 KST = 00:00~06:30 UTC.
pub fn kr_day(day: NaiveDate) -> TradingDay {
    let start = utc_on(day, 0, 0);
    let end = utc_on(day, 6, 30);
    TradingDay {
        date: day,
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start,
            end,
        }],
    }
}

/// 휴장일.
pub fn holiday(day: NaiveDate) -> TradingDay {
    TradingDay {
        date: day,
        sessions: Vec::new(),
    }
}

fn utc_on(day: NaiveDate, h: u32, mi: u32) -> DateTime<Utc> {
    day.and_hms_opt(h, mi, 0).unwrap().and_utc()
}

/// 국내 달력.
pub fn kr_calendar(prev: TradingDay, today: TradingDay, next: TradingDay) -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        today,
        previous_business_day: prev,
        next_business_day: next,
    }
}

/// 2026-11-13(금) · 2026-11-16(월) · 2026-11-17(화)의 국내 달력.
pub fn kr_week() -> MarketCalendar {
    kr_calendar(
        kr_day(date(2026, 11, 13)),
        kr_day(date(2026, 11, 16)),
        kr_day(date(2026, 11, 17)),
    )
}

/// 2026-11-16 · 11-17 · 11-18의 국내 달력(하루 뒤로 옮긴 것).
pub fn kr_next_week_day() -> MarketCalendar {
    kr_calendar(
        kr_day(date(2026, 11, 16)),
        kr_day(date(2026, 11, 17)),
        kr_day(date(2026, 11, 18)),
    )
}

/// 시세.
pub fn quote(symbol: &str, price: &str, received_at: DateTime<Utc>) -> Quote {
    Quote {
        symbol: Symbol::new(symbol),
        price: d(price),
        currency: Currency::Krw,
        exchange_time: None,
        received_at,
    }
}

/// 가짜 시계. 벽시계와 단조 시계를 따로 움직일 수 있다. `sleep`은 둘 다 그만큼 앞으로 옮기고 기록한다.
pub struct FakeClock {
    wall: Mutex<DateTime<Utc>>,
    mono: Mutex<Duration>,
    sleeps: Mutex<Vec<Duration>>,
    stop_after: Mutex<Option<(usize, watch::Sender<bool>)>>,
}

impl FakeClock {
    pub fn new(at: DateTime<Utc>) -> Self {
        Self {
            wall: Mutex::new(at),
            mono: Mutex::new(Duration::from_secs(1_000)),
            sleeps: Mutex::new(Vec::new()),
            stop_after: Mutex::new(None),
        }
    }

    /// 벽시계와 단조 시계를 함께 앞으로.
    pub fn advance(&self, by: Duration) {
        *self.wall.lock().unwrap() += chrono::Duration::from_std(by).unwrap();
        *self.mono.lock().unwrap() += by;
    }

    /// 벽시계를 그 시각으로 옮기고 단조 시계는 `mono_by`만 앞으로(시계 맞춤 · 절전).
    pub fn jump_wall(&self, to: DateTime<Utc>, mono_by: Duration) {
        *self.wall.lock().unwrap() = to;
        *self.mono.lock().unwrap() += mono_by;
    }

    /// 벽시계와 단조 시계를 함께 그 시각으로(앞으로만).
    pub fn set(&self, to: DateTime<Utc>) {
        let now = *self.wall.lock().unwrap();
        let by = (to - now).to_std().unwrap();
        self.advance(by);
    }

    pub fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.lock().unwrap().clone()
    }

    /// `sleep`이 n번 불리면 `stop`에 참을 보낸다.
    pub fn stop_after(&self, n: usize, stop: watch::Sender<bool>) {
        *self.stop_after.lock().unwrap() = Some((n, stop));
    }
}

#[async_trait]
impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.wall.lock().unwrap()
    }

    async fn sleep(&self, duration: Duration) {
        self.advance(duration);
        let count = {
            let mut sleeps = self.sleeps.lock().unwrap();
            sleeps.push(duration);
            sleeps.len()
        };
        if let Some((n, stop)) = &*self.stop_after.lock().unwrap()
            && count >= *n
        {
            stop.send(true).unwrap();
        }
    }
}

impl Monotonic for FakeClock {
    fn elapsed(&self) -> Duration {
        *self.mono.lock().unwrap()
    }
}

/// 전체 정지 표시.
#[derive(Default)]
pub struct FakeStop(AtomicBool);

impl FakeStop {
    pub fn set(&self, stopped: bool) {
        self.0.store(stopped, Ordering::SeqCst);
    }
}

impl StopSignal for FakeStop {
    fn is_stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// 깨우는 순간의 저장소 모습.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub jobs: Vec<JobState>,
    pub receipts: Vec<Receipt>,
}

/// 깨우기를 적어 두는 대상. 실패시킬 수 있고, 잠깐 붙잡아 둘 수도 있다.
#[derive(Default)]
pub struct RecordingWaker {
    wakes: Mutex<Vec<Wake>>,
    fail: AtomicBool,
    hold: AtomicBool,
    entered: Notify,
    release: Notify,
    /// 깨우는 동안 들여다볼 저장소와, 깨울 때마다 본 모습(작업 상태들, 영수증들).
    observed: Mutex<Option<Arc<FlakyStore>>>,
    snapshots: Mutex<Vec<Snapshot>>,
    calls: AtomicUsize,
}

impl RecordingWaker {
    pub fn wakes(&self) -> Vec<Wake> {
        self.wakes.lock().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn fail(&self, fail: bool) {
        self.fail.store(fail, Ordering::SeqCst);
    }

    /// 참이면 `release`가 불릴 때까지 깨우기를 끝내지 않는다.
    pub fn hold(&self, hold: bool) {
        self.hold.store(hold, Ordering::SeqCst);
    }

    /// 붙잡힌 깨우기가 들어올 때까지 기다린다.
    pub async fn entered(&self) {
        self.entered.notified().await;
    }

    pub fn release(&self) {
        self.release.notify_one();
    }

    /// 깨울 때마다 이 저장소의 모습을 적어 둔다.
    pub fn observe(&self, store: Arc<FlakyStore>) {
        *self.observed.lock().unwrap() = Some(store);
    }

    pub fn snapshots(&self) -> Vec<Snapshot> {
        self.snapshots.lock().unwrap().clone()
    }
}

#[async_trait]
impl Waker for RecordingWaker {
    async fn wake(&self, wake: Wake) -> Result<(), WakeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let observed = self.observed.lock().unwrap().clone();
        if let Some(store) = observed {
            let snapshot = Snapshot {
                jobs: store.inner.jobs().await.unwrap(),
                receipts: store.inner.receipts().await.unwrap(),
            };
            self.snapshots.lock().unwrap().push(snapshot);
        }
        self.wakes.lock().unwrap().push(wake);
        if self.hold.load(Ordering::SeqCst) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        if self.fail.load(Ordering::SeqCst) {
            return Err(WakeError("깨운 실행이 실패함".to_string()));
        }
        Ok(())
    }
}

/// 쓰기를 실패시킬 수 있는 저장소(안은 [`MemoryStore`]).
#[derive(Default)]
pub struct FlakyStore {
    pub inner: MemoryStore,
    fail_writes: AtomicBool,
}

impl FlakyStore {
    pub fn fail_writes(&self, fail: bool) {
        self.fail_writes.store(fail, Ordering::SeqCst);
    }

    fn check(&self) -> Result<(), StoreError> {
        if self.fail_writes.load(Ordering::SeqCst) {
            return Err(StoreError("쓸 수 없음".to_string()));
        }
        Ok(())
    }

    /// 실패 표시와 상관없이 작업 상태를 저장한다(사람 · 다른 프로세스가 바꾼 것처럼).
    pub async fn inner_save(&self, job: &JobState) {
        self.inner.save_job(job).await.unwrap();
    }

    /// 작업 상태 하나(없으면 패닉).
    pub async fn job(&self, id: &str) -> JobState {
        self.inner
            .jobs()
            .await
            .unwrap()
            .into_iter()
            .find(|job| job.id == JobId::new(id))
            .unwrap()
    }

    /// 영수증 하나(없으면 `None`).
    pub async fn receipt(&self, id: &str, slot: DateTime<Utc>) -> Option<Receipt> {
        let key = ReceiptKey {
            job: JobId::new(id),
            slot,
        };
        self.inner
            .receipts()
            .await
            .unwrap()
            .into_iter()
            .find(|receipt| receipt.key == key)
    }
}

#[async_trait]
impl TriggerStore for FlakyStore {
    async fn jobs(&self) -> Result<Vec<JobState>, StoreError> {
        self.inner.jobs().await
    }

    async fn save_job(&self, job: &JobState) -> Result<(), StoreError> {
        self.check()?;
        self.inner.save_job(job).await
    }

    async fn open_receipt(
        &self,
        key: &ReceiptKey,
        started_at: DateTime<Utc>,
    ) -> Result<OpenReceipt, StoreError> {
        self.check()?;
        self.inner.open_receipt(key, started_at).await
    }

    async fn mark_delivery_started(&self, key: &ReceiptKey) -> Result<(), StoreError> {
        self.check()?;
        self.inner.mark_delivery_started(key).await
    }

    async fn close_receipt(
        &self,
        key: &ReceiptKey,
        status: ReceiptStatus,
        finished_at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        self.check()?;
        self.inner.close_receipt(key, status, finished_at).await
    }

    async fn receipts(&self) -> Result<Vec<Receipt>, StoreError> {
        self.inner.receipts().await
    }
}

/// 엔진과 가짜들을 한데 묶은 것.
pub struct Harness {
    pub clock: Arc<FakeClock>,
    pub market: Arc<FakeAdapter>,
    pub store: Arc<FlakyStore>,
    pub waker: Arc<RecordingWaker>,
    pub stop: Arc<FakeStop>,
}

impl Harness {
    /// 그 시각, 국내 달력 [`kr_week`].
    pub fn new(at: DateTime<Utc>) -> Self {
        let market = Arc::new(FakeAdapter::new(Market::Kr));
        market.set_calendar(kr_week());
        Self {
            clock: Arc::new(FakeClock::new(at)),
            market,
            store: Arc::new(FlakyStore::default()),
            waker: Arc::new(RecordingWaker::default()),
            stop: Arc::new(FakeStop::default()),
        }
    }

    pub fn ports(&self) -> Ports {
        let market: Arc<dyn MarketAdapter> = self.market.clone();
        Ports {
            clock: self.clock.clone(),
            monotonic: self.clock.clone(),
            markets: vec![market],
            store: self.store.clone(),
            waker: self.waker.clone(),
            stop: self.stop.clone(),
        }
    }

    pub fn trigger(&self, config: TriggerConfig) -> Trigger {
        Trigger::new(config, self.ports()).unwrap()
    }
}

/// 일정 작업.
pub fn schedule(id: &str, kind: ScheduleKind, target: Target) -> ScheduleJob {
    ScheduleJob {
        id: JobId::new(id),
        market: Market::Kr,
        kind,
        target,
    }
}

/// 시세 조건 작업(대상: 담당 `price`).
pub fn condition(id: &str, symbol: &str, kind: ConditionKind) -> ConditionJob {
    ConditionJob {
        id: JobId::new(id),
        market: Market::Kr,
        symbol: Symbol::new(symbol),
        kind,
        target: Target::Member("price".to_string()),
    }
}

/// 설정: 숫자는 모두 시험이 정한다(브리핑 30분 전 · 마감 뒤 30분 · 다시 걸기 0.5% · 늦음 30초 · 시세 10초).
pub fn config(schedules: Vec<ScheduleJob>, conditions: Vec<ConditionJob>) -> TriggerConfig {
    let mut config = TriggerConfig::new(schedules, conditions);
    config.times = ScheduleTimes {
        briefing_lead: Duration::from_secs(30 * 60),
        after_close_delay: Duration::from_secs(30 * 60),
    };
    config.rearm_percent = d("0.5");
    config.stale = pi_market::StalePolicy {
        max_age: Duration::from_secs(30),
    };
    config.quote_interval = Duration::from_secs(10);
    config.stale_target = Target::Team;
    config
}

/// 초.
pub fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}
