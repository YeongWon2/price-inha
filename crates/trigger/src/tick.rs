//! tick 하나의 몸통.
//!
//! 순서(원본: Hermes 1.1): 잠금 → 전체 정지 확인 → 저장소에서 작업 상태 다시 읽기 → 기한 판정 →
//! 작업 상태를 먼저 옮겨 저장 → 영수증 열기 · 넘기기 시작 적기 → 대상마다 묶어 깨우기 → 영수증 닫기.

use std::collections::HashMap;
use std::mem;
use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::{Market, MarketCalendar, Quote, Symbol};

use crate::engine::lock;
use crate::inflight::{Firing, launch};
use crate::plan::{market_date, plan_schedule, schedule_wait};
use crate::{
    ConditionMemory, Error, JobId, JobState, MAX_TIMER, MIN_REFIRE_GAP, Observation, OpenReceipt,
    ReceiptKey, ReceiptStatus, ScheduleJob, StoreError, Target, TickOutcome, TickReport, Trigger,
    TriggerEvent, Wake, WakeReason, observe,
};

type States = HashMap<JobId, JobState>;
type Calendars = HashMap<Market, MarketCalendar>;

pub(crate) async fn tick(trigger: &Trigger) -> Result<TickReport, Error> {
    let Ok(_running) = trigger.running.try_lock() else {
        return Ok(report(TickOutcome::Busy, false, MIN_REFIRE_GAP));
    };
    let now = trigger.now();
    let clock_jumped = trigger.clock_jumped(now, trigger.ports.monotonic.elapsed());
    if trigger.ports.stop.is_stopped() {
        // 전체 정지는 새 매매를 모두 멈춘다(F4 7b2): 모인 자료 사건은 깨우지 않고 버린다.
        lock(&trigger.data).drop_all();
        let delay = stopped_delay(trigger, now).await;
        let mut stopped = report(TickOutcome::Stopped, clock_jumped, delay);
        stopped.data_dropped = lock(&trigger.data).take_dropped();
        return Ok(stopped);
    }
    let calendars = load_calendars(trigger, now).await?;
    // 시세는 쓰기 잠금 밖에서 받는다(그동안 끝난 깨우기의 반영이 막히지 않게, 3차 6번).
    let (quotes, anomalies) = fetch_quotes(trigger, &calendars, now).await;
    // 끝난 깨우기의 반영과 이번 tick의 쓰기가 서로 덮어쓰지 않게 한다(깨우기 자체는 기다리지 않는다).
    let _writes = trigger.inflight.writes.lock().await;

    let mut states = load_states(trigger).await?;
    for id in &job_ids(trigger) {
        if !states.contains_key(id) {
            let state = JobState::new(id.clone(), Some(now));
            save(trigger, &mut states, state).await?;
        }
    }

    let mut firings = Vec::new();
    // 되살릴 회차는 영수증을 열 때까지 남겨 둔다(tick이 중간에 빠져도 잃지 않게, 3차 3번).
    let revived = lock(&trigger.revived).clone();
    plan_schedules(trigger, &calendars, &mut states, now, revived, &mut firings).await?;
    plan_conditions(trigger, &quotes, &mut states, now, &mut firings).await?;

    let mut errors = Vec::new();
    let opened = open_all(trigger, &mut states, firings, now, &mut errors).await;
    // 자료 깨우기는 영수증 없이 띄운다. 같은 대상이 실행 중이면 묶음에 남겨 다음 tick에 다시 본다.
    let data_config = &trigger.config.data;
    let busy = trigger.inflight.target_running(&data_config.target);
    let data = lock(&trigger.data).collect(now, &calendars, data_config, busy);
    let mut wakes = Vec::new();
    for (wake, members) in group(trigger, opened, anomalies, data) {
        wakes.push(wake.clone());
        launch(trigger, wake, members);
    }
    if let Some(error) = errors.into_iter().next() {
        return Err(Error::Store(error));
    }

    let next_delay = next_delay(trigger, &states, &calendars, now);
    Ok(TickReport {
        outcome: TickOutcome::Ran,
        wakes,
        alerts: trigger.inflight.take_alerts(),
        clock_jumped,
        data_dropped: lock(&trigger.data).take_dropped(),
        next_delay,
    })
}

