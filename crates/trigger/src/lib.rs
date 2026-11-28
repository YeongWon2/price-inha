//! pi-trigger: 깨우기(일정 · 시세 조건 · 영수증). 구성 요소 A5.
//!
//! 작업 지시서: `docs/dev/modules/trigger.md`.
//!
//! - 일정(장 시작 전 브리핑 · 장 시작 · 장 마감 · 마감 뒤)은 [`pi_market::MarketCalendar`]에서 낸다.
//! - 시세 조건(넘음 · 내려감 · 변동률)과 다시 걸기(결정 21), 늦은 시세 감시(N6).
//! - 자료 변화(뉴스 · 공시 · 재무 · 거시) 사건의 중복 막기 · 속도 제한 · 묶어 보내기(6차).
//! - 영수증 `(작업, 예정 시각)`과 다시 켠 뒤 처리(E16), 놓친 회차(E18, 결정 20).
//! - 실패하면 물러서고 10번이면 끈다.
//! - 판단 · 저장소 직접 쓰기 · 바깥 HTTP 입구는 없다. 저장은 [`TriggerStore`], 깨우기는 [`Waker`] 포트다.
//! - 시각은 [`Clock`](벽시계) · [`Monotonic`](단조 시계)으로만 받는다.

mod batch;
mod clock;
mod condition;
mod data;
mod discard;
mod engine;
pub mod error;
mod inflight;
mod model;
mod plan;
mod queue;
mod returned;
mod schedule;
mod store;
mod tick;
mod waker;
mod watch;

pub use clock::{CLOCK_JUMP_TOLERANCE, Clock, MAX_TIMER, MIN_REFIRE_GAP, Monotonic, StopSignal};
pub use condition::{
    ConditionKind, ConditionMemory, Direction, Observation, default_rearm_percent, observe,
};
pub use data::{
    DEFAULT_WATCH_CHANGE_PERCENT, DataChange, DataKind, DataPush, DataTriggerConfig,
    watch_conditions,
};
pub use engine::{
    ConditionJob, DEFAULT_QUOTE_INTERVAL, Drained, Finished, Ports, ScheduleJob, SettleReport,
    StartReport, TickOutcome, TickReport, Trigger, TriggerConfig,
};
pub use error::{Error, StoreError, WakeError};
pub use model::{Alert, AlertReason, JobId, ScheduleKind, Target, TriggerEvent, Wake, WakeReason};
pub use queue::{LANE_CAPACITY, Lane, PushOutcome, QueuedItem, WakeQueue};
pub use schedule::{
    AUTO_DISABLE_AFTER, ERROR_BACKOFF, MAX_GRACE, MIN_GRACE, ScheduleTimes, error_backoff,
    grace_for, schedule_slots,
};
pub use store::{
    DeliveryState, DisableReason, JobState, MemoryStore, OpenReceipt, Receipt, ReceiptKey,
    ReceiptStatus, TriggerStore,
};
pub use waker::Waker;
