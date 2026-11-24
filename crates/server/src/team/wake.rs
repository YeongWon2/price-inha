//! 깨우기 엔진 잇기(T11): 장 시작에 하루 운용 판단 · 새 공시 보기, 마감 뒤에 하루 복기.
//!
//! - 작업 상태 · 영수증은 저장소에 둔다([`StoreTriggers`], 다시 켜도 같은 회차를 두 번 돌리지 않는다, E16).
//! - 달력은 주문 길과 같은 것을 쓴다([`CalendarMarket`]). (10b 8번) 시세 조건의 시세는 시세 흐름이 받아 주문 길에
//!   넘긴 마지막 시세다(바깥에 따로 묻지 않는다). 조건 목록 · 자료 사건은 [`super::poll::Poller`]가 넣는다.
//! - 뉴스 공시 시세 담당을 깨운 시세 조건 · 자료 변화는 종목마다 한 번 논의한다([`Crew::on_scout`]).
//! - `Trigger::run`만 돌린다. `Trigger::settle`은 `select!` 안에 두지 않는다(`run`이 되풀이마다
//!   `drain_finished`로 끝난 결과를 가져간다).

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{Clock, Market, MarketAdapter, MarketCalendar, MarketError, Quote, Symbol};
use pi_store::{
    OpenTriggerReceipt, Store, TeamState, TriggerDelivery, TriggerReceiptKey, TriggerReceiptStatus,
    UnixMillis,
};
use pi_trigger::{
    DeliveryState, DisableReason, JobId, JobState, Monotonic, OpenReceipt, Ports, Receipt,
    ReceiptKey, ReceiptStatus, ScheduleJob, ScheduleKind, StopSignal, StoreError, Target, Trigger,
    TriggerConfig, TriggerEvent, TriggerStore, Wake, WakeError, Waker,
};
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::Crew;
use super::poll::Poller;
use crate::{DayPlan, Trading};

fn millis(at: DateTime<Utc>) -> UnixMillis {
    UnixMillis(at.timestamp_millis())
}

fn time(at: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(at).single().unwrap_or_default()
}

fn store_error(error: pi_store::StoreError) -> StoreError {
    StoreError(error.to_string())
}

/// 저장소에 둔 작업 상태 · 영수증.
struct StoreTriggers(Store);

fn job_value(job: &JobState) -> Value {
    let disabled = job.disabled.map(|reason| match reason {
        DisableReason::ConsecutiveFailures => "consecutive_failures",
        _ => "manual",
    });
    json!({
        "last_slot": job.last_slot.map(|t| t.timestamp_millis()),
        "consecutive_errors": job.consecutive_errors,
        "retry_at": job.retry_at.map(|t| t.timestamp_millis()),
        "disabled": disabled,
        "pending_catch_up": job.pending_catch_up.iter().map(DateTime::timestamp_millis).collect::<Vec<_>>(),
        "pending_slot": job.pending_slot.map(|t| t.timestamp_millis()),
    })
}