/// 작업마다 "상태 옮기기 + 영수증 열기"를 한 단위로 쓰고 넘기기 시작을 적는다(2차 1번).
/// 쓰기가 실패한 작업은 이번 발사에서 빠진다(상태 그대로, 회차는 기한으로 남는다). 연 영수증에 넘기기
/// 시작을 적지 못하면 깨우지 않고 실패로 닫는다(실행 중으로 남지 않게).
async fn open_all(
    trigger: &Trigger,
    states: &mut States,
    firings: Vec<Firing>,
    now: DateTime<Utc>,
    errors: &mut Vec<StoreError>,
) -> Vec<Firing> {
    let store = &trigger.ports.store;
    let mut opened = Vec::new();
    for mut firing in firings {
        // 되살린 회차는 영수증과 함께 적으므로 더 기다리는 회차가 아니다(3차 5번).
        firing.state.pending_slot = (!firing.revived).then_some(firing.key.slot);
        let (job, slot) = (&firing.key.job, firing.key.slot);
        let result = store.save_and_open(&firing.state, &firing.key, now).await;
        if firing.revived && matches!(result, Ok(OpenReceipt::Opened | OpenReceipt::Duplicate)) {
            forget_revived(trigger, &firing.key);
        }
        match result {
            Ok(OpenReceipt::Opened) => {
                states.insert(job.clone(), firing.state.clone());
                match store.mark_delivery_started(&firing.key).await {
                    Ok(()) => opened.push(firing),
                    Err(error) => {
                        tracing::warn!(%job, %slot, %error, "넘기기 시작을 적지 못해 깨우지 않고 실패로 닫는다");
                        let closed = store.close_receipt(&firing.key, ReceiptStatus::Error, now);
                        if let Err(close) = closed.await {
                            errors.push(close);
                        }
                        errors.push(error);
                    }
                }
            }
            Ok(OpenReceipt::Duplicate) => {
                states.insert(job.clone(), firing.state.clone());
                tracing::info!(%job, %slot, "같은 회차의 영수증이 있어 깨우지 않는다");
            }
            Ok(OpenReceipt::AlreadyRunning(running)) => {
                tracing::info!(%job, running = %running.slot, "같은 작업이 실행 중이라 깨우지 않는다");
            }
            Err(error) => {
                tracing::warn!(%job, %slot, %error, "상태와 영수증을 적지 못해 이번 발사에서 뺀다");
                errors.push(error);
            }
        }
    }
    opened
}

/// 영수증을 연(또는 이미 있던) 되살린 회차를 목록에서 지운다.
fn forget_revived(trigger: &Trigger, key: &ReceiptKey) {
    lock(&trigger.revived).retain(|revived| revived != key);
}

fn report(outcome: TickOutcome, clock_jumped: bool, next_delay: Duration) -> TickReport {
    TickReport {
        outcome,
        wakes: Vec::new(),
        alerts: Vec::new(),
        clock_jumped,
        data_dropped: 0,
        next_delay,
    }
}

/// 전체 정지 중에도 다음에 볼 시각은 낸다(읽기만 한다). 읽지 못하면 가장 짧게.
async fn stopped_delay(trigger: &Trigger, now: DateTime<Utc>) -> Duration {
    let calendars = load_calendars(trigger, now).await;
    let states = load_states(trigger).await;
    match (calendars, states) {
        (Ok(calendars), Ok(states)) => next_delay(trigger, &states, &calendars, now),
        _ => MIN_REFIRE_GAP,
    }
}

