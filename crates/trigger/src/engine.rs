//! 깨우기 엔진: tick 하나의 순서, 다시 켠 뒤 처리, 타이머.
//!
//! tick 순서(원본: Hermes 1.1): 잠금 → 전체 정지 확인 → 저장소에서 작업 상태 다시 읽기 →
//! 기한 판정 → 작업 상태를 **먼저 옮겨 저장하고** → 영수증을 열고 → 깨운다 → 영수증을 닫는다.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::{Market, MarketAdapter, StalePolicy, Symbol};
use rust_decimal::Decimal;
use tokio::sync::watch;

use crate::batch::DataState;
use crate::inflight::InFlight;
use crate::watch::QuoteWatch;
use crate::{
    Alert, CLOCK_JUMP_TOLERANCE, Clock, ConditionKind, DataKind, DataTriggerConfig, ERROR_BACKOFF,
    Error, JobId, Monotonic, Receipt, ReceiptKey, ReceiptStatus, ScheduleKind, ScheduleTimes,
    StopSignal, Target, TriggerStore, Wake, Waker,
};

/// 시세를 받는 간격의 기본값(10초). 시세 조건이 있고 장이 열려 있을 때 쓴다.
pub const DEFAULT_QUOTE_INTERVAL: Duration = Duration::from_secs(10);

/// 일정 작업 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleJob {
    /// 작업 id(설정 안에서 하나뿐).
    pub id: JobId,
    /// 시장(그 시장의 달력에서 시각을 낸다).
    pub market: Market,
    /// 일정 종류.
    pub kind: ScheduleKind,
    /// 깨울 대상.
    pub target: Target,
}

/// 시세 조건 작업 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionJob {
    /// 작업 id(설정 안에서 하나뿐).
    pub id: JobId,
    /// 시장.
    pub market: Market,
    /// 종목.
    pub symbol: Symbol,
    /// 조건.
    pub kind: ConditionKind,
    /// 깨울 대상.
    pub target: Target,
}

/// 엔진 설정.
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerConfig {
    /// 일정 작업들.
    pub schedules: Vec<ScheduleJob>,
    /// 시세 조건 작업들.
    pub conditions: Vec<ConditionJob>,
    /// 일정 시각의 간격.
    pub times: ScheduleTimes,
    /// 다시 걸기 간격(%). 0 이상.
    pub rearm_percent: Decimal,
    /// 늦은 시세 기준(N6).
    pub stale: StalePolicy,
    /// 시세를 받는 간격.
    pub quote_interval: Duration,
    /// 늦은 시세(이상)로 깨울 대상.
    pub stale_target: Target,
    /// 자료 변화(뉴스 · 공시 · 재무 · 거시)로 깨우기(6차).
    pub data: DataTriggerConfig,
}

impl TriggerConfig {
    /// 작업들과 기본값: 일정 간격 [`ScheduleTimes::default`], 다시 걸기 0.5%,
    /// 늦은 시세 30초, 시세 간격 [`DEFAULT_QUOTE_INTERVAL`], 이상은 [`Target::Team`].
    /// 자료 변화: 네 종류 모두 받음, 대상 `Target::Member("scout")`, 중복 1시간, 조용한 시간 30초,
    /// 최대 기다림 300초, 출처마다 60초에 30개, 한 깨우기 상한 20.
    pub fn new(schedules: Vec<ScheduleJob>, conditions: Vec<ConditionJob>) -> Self {
        Self {
            schedules,
            conditions,
            times: ScheduleTimes::default(),
            rearm_percent: crate::default_rearm_percent(),
            stale: StalePolicy::default(),
            quote_interval: DEFAULT_QUOTE_INTERVAL,
            stale_target: Target::Team,
            data: DataTriggerConfig {
                enabled: vec![
                    DataKind::News,
                    DataKind::Disclosure,
                    DataKind::Financials,
                    DataKind::Macro,
                ],
                target: Target::Member("scout".to_string()),
                dedupe_for: Duration::from_secs(60 * 60),
                quiet: Duration::from_secs(30),
                max_wait: Duration::from_secs(300),
                per_source_window: Duration::from_secs(60),
                per_source_limit: 30,
                batch_limit: 20,
            },
        }
    }
}

