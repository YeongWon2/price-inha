//! 작업 상태와 영수증의 저장소 포트(조립에서 store를 끼운다)와 메모리 구현.

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{ConditionMemory, JobId, StoreError};

/// 작업 하나의 저장되는 상태. tick마다 저장소에서 다시 읽는다.
#[derive(Debug, Clone, PartialEq)]
pub struct JobState {
    /// 작업.
    pub id: JobId,
    /// 마지막으로 처리한(발사했거나 놓친 것으로 적은) 회차의 예정 시각. 이 시각 이하의 회차는 다시 보지 않는다.
    /// 처음 보는 작업은 그때의 지금 시각으로 채운다(지난 회차를 놓친 것으로 보지 않는다).
    pub last_slot: Option<DateTime<Utc>>,
    /// 연속 실패 수. 성공하면 0.
    pub consecutive_errors: u32,
    /// 물러선 뒤 다시 해도 되는 시각(끝난 시각 + 오류 지연). 그 전에는 발사하지 않는다.
    pub retry_at: Option<DateTime<Utc>>,
    /// 꺼졌으면 그 까닭. 꺼진 작업은 발사하지 않는다.
    pub disabled: Option<DisableReason>,
    /// 장 밖이라 다음 장 시작으로 미뤄 둔 놓친 회차들(이른 순).
    pub pending_catch_up: Vec<DateTime<Utc>>,
    /// 시세 조건의 다시 걸기 상태. 일정 작업은 `None`. 시세 조건인데 `None`이면 `Armed`로 본다.
    pub condition: Option<ConditionMemory>,
}

impl JobState {
    /// 처음 보는 작업의 상태: `last_slot`만 정하고 나머지는 비었다.
    pub fn new(id: JobId, last_slot: Option<DateTime<Utc>>) -> Self {
        Self {
            id,
            last_slot,
            consecutive_errors: 0,
            retry_at: None,
            disabled: None,
            pending_catch_up: Vec::new(),
            condition: None,
        }
    }
}

/// 작업을 끈 까닭.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DisableReason {
    /// 연달아 [`crate::AUTO_DISABLE_AFTER`]번 실패했다.
    ConsecutiveFailures,
    /// 사람이 껐다.
    Manual,
}

/// 영수증 키: `(작업, 예정 시각)`. 같은 회차는 키가 같아 두 번 접수되지 않는다.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReceiptKey {
    /// 작업.
    pub job: JobId,
    /// 예정 시각(시세 조건 · 늦은 시세는 그 tick의 시각).
    pub slot: DateTime<Utc>,
}

/// 영수증 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptStatus {
    /// 실행 중. 작업마다 하나뿐이다.
    Running,
    /// 끝났다.
    Ok,
    /// 실패했다.
    Error,
    /// 꺼졌다 켜져 중단됨으로 닫았다(다시 돌리지 않는다).
    Interrupted,
}

/// 깨울 대상에 넘기기를 시작했는가. 다시 켠 뒤 "보냈을지도 모르는 것"을 다시 보내지 않으려고 적는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryState {
    /// 아직 넘기지 않았다.
    NotStarted,
    /// 넘기기를 시작했다.
    Started,
}

/// 영수증.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// 키.
    pub key: ReceiptKey,
    /// 상태.
    pub status: ReceiptStatus,
    /// 넘기기를 시작했는가.
    pub delivery: DeliveryState,
    /// 연 시각.
    pub started_at: DateTime<Utc>,
    /// 닫은 시각. `Running`이면 없다.
    pub finished_at: Option<DateTime<Utc>>,
}

/// 영수증을 열려고 한 결과. 같은 키를 먼저 보고(`Duplicate`), 그다음 실행 중인 것을 본다(`AlreadyRunning`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenReceipt {
    /// 열었다(`Running`, `NotStarted`).
    Opened,
    /// 같은 키의 영수증이 이미 있다(상태와 상관없이). 열지 않았다.
    Duplicate,
    /// 같은 작업의 다른 회차가 실행 중이다. 열지 않았다.
    AlreadyRunning(ReceiptKey),
}

/// 작업 상태와 영수증 저장소(포트). 조립에서 store를 끼운다.
#[async_trait]
pub trait TriggerStore: Send + Sync {
    /// 저장된 작업 상태들.
    async fn jobs(&self) -> Result<Vec<JobState>, StoreError>;
    /// 작업 상태를 저장한다(같은 id는 바꾼다).
    async fn save_job(&self, job: &JobState) -> Result<(), StoreError>;
    /// 영수증을 연다. 규칙은 [`OpenReceipt`]의 각 값에 적었다.
    async fn open_receipt(
        &self,
        key: &ReceiptKey,
        started_at: DateTime<Utc>,
    ) -> Result<OpenReceipt, StoreError>;
    /// 넘기기를 시작했다고 적는다.
    async fn mark_delivery_started(&self, key: &ReceiptKey) -> Result<(), StoreError>;
    /// 영수증을 닫는다(`Ok` · `Error` · `Interrupted`).
    async fn close_receipt(
        &self,
        key: &ReceiptKey,
        status: ReceiptStatus,
        finished_at: DateTime<Utc>,
    ) -> Result<(), StoreError>;
    /// 모든 영수증(연 순).
    async fn receipts(&self) -> Result<Vec<Receipt>, StoreError>;
}

/// 메모리 안의 저장소. 테스트와 시연용이고, 같은 규칙을 지킨다.
#[derive(Debug, Default)]
pub struct MemoryStore {
    state: Mutex<MemoryState>,
}

#[derive(Debug, Default)]
struct MemoryState {
    jobs: Vec<JobState>,
    receipts: Vec<Receipt>,
}

impl MemoryStore {
    /// 빈 저장소.
    pub fn new() -> Self {
        Self::default()
    }

    /// 영수증을 그대로 넣는다(다시 켠 뒤를 시험할 때 "꺼지기 전" 상태를 만든다). 규칙을 보지 않는다.
    pub fn insert_receipt(&self, receipt: Receipt) {
        let _ = (&self.state, receipt);
        todo!()
    }
}

#[async_trait]
impl TriggerStore for MemoryStore {
    async fn jobs(&self) -> Result<Vec<JobState>, StoreError> {
        todo!()
    }

    async fn save_job(&self, job: &JobState) -> Result<(), StoreError> {
        let _ = job;
        todo!()
    }

    async fn open_receipt(
        &self,
        key: &ReceiptKey,
        started_at: DateTime<Utc>,
    ) -> Result<OpenReceipt, StoreError> {
        let _ = (key, started_at);
        todo!()
    }

    async fn mark_delivery_started(&self, key: &ReceiptKey) -> Result<(), StoreError> {
        let _ = key;
        todo!()
    }

    async fn close_receipt(
        &self,
        key: &ReceiptKey,
        status: ReceiptStatus,
        finished_at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        let _ = (key, status, finished_at);
        todo!()
    }

    async fn receipts(&self) -> Result<Vec<Receipt>, StoreError> {
        todo!()
    }
}