fn job_ids(trigger: &Trigger) -> Vec<JobId> {
    let schedules = trigger.config.schedules.iter().map(|job| job.id.clone());
    schedules
        .chain(trigger.conditions().into_iter().map(|job| job.id))
        .collect()
}

async fn load_calendars(trigger: &Trigger, now: DateTime<Utc>) -> Result<Calendars, Error> {
    let mut markets: Vec<Market> = trigger
        .config
        .schedules
        .iter()
        .map(|job| job.market)
        .collect();
    markets.extend(trigger.conditions().iter().map(|job| job.market));
    markets.extend(lock(&trigger.data).markets());
    let mut calendars = HashMap::new();
    for market in markets {
        if calendars.contains_key(&market) {
            continue;
        }
        let Some(adapter) = trigger.adapter(market) else {
            continue;
        };
        let calendar = adapter.calendar(market_date(market, now)).await?;
        calendars.insert(market, calendar);
    }
    Ok(calendars)
}

async fn load_states(trigger: &Trigger) -> Result<States, Error> {
    let jobs = trigger.ports.store.jobs().await?;
    Ok(jobs.into_iter().map(|job| (job.id.clone(), job)).collect())
}

async fn save(trigger: &Trigger, states: &mut States, state: JobState) -> Result<(), Error> {
    trigger.ports.store.save_job(&state).await?;
    states.insert(state.id.clone(), state);
    Ok(())
}

/// 일정: 기한을 판정한다. 발사하지 않고 상태만 바뀐 것은 바로 저장하고, 발사할 것은 영수증과 한 단위로
/// 쓰도록 넘긴다. 되살린 회차는 먼저 넘긴다(같은 작업의 새 회차는 그것이 끝난 뒤로 남는다).
async fn plan_schedules(
    trigger: &Trigger,
    calendars: &Calendars,
    states: &mut States,
    now: DateTime<Utc>,
    mut revived: Vec<ReceiptKey>,
    firings: &mut Vec<Firing>,
) -> Result<(), Error> {
    for job in &trigger.config.schedules {
        let (Some(calendar), Some(state)) = (calendars.get(&job.market), states.get(&job.id))
        else {
            continue;
        };
        for key in take_revived(&mut revived, &job.id) {
            if state.disabled.is_none() {
                firings.push(revived_firing(job, state, key));
            } else {
                forget_revived(trigger, &key);
            }
        }
        let plan = plan_schedule(job, state, calendar, &trigger.config.times, now);
        match plan.receipt_slot {
            Some(slot) if !plan.events.is_empty() => firings.push(Firing {
                key: ReceiptKey {
                    job: job.id.clone(),
                    slot,
                },
                target: job.target.clone(),
                events: plan.events,
                on_success: None,
                state: plan.state,
                revived: false,
            }),
            _ if plan.changed => save(trigger, states, plan.state).await?,
            _ => {}
        }
    }
    Ok(())
}

fn take_revived(revived: &mut Vec<ReceiptKey>, job: &JobId) -> Vec<ReceiptKey> {
    let (mine, rest) = mem::take(revived)
        .into_iter()
        .partition(|key| &key.job == job);
    *revived = rest;
    mine
}

/// 되살린 회차의 발사. 상태는 이미 옮겨져 있으므로 그대로 적는다.
fn revived_firing(job: &ScheduleJob, state: &JobState, key: ReceiptKey) -> Firing {
    Firing {
        target: job.target.clone(),
        events: vec![TriggerEvent::Schedule {
            job: job.id.clone(),
            kind: job.kind,
            market: job.market,
            slot: key.slot,
            missed: 0,
        }],
        on_success: None,
        state: state.clone(),
        revived: true,
        key,
    }
}