/// 엔진이 쓰는 포트들.
#[derive(Clone)]
pub struct Ports {
    /// 벽시계와 기다림.
    pub clock: Arc<dyn Clock>,
    /// 단조 시계.
    pub monotonic: Arc<dyn Monotonic>,
    /// 시장 어댑터들(달력 · 시세). 시장마다 하나.
    pub markets: Vec<Arc<dyn MarketAdapter>>,
    /// 작업 상태 · 영수증 저장소.
    pub store: Arc<dyn TriggerStore>,
    /// 깨울 대상.
    pub waker: Arc<dyn Waker>,
    /// 전체 정지 표시.
    pub stop: Arc<dyn StopSignal>,
}

/// tick 하나가 어떻게 끝났나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickOutcome {
    /// 돌았다(깨운 것이 없어도).
    Ran,
    /// 다른 tick이 돌고 있어 아무것도 하지 않았다.
    Busy,
    /// 전체 정지 중이라 아무것도 하지 않았다. 기한이 된 회차는 다음 tick에 남는다.
    Stopped,
}

/// tick 하나의 결과.
#[derive(Debug, Clone, PartialEq)]
pub struct TickReport {
    /// 어떻게 끝났나.
    pub outcome: TickOutcome,
    /// 이번에 띄운 깨우기들. 띄우기만 하고 끝나기를 기다리지 않는다(결과는 [`Trigger::settle`]).
    pub wakes: Vec<Wake>,
    /// 지난 보고 뒤로 끝난 깨우기에서 생긴 알림 요구들 가운데 아직 내지 않은 것.
    /// 알림 요구 하나는 `tick`과 [`Trigger::settle`] 가운데 먼저 가져간 쪽이 한 번만 낸다.
    pub alerts: Vec<Alert>,
    /// 지난 tick 뒤로 벽시계 경과와 단조 시계 경과가 [`crate::CLOCK_JUMP_TOLERANCE`]보다 크게
    /// 달랐나(시계가 뒤로 감 · 절전에서 깸 · 시계 맞춤). 첫 tick은 거짓.
    pub clock_jumped: bool,
    /// 이번 tick에 상한 · 전체 정지로 버린 자료 사건 수(6차).
    pub data_dropped: usize,
    /// 다음 tick까지 기다릴 시간: 다음 회차까지 · 시세 간격(시세 조건이 있고 장이 열렸을 때) ·
    /// [`crate::MAX_TIMER`] 가운데 가장 짧은 것. 0 이하이면 [`crate::MIN_REFIRE_GAP`].
    pub next_delay: Duration,
}

/// 다시 켠 뒤 처리의 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartReport {
    /// 실행 중으로 남아 있어 중단됨으로 닫은 영수증들(닫기 전 모습, `delivery` 포함). 다시 돌리지 않는다.
    pub interrupted: Vec<Receipt>,
    /// 상태는 옮겨졌는데 영수증이 없어 되살린 회차들([`crate::JobState::pending_slot`], 원본 Hermes의
    /// 주인 없는 대기 회차). 다음 `tick`이 그 회차로 발사한다.
    pub revived: Vec<ReceiptKey>,
}

/// 띄워 둔 깨우기 하나가 끝난 결과.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    /// 끝난 깨우기(띄울 때 [`TickReport::wakes`]에 넣은 것과 같다).
    pub wake: Wake,
    /// 성공했나. 거짓이면 실패로 셌다(물러서기 · 10번이면 끄기).
    pub ok: bool,
}

/// [`Trigger::settle`]의 결과.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettleReport {
    /// 지난 `settle` 뒤로 끝난 깨우기들(끝난 순). 한 번 낸 것은 다시 내지 않는다.
    pub finished: Vec<Finished>,
    /// 아직 내지 않은 알림 요구들(`tick`이 먼저 가져간 것은 빠진다).
    pub alerts: Vec<Alert>,
}

/// [`Trigger::drain_finished`]의 결과(3차 2번).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Drained {
    /// 지난 가져가기(`drain_finished` · `settle`) 뒤로 끝난 깨우기들(끝난 순). 한 번 낸 것은 다시 내지 않는다.
    pub finished: Vec<Finished>,
    /// 끝난 결과를 반영하다 생긴 저장소 오류들(생긴 순). 한 번 낸 것은 다시 내지 않는다.
    pub errors: Vec<crate::StoreError>,
}

