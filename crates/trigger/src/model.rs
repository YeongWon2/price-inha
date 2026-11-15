//! 작업 · 대상 · 사건 · 깨우기 값.

use std::fmt;

use chrono::{DateTime, Utc};
use pi_market::{Market, Symbol};
use rust_decimal::Decimal;

use crate::{ConditionKind, Direction};

/// 작업 id. 영수증 키 `(작업, 예정 시각)`의 앞쪽이다.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(String);

impl JobId {
    /// 글자 그대로 id를 만든다.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// id 글자.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 깨울 대상.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    /// 팀 전체(팀장이 받는다).
    Team,
    /// 담당 하나(담당 id).
    Member(String),
}

/// 일정의 종류. 시각은 시장 달력의 정규장에서 낸다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScheduleKind {
    /// 장 시작 전 브리핑: 정규장 시작 − [`crate::ScheduleTimes::briefing_lead`].
    PreOpenBriefing,
    /// 장 시작: 정규장 시작.
    MarketOpen,
    /// 장 마감: 정규장 끝.
    MarketClose,
    /// 마감 뒤(복기 · 백업): 정규장 끝 + [`crate::ScheduleTimes::after_close_delay`].
    AfterClose,
}

/// 깨우기에 묶여 넘어가는 사건 하나.
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerEvent {
    /// 일정의 회차가 되었다.
    Schedule {
        /// 작업.
        job: JobId,
        /// 일정 종류.
        kind: ScheduleKind,
        /// 시장.
        market: Market,
        /// 이번에 발사한 회차의 예정 시각.
        slot: DateTime<Utc>,
        /// 이 발사에 합친, 그보다 앞서 놓친 회차 수(제때면 0).
        missed: u32,
    },
    /// 장 밖이라 따라잡지 않고 다음 장 시작으로 미뤄 둔 놓친 회차들(E18, 결정 20).
    CatchUp {
        /// 작업.
        job: JobId,
        /// 일정 종류.
        kind: ScheduleKind,
        /// 시장.
        market: Market,
        /// 놓친 회차의 예정 시각들(이른 순).
        slots: Vec<DateTime<Utc>>,
    },
    /// 시세 조건이 맞았다.
    Price {
        /// 작업(시세 조건).
        job: JobId,
        /// 종목.
        symbol: Symbol,
        /// 조건.
        condition: ConditionKind,
        /// 맞은 방향.
        direction: Direction,
        /// 알린 값(그때의 가격).
        price: Decimal,
        /// 시세를 받은 시각.
        at: DateTime<Utc>,
    },
    /// 시세가 늦다(N6). 이상 깨움.
    StaleQuote {
        /// 시장.
        market: Market,
        /// 종목.
        symbol: Symbol,
        /// 마지막으로 받은 시각. 한 번도 못 받았으면 없다.
        last_received: Option<DateTime<Utc>>,
    },
}

/// 깨우는 이유. 한 깨우기에 여러 사건이 묶이면 `Anomaly` > `CatchUp` > `Schedule` > `Price` 순으로 앞의 것.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WakeReason {
    /// 이상(늦은 시세 등).
    Anomaly,
    /// 미뤄 둔 놓친 회차를 장 시작에 넘긴다.
    CatchUp,
    /// 정해진 시간.
    Schedule,
    /// 시세 조건.
    Price,
}

/// [`crate::Waker`]에 넘기는 깨우기 하나. 대상 하나에 한 번의 tick에서 생긴 사건을 묶는다.
#[derive(Debug, Clone, PartialEq)]
pub struct Wake {
    /// 깨울 대상.
    pub target: Target,
    /// 이유.
    pub reason: WakeReason,
    /// 묶인 사건들(생긴 순).
    pub events: Vec<TriggerEvent>,
}

/// 조립하는 쪽에 내는 알림 요구.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// 작업.
    pub job: JobId,
    /// 까닭.
    pub reason: AlertReason,
}

/// 알림 요구의 까닭.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AlertReason {
    /// 연달아 실패해 작업을 껐다.
    AutoDisabled {
        /// 연속 실패 수.
        consecutive_errors: u32,
    },
}