/// 세션이 열린 시장만 시세를 받는다. 받은(늦지 않은) 시세와 늦은 시세 사건들을 돌려준다.
async fn fetch_quotes(
    trigger: &Trigger,
    calendars: &Calendars,
    now: DateTime<Utc>,
) -> (HashMap<(Market, Symbol), Quote>, Vec<TriggerEvent>) {
    let mut by_market: Vec<(Market, Vec<Symbol>)> = Vec::new();
    for job in &trigger.conditions() {
        match by_market
            .iter_mut()
            .find(|(market, _)| *market == job.market)
        {
            Some((_, symbols)) if symbols.contains(&job.symbol) => {}
            Some((_, symbols)) => symbols.push(job.symbol.clone()),
            None => by_market.push((job.market, vec![job.symbol.clone()])),
        }
    }
    let mut fresh = HashMap::new();
    let mut anomalies = Vec::new();
    for (market, symbols) in by_market {
        let (Some(calendar), Some(adapter)) = (calendars.get(&market), trigger.adapter(market))
        else {
            continue;
        };
        if calendar.session_at(now).is_none() {
            let mut watches = lock(&trigger.watches);
            for symbol in &symbols {
                if let Some(watch) = watches.get_mut(&(market, symbol.clone())) {
                    watch.closed();
                }
            }
            continue;
        }
        let quotes = adapter.quotes(&symbols).await.unwrap_or_else(|error| {
            tracing::warn!(?market, %error, "시세를 받지 못했다");
            Vec::new()
        });
        let mut watches = lock(&trigger.watches);
        for symbol in symbols {
            let received = quotes
                .iter()
                .filter(|quote| quote.symbol == symbol)
                .max_by_key(|quote| quote.received_at)
                .cloned();
            let watch = watches.entry((market, symbol.clone())).or_default();
            let policy = &trigger.config.stale;
            let (quote, anomaly) = watch.observe(market, &symbol, received, calendar, now, policy);
            if let Some(quote) = quote {
                fresh.insert((market, symbol), quote);
            }
            anomalies.extend(anomaly);
        }
    }
    (fresh, anomalies)
}

/// 시세 조건: 맞으면 발사하고, 다시 걸리면 상태만 저장한다.
async fn plan_conditions(
    trigger: &Trigger,
    quotes: &HashMap<(Market, Symbol), Quote>,
    states: &mut States,
    now: DateTime<Utc>,
    firings: &mut Vec<Firing>,
) -> Result<(), Error> {
    for job in &trigger.conditions() {
        let Some(state) = states.get(&job.id) else {
            continue;
        };
        if state.disabled.is_some() || state.retry_at.is_some_and(|retry| now < retry) {
            continue;
        }
        let Some(quote) = quotes.get(&(job.market, job.symbol.clone())) else {
            continue;
        };
        let memory = state.condition.clone().unwrap_or(ConditionMemory::Armed);
        match observe(
            &job.kind,
            &memory,
            quote.price,
            trigger.config.rearm_percent,
        ) {
            Observation::Fire(direction) => firings.push(Firing {
                key: ReceiptKey {
                    job: job.id.clone(),
                    slot: now,
                },
                target: job.target.clone(),
                events: vec![TriggerEvent::Price {
                    job: job.id.clone(),
                    symbol: job.symbol.clone(),
                    condition: job.kind.clone(),
                    direction,
                    price: quote.price,
                    at: quote.received_at,
                }],
                on_success: Some(ConditionMemory::Fired {
                    notified: quote.price,
                    direction,
                }),
                state: state.clone(),
                revived: false,
            }),
            Observation::Rearm => {
                let mut state = state.clone();
                state.condition = Some(ConditionMemory::Armed);
                save(trigger, states, state).await?;
            }
            Observation::Nothing => {}
        }
    }
    Ok(())
}

