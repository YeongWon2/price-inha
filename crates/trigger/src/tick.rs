//! tick 하나의 몸통.
//!
//! 순서(원본: Hermes 1.1): 잠금 → 전체 정지 확인 → 저장소에서 작업 상태 다시 읽기 → 기한 판정 →
//! 작업 상태를 먼저 옮겨 저장 → 영수증 열기 · 넘기기 시작 적기 → 대상마다 묶어 깨우기 → 영수증 닫기.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::{Market, MarketCalendar, Quote, Symbol};

use crate::engine::lock;
use crate::plan::{market_date, plan_schedule, schedule_wait};
use crate::{
    AUTO_DISABLE_AFTER, Alert, AlertReason, ConditionMemory, DisableReason, Error, JobId, JobState,
    MAX_TIMER, MIN_REFIRE_GAP, Observation, OpenReceipt, ReceiptKey, ReceiptStatus, Target,
    TickOutcome, TickReport, Trigger, TriggerEvent, Wake, WakeReason, error_backoff, observe,
};

/// 발사하기로 한 작업 하나.
struct Firing {
    key: ReceiptKey,
    target: Target,
    events: Vec<TriggerEvent>,
    /// 깨우기가 성공하면 저장할 다시 걸기 상태(시세 조건).
    on_success: Option<ConditionMemory>,
}

type States = HashMap<JobId, JobState>;
type Calendars = HashMap<Market, MarketCalendar>;

pub(crate) async fn tick(trigger: &Trigger) -> Result<TickReport, Error> {
    let Ok(_running) = trigger.running.try_lock() else {
        return Ok(report(TickOutcome::Busy, false, MIN_REFIRE_GAP));
    };
    let now = trigger.now();
    let clock_jumped = trigger.clock_jumped(now, trigger.ports.monotonic.elapsed());
    if trigger.ports.stop.is_stopped() {
        let delay = stopped_delay(trigger, now).await;
        return Ok(report(TickOutcome::Stopped, clock_jumped, delay));
    }

    let calendars = load_calendars(trigger, now).await?;
    let mut states = load_states(trigger).await?;
    for id in job_ids(trigger) {
        if !states.contains_key(id) {
            let state = JobState::new(id.clone(), Some(now));
            save(trigger, &mut states, state).await?;
        }
    }

    let mut firings = Vec::new();
    plan_schedules(trigger, &calendars, &mut states, now, &mut firings).await?;
    let (quotes, anomalies) = fetch_quotes(trigger, &calendars, now).await;
    plan_conditions(trigger, &quotes, &mut states, now, &mut firings).await?;

    let mut opened = Vec::new();
    for firing in firings {
        match trigger.ports.store.open_receipt(&firing.key, now).await? {
            OpenReceipt::Opened => opened.push(firing),
            OpenReceipt::Duplicate => {
                tracing::info!(job = %firing.key.job, slot = %firing.key.slot, "같은 회차의 영수증이 있어 깨우지 않는다");
            }
            OpenReceipt::AlreadyRunning(running) => {
                tracing::info!(job = %firing.key.job, running = %running.slot, "같은 작업이 실행 중이라 깨우지 않는다");
            }
        }
    }
    for firing in &opened {
        trigger
            .ports
            .store
            .mark_delivery_started(&firing.key)
            .await?;
    }

    let groups = group(trigger, opened, anomalies);
    let mut wakes = Vec::new();
    let mut alerts = Vec::new();
    for (wake, members) in groups {
        let result = trigger.ports.waker.wake(wake.clone()).await;
        if let Err(error) = &result {
            tracing::warn!(target = ?wake.target, %error, "깨운 실행이 실패했다");
        }
        wakes.push(wake);
        let finished = trigger.now();
        for firing in members {
            if let Some(alert) =
                finish(trigger, &mut states, firing, result.is_ok(), finished).await?
            {
                alerts.push(alert);
            }
        }
    }

    let next_delay = next_delay(trigger, &states, &calendars, now);
    Ok(TickReport {
        outcome: TickOutcome::Ran,
        wakes,
        alerts,
        clock_jumped,
        next_delay,
    })
}