/// 깨우기 엔진.
pub struct Trigger {
    pub(crate) config: TriggerConfig,
    pub(crate) ports: Ports,
    /// tick 하나만 돈다(원본: Hermes tick 잠금). 잡지 못하면 기다리지 않고 바쁨으로 끝난다.
    pub(crate) running: tokio::sync::Mutex<()>,
    /// 지난 tick의 벽시계 · 단조 시계.
    last_seen: Mutex<Option<(DateTime<Utc>, Duration)>>,
    /// 종목마다의 늦은 시세 감시.
    pub(crate) watches: Mutex<HashMap<(Market, Symbol), QuoteWatch>>,
    /// 띄워 둔 깨우기들과 끝난 결과.
    pub(crate) inflight: Arc<InFlight>,
    /// 다시 켤 때 되살려 다음 tick이 발사할 회차들.
    pub(crate) revived: Mutex<Vec<ReceiptKey>>,
    /// 지금 보는 시세 조건들([`Trigger::replace_conditions`]가 바꾼다).
    pub(crate) conditions: Mutex<Vec<ConditionJob>>,
    /// 자료 변화 사건의 중복 표 · 속도 제한 · 묶음(메모리에만).
    pub(crate) data: Mutex<DataState>,
}

impl Trigger {
    /// 엔진을 만든다. 설정을 확인한다: 작업 id가 겹치면 [`Error::DuplicateJob`], 작업의 시장 어댑터가
    /// 없으면 [`Error::MissingMarket`], 시세 조건 값이 잘못되면 [`Error::InvalidCondition`],
    /// 다시 걸기 간격이 음수이면 [`Error::InvalidConfig`].
    pub fn new(config: TriggerConfig, ports: Ports) -> Result<Self, Error> {
        validate(&config.schedules, &config.conditions, &ports.markets)?;
        if config.rearm_percent.is_sign_negative() {
            return Err(Error::InvalidConfig("다시 걸기 간격은 0 이상이어야 한다"));
        }
        if config.quote_interval.is_zero() {
            return Err(Error::InvalidConfig("시세 간격은 0보다 커야 한다"));
        }
        Ok(Self {
            conditions: Mutex::new(config.conditions.clone()),
            data: Mutex::new(DataState::default()),
            config,
            ports,
            running: tokio::sync::Mutex::new(()),
            last_seen: Mutex::new(None),
            watches: Mutex::new(HashMap::new()),
            inflight: Arc::new(InFlight::default()),
            revived: Mutex::new(Vec::new()),
        })
    }

    /// 다시 켠 뒤 처리(E16). `tick` 전에 한 번 부른다.
    ///
    /// 실행 중으로 남은 영수증을 모두 중단됨(`Interrupted`, 닫은 시각 = 지금)으로 닫고 **다시 돌리지 않는다**.
    /// 상태는 옮겼는데(`pending_slot`) 그 회차의 영수증이 없으면 되살려 다음 `tick`이 그 회차로 발사한다
    /// (원본 Hermes 기한 판정 2단계, 2차 3번). 놓친 회차는 다음 `tick`이 다룬다.
    pub async fn start(&self) -> Result<StartReport, Error> {
        let now = self.now();
        let mut interrupted = Vec::new();
        for receipt in self.ports.store.receipts().await? {
            if receipt.status != ReceiptStatus::Running {
                continue;
            }
            self.ports
                .store
                .close_receipt(&receipt.key, ReceiptStatus::Interrupted, now)
                .await?;
            tracing::warn!(
                job = %receipt.key.job,
                slot = %receipt.key.slot,
                delivery = ?receipt.delivery,
                "실행 중으로 남은 영수증을 중단됨으로 닫았다(다시 돌리지 않는다)"
            );
            interrupted.push(receipt);
        }
        let revived = self.orphans().await?;
        lock(&self.revived).clone_from(&revived);
        Ok(StartReport {
            interrupted,
            revived,
        })
    }

    /// 상태를 옮겼는데(`pending_slot`) 그 회차의 영수증이 없는 일정 작업의 회차들(원본 Hermes 주인 없는 대기 회차).
    /// 영수증이 있으면(닫혔든 실행 중이든) 되살리지 않는다.
    async fn orphans(&self) -> Result<Vec<ReceiptKey>, Error> {
        let receipts = self.ports.store.receipts().await?;
        let mut revived = Vec::new();
        for state in self.ports.store.jobs().await? {
            let Some(slot) = state.pending_slot else {
                continue;
            };
            if !self.config.schedules.iter().any(|job| job.id == state.id) {
                continue;
            }
            let key = ReceiptKey {
                job: state.id,
                slot,
            };
            if receipts.iter().any(|receipt| receipt.key == key) {
                continue;
            }
            tracing::warn!(job = %key.job, slot = %key.slot, "영수증 없이 옮겨진 회차를 되살린다");
            revived.push(key);
        }
        Ok(revived)
    }

