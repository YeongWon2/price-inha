//! 깨우기 엔진: tick 하나의 순서, 다시 켠 뒤 처리, 타이머.
//!
//! tick 순서(원본: Hermes 1.1): 잠금 → 전체 정지 확인 → 저장소에서 작업 상태 다시 읽기 →
//! 기한 판정 → 작업 상태를 **먼저 옮겨 저장하고** → 영수증을 열고 → 깨운다 → 영수증을 닫는다.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::{Market, MarketAdapter, StalePolicy, Symbol};
use rust_decimal::Decimal;
use tokio::sync::watch;

use crate::{
    Alert, Clock, ConditionKind, Error, JobId, Monotonic, Receipt, ScheduleKind, ScheduleTimes,
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
}

impl TriggerConfig {
    /// 작업들과 기본값: 일정 간격 [`ScheduleTimes::default`], 다시 걸기 0.5%,
    /// 늦은 시세 30초, 시세 간격 [`DEFAULT_QUOTE_INTERVAL`], 이상은 [`Target::Team`].
    pub fn new(schedules: Vec<ScheduleJob>, conditions: Vec<ConditionJob>) -> Self {
        Self {
            schedules,
            conditions,
            times: ScheduleTimes::default(),
            rearm_percent: crate::default_rearm_percent(),
            stale: StalePolicy::default(),
            quote_interval: DEFAULT_QUOTE_INTERVAL,
            stale_target: Target::Team,
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
    /// 이번에 깨운 것들(실패한 것도 넣는다).
    pub wakes: Vec<Wake>,
    /// 이번에 생긴 알림 요구들.
    pub alerts: Vec<Alert>,
    /// 지난 tick 뒤로 벽시계 경과와 단조 시계 경과가 [`crate::CLOCK_JUMP_TOLERANCE`]보다 크게
    /// 달랐나(시계가 뒤로 감 · 절전에서 깸 · 시계 맞춤). 첫 tick은 거짓.
    pub clock_jumped: bool,
    /// 다음 tick까지 기다릴 시간: 다음 회차까지 · 시세 간격(시세 조건이 있고 장이 열렸을 때) ·
    /// [`crate::MAX_TIMER`] 가운데 가장 짧은 것. 0 이하이면 [`crate::MIN_REFIRE_GAP`].
    pub next_delay: Duration,
}

/// 다시 켠 뒤 처리의 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartReport {
    /// 실행 중으로 남아 있어 중단됨으로 닫은 영수증들(닫기 전 모습, `delivery` 포함). 다시 돌리지 않는다.
    pub interrupted: Vec<Receipt>,
}

/// 깨우기 엔진.
pub struct Trigger {
    config: TriggerConfig,
    ports: Ports,
}

impl Trigger {
    /// 엔진을 만든다. 설정을 확인한다: 작업 id가 겹치면 [`Error::DuplicateJob`], 작업의 시장 어댑터가
    /// 없으면 [`Error::MissingMarket`], 시세 조건 값이 잘못되면 [`Error::InvalidCondition`],
    /// 다시 걸기 간격이 음수이면 [`Error::InvalidConfig`].
    pub fn new(config: TriggerConfig, ports: Ports) -> Result<Self, Error> {
        let _ = (&config, &ports);
        todo!()
    }

    /// 다시 켠 뒤 처리(E16). `tick` 전에 한 번 부른다.
    ///
    /// 실행 중으로 남은 영수증을 모두 중단됨(`Interrupted`, 닫은 시각 = 지금)으로 닫고 **다시 돌리지 않는다**.
    /// 놓친 회차는 다음 `tick`이 다룬다.
    pub async fn start(&self) -> Result<StartReport, Error> {
        let _ = (&self.config, &self.ports);
        todo!()
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
    /// [`crate::AUTO_DISABLE_AFTER`]번이면 끄고 알림 요구를 낸다. 저장소에 쓰지 못하면 깨우지 않고 오류다.
    pub async fn tick(&self) -> Result<TickReport, Error> {
        todo!()
    }

    /// `start`를 부른 뒤 `tick` → `clock.sleep(next_delay)`를 되풀이한다. `stop`이 참이 되면 끝난다.
    /// tick의 오류는 기록하고 계속한다.
    pub async fn run(&self, stop: watch::Receiver<bool>) -> Result<(), Error> {
        let _ = stop;
        todo!()
    }

    /// 지금 시각(벽시계). 시험과 기록용.
    pub fn now(&self) -> DateTime<Utc> {
        todo!()
    }
}