fn job_of(id: &str, value: &Value) -> JobState {
    let at = |name: &str| value[name].as_i64().map(time);
    let mut job = JobState::new(JobId::new(id), at("last_slot"));
    job.consecutive_errors =
        u32::try_from(value["consecutive_errors"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
    job.retry_at = at("retry_at");
    job.disabled = match value["disabled"].as_str() {
        Some("consecutive_failures") => Some(DisableReason::ConsecutiveFailures),
        Some(_) => Some(DisableReason::Manual),
        None => None,
    };
    job.pending_catch_up = value["pending_catch_up"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_i64).map(time).collect())
        .unwrap_or_default();
    job.pending_slot = at("pending_slot");
    job
}

fn key_of(key: &ReceiptKey) -> TriggerReceiptKey {
    TriggerReceiptKey {
        job: key.job.as_str().to_string(),
        slot: millis(key.slot),
    }
}

fn open_of(outcome: OpenTriggerReceipt) -> OpenReceipt {
    match outcome {
        OpenTriggerReceipt::Opened => OpenReceipt::Opened,
        OpenTriggerReceipt::Duplicate => OpenReceipt::Duplicate,
        OpenTriggerReceipt::AlreadyRunning(key) => OpenReceipt::AlreadyRunning(ReceiptKey {
            job: JobId::new(key.job),
            slot: time(key.slot.0),
        }),
    }
}

#[async_trait]
impl TriggerStore for StoreTriggers {
    async fn jobs(&self) -> Result<Vec<JobState>, StoreError> {
        let jobs = self.0.trigger_jobs().map_err(store_error)?;
        Ok(jobs
            .iter()
            .map(|job| job_of(&job.job, &job.state))
            .collect())
    }

    async fn save_job(&self, job: &JobState) -> Result<(), StoreError> {
        self.0
            .save_trigger_job(job.id.as_str(), &job_value(job))
            .map_err(store_error)
    }

    async fn open_receipt(
        &self,
        key: &ReceiptKey,
        started_at: DateTime<Utc>,
    ) -> Result<OpenReceipt, StoreError> {
        let opened = self
            .0
            .open_trigger_receipt(&key_of(key), millis(started_at));
        opened.map(open_of).map_err(store_error)
    }

    async fn save_and_open(
        &self,
        job: &JobState,
        key: &ReceiptKey,
        started_at: DateTime<Utc>,
    ) -> Result<OpenReceipt, StoreError> {
        let value = job_value(job);
        let opened = self.0.save_job_and_open_receipt(
            job.id.as_str(),
            &value,
            &key_of(key),
            millis(started_at),
        );
        opened.map(open_of).map_err(store_error)
    }

    async fn mark_delivery_started(&self, key: &ReceiptKey) -> Result<(), StoreError> {
        self.0
            .mark_trigger_delivery_started(&key_of(key))
            .map_err(store_error)
    }

    async fn close_receipt(
        &self,
        key: &ReceiptKey,
        status: ReceiptStatus,
        finished_at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let status = match status {
            ReceiptStatus::Running => TriggerReceiptStatus::Running,
            ReceiptStatus::Ok => TriggerReceiptStatus::Ok,
            ReceiptStatus::Error => TriggerReceiptStatus::Error,
            ReceiptStatus::Interrupted => TriggerReceiptStatus::Interrupted,
        };
        self.0
            .close_trigger_receipt(&key_of(key), status, millis(finished_at))
            .map_err(store_error)
    }

    async fn receipts(&self) -> Result<Vec<Receipt>, StoreError> {
        let stored = self.0.trigger_receipts().map_err(store_error)?;
        Ok(stored
            .into_iter()
            .map(|r| Receipt {
                key: ReceiptKey {
                    job: JobId::new(r.key.job),
                    slot: time(r.key.slot.0),
                },
                status: match r.status {
                    TriggerReceiptStatus::Running => ReceiptStatus::Running,
                    TriggerReceiptStatus::Ok => ReceiptStatus::Ok,
                    TriggerReceiptStatus::Error => ReceiptStatus::Error,
                    TriggerReceiptStatus::Interrupted => ReceiptStatus::Interrupted,
                },
                delivery: match r.delivery {
                    TriggerDelivery::NotStarted => DeliveryState::NotStarted,
                    TriggerDelivery::Started => DeliveryState::Started,
                },
                started_at: time(r.started_at.0),
                finished_at: r.finished_at.map(|at| time(at.0)),
            })
            .collect())
    }
}

/// 주문 길의 달력과 마지막 시세를 내주는 시장 어댑터(바깥에 묻지 않는다).
struct CalendarMarket(Arc<Trading>);

#[async_trait]
impl MarketAdapter for CalendarMarket {
    fn market(&self) -> Market {
        self.0.market()
    }

    async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
        Ok(symbols
            .iter()
            .filter_map(|symbol| self.0.last_quote(symbol.as_str()))
            .collect())
    }

    async fn calendar(&self, date: NaiveDate) -> Result<MarketCalendar, MarketError> {
        self.0.calendar_on(date).ok_or(MarketError::Unavailable {
            status: 503,
            code: None,
        })
    }
}

/// 실제 단조 시계.
struct Steady(Instant);

impl Monotonic for Steady {
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
}

/// 전체 정지 표시: 저장소의 팀 상태.
struct FullStop(Store);

impl StopSignal for FullStop {
    fn is_stopped(&self) -> bool {
        matches!(self.0.team_state(), Ok(TeamState::FullStop))
    }
}

/// 팀을 깨운다.
struct CrewWaker(Arc<Crew>);

impl CrewWaker {
    async fn open(&self) -> Result<(), WakeError> {
        let fail = |error: crate::TeamError| WakeError(error.to_string());
        // (10b-2) 장 시작의 공시 보기는 쓰는 템플릿이 공시를 구독할 때만 깨우기를 만든다.
        if self.0.plan_day().await.map_err(fail)? == DayPlan::Trade
            && super::poll::disclosure_subscribed(&self.0)
        {
            self.0.scan().await.map_err(fail)?;
        }
        Ok(())
    }
}