    /// tick 하나.
    ///
    /// 일정: 저장된 `last_slot`보다 뒤이고 지금 이하인 회차가 기한이다. 가장 늦은 회차의 늦은 정도가
    /// 유예([`crate::grace_for`], 하루 주기 → 2시간) 이하면 지금 발사하고 앞의 회차들은 `missed`로 센다.
    /// 유예를 넘었으면 정규장이 열려 있을 때만 한 번 따라잡고, 장 밖이면 발사하지 않고
    /// `pending_catch_up`에 적어 다음 정규장 시작(정규장이 열린 첫 tick)에 [`crate::TriggerEvent::CatchUp`]
    /// 하나로 합쳐 넘긴다(E18, 결정 20).
    ///
    /// 시세 조건: 그 시장의 세션이 열려 있을 때만 시세를 받는다. 깨우기가 성공했을 때만 다시 걸기 상태를
    /// 저장한다. 늦은 시세(N6)는 [`pi_market::is_stale`]로 보고, 늦어진 동안 한 번만 이상으로 깨운다
    /// (새 시세가 오면 풀린다). 한 번도 받지 못한 종목은 처음 받으려 한 시각부터 잰다.
    /// 늦은 시세 깨우기에는 영수증 · 물러서기를 쓰지 않는다.
    ///
    /// 실패하면 연속 실패 수를 올리고 `retry_at` = 끝난 시각 + [`crate::error_backoff`]로 물러서며,
    /// [`crate::AUTO_DISABLE_AFTER`]번이면 끄고 알림 요구를 낸다.
    ///
    /// 발사는 작업마다 "상태 옮기기 + 영수증 열기"를 한 단위([`crate::TriggerStore::save_and_open`])로 한다.
    /// 한 작업의 쓰기가 실패하면 그 작업만 이번 발사에서 빠지고(상태 그대로, 회차는 기한으로 남는다)
    /// 나머지는 깨운다. 이번 tick에 연 영수증을 깨우지 못했으면 실패(`Error`)로 닫아 실행 중으로 남기지 않는다.
    /// 빠진 작업이 있으면 나머지를 띄운 뒤 [`Error::Store`]를 돌려준다.
    ///
    /// 깨우기는 대상마다 따로 띄우고 끝나기를 기다리지 않는다(tick 잠금을 바로 놓는다). 결과가 오면 영수증을
    /// 닫고 상태를 고친다. 실행 중 영수증이 같은 작업의 중복을 막는다. 끝나기를 기다리려면 [`Trigger::settle`].
    pub async fn tick(&self) -> Result<TickReport, Error> {
        crate::tick::tick(self).await
    }

    /// 띄워 둔 깨우기가 모두 끝날 때까지 기다린다. 끝난 결과는 띄운 쪽이 이미 반영했다(영수증 닫기 ·
    /// 연속 실패 수 · 물러서기 · 끄기 · 다시 걸기 상태). 지난 `settle` 뒤로 끝난 것과 아직 내지 않은
    /// 알림 요구를 돌려준다. 결과를 반영하다 저장소가 실패했으면 [`Error::Store`].
    pub async fn settle(&self) -> Result<SettleReport, Error> {
        self.inflight.settle().await
    }

    /// 지금 띄워 두고 아직 끝나지 않은 깨우기 수.
    pub fn in_flight(&self) -> usize {
        self.inflight.count()
    }

    /// 띄운 깨우기를 **기다리지 않고**, 지금까지 끝난 결과와 반영하다 생긴 저장소 오류를 가져간다(3차 2번).
    /// 가져간 것은 비운다(다음 `drain_finished` · [`Trigger::settle`]에 다시 나오지 않는다).
    /// 알림 요구는 가져가지 않는다(`tick` · `settle`이 낸다). [`Trigger::run`]은 되풀이마다 이것으로 비워
    /// 끝난 결과가 끝없이 쌓이지 않게 한다.
    pub fn drain_finished(&self) -> Drained {
        self.inflight.drain()
    }