/// 대상마다 하나의 깨우기로 묶는다(대상이 처음 나온 순, 사건은 생긴 순).
fn group(
    trigger: &Trigger,
    firings: Vec<Firing>,
    anomalies: Vec<TriggerEvent>,
    data: Vec<TriggerEvent>,
) -> Vec<(Wake, Vec<Firing>)> {
    let mut groups: Vec<(Wake, Vec<Firing>)> = Vec::new();
    let slot_for = |groups: &mut Vec<(Wake, Vec<Firing>)>, target: &Target| -> usize {
        if let Some(index) = groups.iter().position(|(wake, _)| &wake.target == target) {
            return index;
        }
        groups.push((
            Wake {
                target: target.clone(),
                reason: WakeReason::Price,
                events: Vec::new(),
            },
            Vec::new(),
        ));
        groups.len() - 1
    };
    for mut firing in firings {
        let index = slot_for(&mut groups, &firing.target);
        let (wake, members) = &mut groups[index];
        wake.events.append(&mut firing.events);
        members.push(firing);
    }
    if !anomalies.is_empty() {
        let index = slot_for(&mut groups, &trigger.config.stale_target);
        groups[index].0.events.extend(anomalies);
    }
    if !data.is_empty() {
        let index = slot_for(&mut groups, &trigger.config.data.target);
        groups[index].0.events.extend(data);
    }
    for (wake, _) in &mut groups {
        wake.reason = reason_of(&wake.events);
    }
    groups
}

/// 묶인 사건들의 이유: `Anomaly` > `CatchUp` > `Schedule` > `Price` > `Data`.
fn reason_of(events: &[TriggerEvent]) -> WakeReason {
    let rank = |event: &TriggerEvent| match event {
        TriggerEvent::StaleQuote { .. } => (0, WakeReason::Anomaly),
        TriggerEvent::CatchUp { .. } => (1, WakeReason::CatchUp),
        TriggerEvent::Schedule { .. } => (2, WakeReason::Schedule),
        TriggerEvent::Price { .. } => (3, WakeReason::Price),
        TriggerEvent::Data { .. } => (4, WakeReason::Data),
    };
    events
        .iter()
        .map(rank)
        .min_by_key(|(order, _)| *order)
        .map_or(WakeReason::Price, |(_, reason)| reason)
}

/// 다음 tick까지: 다음 회차 · 시세 간격(시세 조건이 있고 장이 열렸을 때) · 자료 묶음을 보낼 때 ·
/// 60초 가운데 가장 짧은 것.
fn next_delay(
    trigger: &Trigger,
    states: &States,
    calendars: &Calendars,
    now: DateTime<Utc>,
) -> Duration {
    let mut delay = MAX_TIMER;
    for job in &trigger.config.schedules {
        let Some(calendar) = calendars.get(&job.market) else {
            continue;
        };
        let fresh;
        let state = match states.get(&job.id) {
            Some(state) => state,
            None => {
                fresh = JobState::new(job.id.clone(), Some(now));
                &fresh
            }
        };
        if let Some(at) = schedule_wait(job, state, calendar, &trigger.config.times, now) {
            // 같은 작업이 실행 중이라 기한으로 남은 회차는 끝난 뒤에 나가므로 대기 계산에서 뺀다(3차 4번).
            if at <= now && trigger.inflight.is_running(&job.id) {
                continue;
            }
            let until = at.signed_duration_since(now).to_std().unwrap_or_default();
            delay = delay.min(until);
        }
    }
    let quoting = trigger.conditions().iter().any(|job| {
        calendars
            .get(&job.market)
            .is_some_and(|calendar| calendar.session_at(now).is_some())
    });
    if quoting {
        delay = delay.min(trigger.config.quote_interval);
    }
    let data_config = &trigger.config.data;
    if let Some(due) = lock(&trigger.data).next_due(now, calendars, data_config) {
        // 대상이 실행 중이면 끝나기를 시세 간격마다 본다(짧게 돌지 않게).
        let due = if trigger.inflight.target_running(&data_config.target) {
            due.max(trigger.config.quote_interval)
        } else {
            due
        };
        delay = delay.min(due);
    }
    if delay.is_zero() {
        MIN_REFIRE_GAP
    } else {
        delay
    }
}