#[async_trait]
impl Waker for CrewWaker {
    async fn wake(&self, wake: Wake) -> Result<(), WakeError> {
        if let Target::Member(member) = &wake.target
            && member == super::poll::SCOUT
        {
            return self
                .0
                .on_scout(&wake.events)
                .await
                .map_err(|error| WakeError(error.to_string()));
        }
        for event in &wake.events {
            let kind = match event {
                TriggerEvent::Schedule { kind, .. } | TriggerEvent::CatchUp { kind, .. } => *kind,
                _ => continue,
            };
            match kind {
                ScheduleKind::MarketOpen => self.open().await?,
                ScheduleKind::AfterClose => {
                    self.0
                        .review_day()
                        .await
                        .map_err(|error| WakeError(error.to_string()))?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// 장 시작 작업 id.
const OPEN_JOB: &str = "team-open";

/// (24차 3번, 시연 조립에서만) 장 시작 작업을 처음 보고 지금 그날 정규장 안이면, 그 작업의 기준 시각을 그날 장 시작
/// 바로 앞으로 적어 둔다. 그래서 첫 tick이 이미 지난 그날 장 시작 회차를 기한으로 보고(장이 열려 있으니 바로)
/// 하루 판단을 연다. 처음 보는 작업은 기준 시각을 띄운 시각으로 잡아 지난 회차를 돌리지 않는 것이 엔진의 동작이라,
/// 장 중간에 띄운 시연은 다음 날까지 하루를 시작하지 않기 때문이다. 이미 상태가 있으면(다시 켬) 건드리지 않는다.
fn catch_up_today(crew: &Crew, now: DateTime<Utc>) {
    let store = &crew.store;
    match store.trigger_jobs() {
        Ok(jobs) if jobs.iter().all(|job| job.job != OPEN_JOB) => {}
        Ok(_) => return,
        Err(error) => {
            tracing::warn!(%error, "깨우기 작업 상태를 읽지 못했다");
            return;
        }
    }
    let trading = &crew.trading;
    let Some(calendar) = trading.calendar_on(trading.date_of(now)) else {
        return;
    };
    let Some(open) = calendar
        .session_at(now)
        .filter(|session| session.kind == pi_market::SessionKind::Regular)
        .map(|session| session.start)
    else {
        return;
    };
    let before = open - chrono::Duration::milliseconds(1);
    let job = JobState::new(JobId::new(OPEN_JOB), Some(before));
    if let Err(error) = store.save_trigger_job(OPEN_JOB, &job_value(&job)) {
        tracing::warn!(%error, "장 시작 따라잡기를 적지 못했다");
    }
}

/// 깨우기 엔진을 띄운다. `stop`이 참이 되면 끝난다.
///
/// 띄우기 전에 여기서 한 번 돈다: 처음 보는 작업의 기준 시각을 **띄운 시각**으로 적어 두어, 뒤에서 도는
/// 고리가 늦게 시작해도 띄운 뒤에 온 회차(예: 장 시작 전에 띄우고 맞은 장 시작)를 놓치지 않는다.
///
/// (24차) `demo`가 시연 달력을 켰으면, 장 중간에 처음 띄운 날에도 그날 장 시작 회차를 따라잡는다
/// ([`catch_up_today`]). 가짜 공시 간격이 있으면 그 간격마다 시연용 가짜 공시를 넣는다.
pub(crate) async fn spawn_trigger(
    crew: &Arc<Crew>,
    clock: Arc<dyn Clock>,
    stop: watch::Receiver<bool>,
    demo: &crate::DemoWiring,
) -> Result<JoinHandle<()>, crate::TeamError> {
    if demo.calendar {
        catch_up_today(crew, clock.now());
    }
    let market = crew.trading.market();
    let job = |id: &str, kind| ScheduleJob {
        id: JobId::new(id),
        market,
        kind,
        target: Target::Team,
    };
    let mut config = TriggerConfig::new(
        vec![
            job(OPEN_JOB, ScheduleKind::MarketOpen),
            job("team-after-close", ScheduleKind::AfterClose),
        ],
        Vec::new(),
    );
    config.times = crew.wiring.schedule;
    let ports = Ports {
        clock,
        monotonic: Arc::new(Steady(Instant::now())),
        markets: vec![Arc::new(CalendarMarket(Arc::clone(&crew.trading)))],
        store: Arc::new(StoreTriggers(crew.store.clone())),
        waker: Arc::new(CrewWaker(Arc::clone(crew))),
        stop: Arc::new(FullStop(crew.store.clone())),
    };
    let failed = |e: pi_trigger::Error| crate::TeamError::Trigger(e.to_string());
    let trigger = Arc::new(Trigger::new(config, ports).map_err(failed)?);
    trigger.start().await.map_err(failed)?;
    if let Err(error) = trigger.tick().await {
        tracing::warn!(%error, "깨우기 엔진의 첫 tick이 실패했다");
    }
    // (10b 8번) 시세 조건 · 자료 변화를 엔진에 넣는 고리. 엔진과 함께 멈춘다.
    let poller =
        Poller::new(Arc::clone(crew), Arc::clone(&trigger)).with_demo(demo.disclosure_every);
    let polling = stop.clone();
    Ok(tokio::spawn(async move {
        let engine = async {
            if let Err(error) = trigger.run(stop).await {
                tracing::warn!(%error, "깨우기 엔진이 멈췄다");
            }
        };
        tokio::join!(engine, poller.run(polling));
    }))
}