    /// `start`를 부른 뒤 `tick` → `clock.sleep(next_delay)`를 되풀이한다. `stop`이 참이 되면 끝난다.
    /// tick의 오류는 기록하고 계속한다.
    pub async fn run(&self, mut stop: watch::Receiver<bool>) -> Result<(), Error> {
        self.start().await?;
        loop {
            self.drain_logged();
            if *stop.borrow() {
                return Ok(());
            }
            let delay = match self.tick().await {
                Ok(report) => {
                    if report.data_dropped > 0 {
                        let dropped = report.data_dropped;
                        tracing::warn!(dropped, "이번 tick 보고에 버린 자료 사건이 있다");
                    }
                    report.next_delay
                }
                Err(error) => {
                    tracing::warn!(%error, "tick이 실패했다. 잠시 뒤 다시 본다");
                    ERROR_BACKOFF[0]
                }
            };
            self.drain_logged();
            if *stop.borrow() {
                return Ok(());
            }
            let sleep = self.ports.clock.sleep(delay);
            tokio::pin!(sleep);
            tokio::select! {
                () = &mut sleep => {}
                changed = stop.changed() => {
                    if changed.is_err() {
                        // 멈추라고 할 쪽이 사라졌다. 기다림은 마저 채운다.
                        sleep.await;
                    }
                }
            }
        }
    }

    /// 끝난 결과를 비운다(`run`이 쌓아 두지 않게). 반영하다 생긴 저장소 오류는 기록한다.
    fn drain_logged(&self) {
        let drained = self.drain_finished();
        for error in &drained.errors {
            tracing::warn!(%error, "끝난 깨우기의 결과를 저장소에 적지 못했다");
        }
    }

    /// 지금 시각(벽시계). 시험과 기록용.
    pub fn now(&self) -> DateTime<Utc> {
        self.ports.clock.now()
    }

    /// 지난 tick 뒤로 벽시계 경과와 단조 시계 경과가 허용치보다 크게 달랐나(원본: OpenClaw tick 1단계).
    pub(crate) fn clock_jumped(&self, wall: DateTime<Utc>, mono: Duration) -> bool {
        let mut last = lock(&self.last_seen);
        let jumped = last.is_some_and(|(last_wall, last_mono)| {
            let wall_elapsed = wall.signed_duration_since(last_wall);
            let mono_elapsed = chrono::Duration::from_std(mono.saturating_sub(last_mono))
                .unwrap_or(chrono::TimeDelta::MAX);
            let tolerance =
                chrono::Duration::from_std(CLOCK_JUMP_TOLERANCE).unwrap_or(chrono::TimeDelta::MAX);
            (wall_elapsed - mono_elapsed).abs() > tolerance
        });
        *last = Some((wall, mono));
        if jumped {
            tracing::warn!(%wall, "벽시계가 단조 시계와 다르게 움직였다(시계 맞춤 · 절전에서 깸)");
        }
        jumped
    }

    /// 지금 보는 시세 조건들(복사본).
    pub(crate) fn conditions(&self) -> Vec<ConditionJob> {
        lock(&self.conditions).clone()
    }

    /// 그 시장의 어댑터.
    pub(crate) fn adapter(&self, market: Market) -> Option<&Arc<dyn MarketAdapter>> {
        self.ports.markets.iter().find(|m| m.market() == market)
    }
}

/// 작업 설정을 확인한다: id 겹침 · 시장 어댑터 없음 · 시세 조건 값.
pub(crate) fn validate(
    schedules: &[ScheduleJob],
    conditions: &[ConditionJob],
    markets: &[Arc<dyn MarketAdapter>],
) -> Result<(), Error> {
    let has_market = |market: Market| markets.iter().any(|m| m.market() == market);
    let mut seen = HashSet::new();
    let jobs = schedules
        .iter()
        .map(|job| (&job.id, job.market))
        .chain(conditions.iter().map(|job| (&job.id, job.market)));
    for (id, market) in jobs {
        if !seen.insert(id) {
            return Err(Error::DuplicateJob(id.clone()));
        }
        if !has_market(market) {
            return Err(Error::MissingMarket(id.clone()));
        }
    }
    if let Some(job) = conditions
        .iter()
        .find(|job| !crate::condition::is_valid(&job.kind))
    {
        return Err(Error::InvalidCondition(job.id.clone()));
    }
    Ok(())
}

/// 잠금 안의 값은 한 번에 하나씩만 바꾸므로, 다른 스레드가 잠금을 쥔 채 죽었어도 그대로 이어 쓴다.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