fn report(outcome: TickOutcome, clock_jumped: bool, next_delay: Duration) -> TickReport {
    TickReport {
        outcome,
        wakes: Vec::new(),
        alerts: Vec::new(),
        clock_jumped,
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

fn job_ids(trigger: &Trigger) -> impl Iterator<Item = &JobId> {
    let config = &trigger.config;
    let schedules = config.schedules.iter().map(|job| &job.id);
    schedules.chain(config.conditions.iter().map(|job| &job.id))
}

async fn load_calendars(trigger: &Trigger, now: DateTime<Utc>) -> Result<Calendars, Error> {
    let config = &trigger.config;
    let mut markets: Vec<Market> = config.schedules.iter().map(|job| job.market).collect();
    markets.extend(config.conditions.iter().map(|job| job.market));
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

/// 일정: 기한을 판정하고, 옮긴 상태를 깨우기 전에 먼저 저장한다.
async fn plan_schedules(
    trigger: &Trigger,
    calendars: &Calendars,
    states: &mut States,
    now: DateTime<Utc>,
    firings: &mut Vec<Firing>,
) -> Result<(), Error> {
    for job in &trigger.config.schedules {
        let (Some(calendar), Some(state)) = (calendars.get(&job.market), states.get(&job.id))
        else {
            continue;
        };
        let plan = plan_schedule(job, state, calendar, &trigger.config.times, now);
        if plan.changed {
            save(trigger, states, plan.state).await?;
        }
        if let Some(slot) = plan.receipt_slot
            && !plan.events.is_empty()
        {
            firings.push(Firing {
                key: ReceiptKey {
                    job: job.id.clone(),
                    slot,
                },
                target: job.target.clone(),
                events: plan.events,
                on_success: None,
            });
        }
    }
    Ok(())
}

/// 세션이 열린 시장만 시세를 받는다. 받은(늦지 않은) 시세와 늦은 시세 사건들을 돌려준다.
async fn fetch_quotes(
    trigger: &Trigger,
    calendars: &Calendars,
    now: DateTime<Utc>,
) -> (HashMap<(Market, Symbol), Quote>, Vec<TriggerEvent>) {
    let mut by_market: Vec<(Market, Vec<Symbol>)> = Vec::new();
    for job in &trigger.config.conditions {
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
    for job in &trigger.config.conditions {
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
    for (wake, _) in &mut groups {
        wake.reason = reason_of(&wake.events);
    }
    groups
}

/// 묶인 사건들의 이유: `Anomaly` > `CatchUp` > `Schedule` > `Price`.
fn reason_of(events: &[TriggerEvent]) -> WakeReason {
    let rank = |event: &TriggerEvent| match event {
        TriggerEvent::StaleQuote { .. } => (0, WakeReason::Anomaly),
        TriggerEvent::CatchUp { .. } => (1, WakeReason::CatchUp),
        TriggerEvent::Schedule { .. } => (2, WakeReason::Schedule),
        TriggerEvent::Price { .. } => (3, WakeReason::Price),
    };
    events
        .iter()
        .map(rank)
        .min_by_key(|(order, _)| *order)
        .map_or(WakeReason::Price, |(_, reason)| reason)
}

/// 깨운 결과로 영수증을 닫고 작업 상태를 고친다. 끈 순간에만 알림 요구를 낸다.
async fn finish(
    trigger: &Trigger,
    states: &mut States,
    firing: Firing,
    ok: bool,
    finished: DateTime<Utc>,
) -> Result<Option<Alert>, Error> {
    let status = if ok {
        ReceiptStatus::Ok
    } else {
        ReceiptStatus::Error
    };
    trigger
        .ports
        .store
        .close_receipt(&firing.key, status, finished)
        .await?;
    let Some(mut state) = states.get(&firing.key.job).cloned() else {
        return Ok(None);
    };
    let mut alert = None;
    if ok {
        state.consecutive_errors = 0;
        state.retry_at = None;
        if let Some(memory) = firing.on_success {
            state.condition = Some(memory);
        }
    } else {
        state.consecutive_errors = state.consecutive_errors.saturating_add(1);
        let backoff = chrono::Duration::from_std(error_backoff(state.consecutive_errors))
            .unwrap_or(chrono::TimeDelta::MAX);
        state.retry_at = finished.checked_add_signed(backoff);
        if state.consecutive_errors >= AUTO_DISABLE_AFTER && state.disabled.is_none() {
            state.disabled = Some(DisableReason::ConsecutiveFailures);
            tracing::warn!(job = %state.id, errors = state.consecutive_errors, "연달아 실패해 작업을 껐다");
            alert = Some(Alert {
                job: state.id.clone(),
                reason: AlertReason::AutoDisabled {
                    consecutive_errors: state.consecutive_errors,
                },
            });
        }
    }
    save(trigger, states, state).await?;
    Ok(alert)
}

/// 다음 tick까지: 다음 회차 · 시세 간격(시세 조건이 있고 장이 열렸을 때) · 60초 가운데 가장 짧은 것.
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
            let until = at.signed_duration_since(now).to_std().unwrap_or_default();
            delay = delay.min(until);
        }
    }
    let quoting = trigger.config.conditions.iter().any(|job| {
        calendars
            .get(&job.market)
            .is_some_and(|calendar| calendar.session_at(now).is_some())
    });
    if quoting {
        delay = delay.min(trigger.config.quote_interval);
    }
    if delay.is_zero() {
        MIN_REFIRE_GAP
    } else {
        delay
    }
}
