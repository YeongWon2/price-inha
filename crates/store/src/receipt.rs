//! 트리거 작업 상태와 영수증(10차 2번). 원본 근거: `docs/research/triggers-approval-memory-deep.md` 1.2
//! (OpenClaw `cron_run_receipts`).
//!
//! `pi-trigger`의 `TriggerStore` 포트를 조립에서 이 저장소로 끼울 수 있게 모양과 규칙을 맞춘다
//! (store는 trigger에 기대지 않는다). 작업 상태는 trigger가 정한 모양을 JSON 값으로 받아 그대로 둔다.
//!
//! 표가 스스로 지킨다(어떤 SQL로도):
//! - 영수증 표는 `trigger_receipts`이고 칸 `job`(작업 id 글자) · `slot`(예정 시각, 밀리초 정수) ·
//!   `status` · `finished_at`(밀리초 정수)을 가진다. `(job, slot)`이 겹치지 않는다.
//! - 상태 칸 `status`는 `running` · `ok` · `error` · `interrupted` 가운데 하나다.
//! - `running`이면 닫은 시각 `finished_at`이 비어 있고, 아니면 채워져 있다.
//! - 한 작업(`job`)에 `running`인 줄은 하나뿐이다.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 영수증 키: `(작업, 예정 시각)`. 같은 회차는 키가 같아 두 번 접수되지 않는다.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TriggerReceiptKey {
    /// 작업 id.
    pub job: String,
    /// 예정 시각.
    pub slot: UnixMillis,
}

/// 영수증 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerReceiptStatus {
    /// 실행 중. 작업마다 하나뿐이다.
    Running,
    /// 끝났다.
    Ok,
    /// 실패했다.
    Error,
    /// 꺼졌다 켜져 중단됨으로 닫았다(다시 돌리지 않는다).
    Interrupted,
}

/// 깨울 대상에 넘기기를 시작했는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriggerDelivery {
    /// 아직 넘기지 않았다.
    NotStarted,
    /// 넘기기를 시작했다.
    Started,
}

/// 저장된 영수증.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerReceipt {
    /// 키.
    pub key: TriggerReceiptKey,
    /// 상태.
    pub status: TriggerReceiptStatus,
    /// 넘기기를 시작했는가.
    pub delivery: TriggerDelivery,
    /// 연 시각.
    pub started_at: UnixMillis,
    /// 닫은 시각. `Running`이면 없다.
    pub finished_at: Option<UnixMillis>,
}

/// 영수증을 열려고 한 결과. 같은 키를 먼저 보고(`Duplicate`), 그다음 실행 중인 것을 본다(`AlreadyRunning`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenTriggerReceipt {
    /// 열었다(`Running`, `NotStarted`).
    Opened,
    /// 같은 키의 영수증이 이미 있다(상태와 상관없이). 열지 않았다.
    Duplicate,
    /// 같은 작업의 다른 회차가 실행 중이다(그 키). 열지 않았다.
    AlreadyRunning(TriggerReceiptKey),
}

/// 저장된 작업 상태 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerJob {
    /// 작업 id.
    pub job: String,
    /// 상태 값(trigger가 정한 모양의 JSON). 넣은 그대로 나온다.
    pub state: serde_json::Value,
}

impl Store {
    /// 저장된 작업 상태들. 작업 id 순서다.
    pub fn trigger_jobs(&self) -> Result<Vec<TriggerJob>, StoreError> {
        self.read(|conn| {
            let mut stmt = conn.prepare("SELECT job, state FROM trigger_jobs ORDER BY job")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.map(|row| {
                let (job, state) = row?;
                Ok(TriggerJob {
                    job,
                    state: serde_json::from_str(&state)?,
                })
            })
            .collect()
        })
    }

    /// 작업 상태를 저장한다(같은 id는 바꾼다).
    pub fn save_trigger_job(&self, job: &str, state: &serde_json::Value) -> Result<(), StoreError> {
        self.write(|conn| save_job(conn, job, state))
    }

    /// 영수증을 연다. 규칙은 [`OpenTriggerReceipt`]의 각 값에 적었다.
    /// `Opened`이면 영수증은 `Running` · `NotStarted`이고 연 시각은 `started_at`이다.
    pub fn open_trigger_receipt(
        &self,
        key: &TriggerReceiptKey,
        started_at: UnixMillis,
    ) -> Result<OpenTriggerReceipt, StoreError> {
        self.write(|conn| {
            let outcome = check_open(conn, key)?;
            if outcome == OpenTriggerReceipt::Opened {
                insert_receipt(conn, key, started_at)?;
            }
            Ok(outcome)
        })
    }

    /// 작업 상태 옮기기와 영수증 열기를 **한 단위로** 한다(둘 다 되거나 둘 다 안 된다).
    ///
    /// - `Opened`: 상태를 저장하고 영수증을 열었다.
    /// - `Duplicate`: 상태는 저장하고(회차를 지나간 것으로 옮긴다) 영수증은 그대로 둔다.
    /// - `AlreadyRunning`: 아무것도 저장하지 않는다(상태가 그대로라 회차를 잃지 않는다).
    /// - `Err`: 아무것도 저장하지 않았다.
    pub fn save_job_and_open_receipt(
        &self,
        job: &str,
        state: &serde_json::Value,
        key: &TriggerReceiptKey,
        started_at: UnixMillis,
    ) -> Result<OpenTriggerReceipt, StoreError> {
        self.write(|conn| {
            let outcome = check_open(conn, key)?;
            match outcome {
                OpenTriggerReceipt::Opened => {
                    save_job(conn, job, state)?;
                    insert_receipt(conn, key, started_at)?;
                }
                OpenTriggerReceipt::Duplicate => save_job(conn, job, state)?,
                OpenTriggerReceipt::AlreadyRunning(_) => {}
            }
            Ok(outcome)
        })
    }

    /// 넘기기를 시작했다고 적는다. 영수증이 없으면 [`StoreError::TriggerReceiptNotFound`].
    pub fn mark_trigger_delivery_started(&self, key: &TriggerReceiptKey) -> Result<(), StoreError> {
        self.write(|conn| {
            let changed = conn.execute(
                "UPDATE trigger_receipts SET delivery = 'started' WHERE job = ?1 AND slot = ?2",
                params![key.job, key.slot.0],
            )?;
            if changed == 0 {
                return Err(StoreError::TriggerReceiptNotFound(key.clone()));
            }
            Ok(())
        })
    }

    /// 영수증을 닫는다(`Ok` · `Error` · `Interrupted`, 닫은 시각 `finished_at`).
    ///
    /// - `Running`으로는 닫을 수 없다: [`StoreError::TriggerReceiptRunningClose`]이고 그대로다.
    /// - 영수증이 없으면 [`StoreError::TriggerReceiptNotFound`].
    /// - 닫으면 그 작업은 다른 회차를 열 수 있다.
    pub fn close_trigger_receipt(
        &self,
        key: &TriggerReceiptKey,
        status: TriggerReceiptStatus,
        finished_at: UnixMillis,
    ) -> Result<(), StoreError> {
        let code = match status {
            TriggerReceiptStatus::Running => {
                return Err(StoreError::TriggerReceiptRunningClose(key.clone()));
            }
            other => status_code(other),
        };
        self.write(|conn| {
            let changed = conn.execute(
                "UPDATE trigger_receipts SET status = ?3, finished_at = ?4 \
                 WHERE job = ?1 AND slot = ?2",
                params![key.job, key.slot.0, code, finished_at.0],
            )?;
            if changed == 0 {
                return Err(StoreError::TriggerReceiptNotFound(key.clone()));
            }
            Ok(())
        })
    }

    /// 모든 영수증. 연 순서다.
    pub fn trigger_receipts(&self) -> Result<Vec<TriggerReceipt>, StoreError> {
        self.read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT job, slot, status, delivery, started_at, finished_at \
                 FROM trigger_receipts ORDER BY rowid",
            )?;
            let rows = stmt.query_map([], receipt_from)?;
            rows.map(|row| row?).collect()
        })
    }
}

fn save_job(conn: &Connection, job: &str, state: &serde_json::Value) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO trigger_jobs (job, state) VALUES (?1, ?2) \
         ON CONFLICT (job) DO UPDATE SET state = excluded.state",
        params![job, state.to_string()],
    )?;
    Ok(())
}

/// 같은 키를 먼저 보고, 그다음 같은 작업의 실행 중인 회차를 본다. 아무것도 쓰지 않는다.
fn check_open(
    conn: &Connection,
    key: &TriggerReceiptKey,
) -> Result<OpenTriggerReceipt, StoreError> {
    let duplicate: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM trigger_receipts WHERE job = ?1 AND slot = ?2)",
        params![key.job, key.slot.0],
        |row| row.get(0),
    )?;
    if duplicate {
        return Ok(OpenTriggerReceipt::Duplicate);
    }
    let running: Option<i64> = conn
        .query_row(
            "SELECT slot FROM trigger_receipts WHERE job = ?1 AND status = 'running'",
            [&key.job],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match running {
        Some(slot) => OpenTriggerReceipt::AlreadyRunning(TriggerReceiptKey {
            job: key.job.clone(),
            slot: UnixMillis(slot),
        }),
        None => OpenTriggerReceipt::Opened,
    })
}

fn insert_receipt(
    conn: &Connection,
    key: &TriggerReceiptKey,
    started_at: UnixMillis,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO trigger_receipts (job, slot, status, delivery, started_at) \
         VALUES (?1, ?2, 'running', 'not_started', ?3)",
        params![key.job, key.slot.0, started_at.0],
    )?;
    Ok(())
}

fn status_code(status: TriggerReceiptStatus) -> &'static str {
    match status {
        TriggerReceiptStatus::Running => "running",
        TriggerReceiptStatus::Ok => "ok",
        TriggerReceiptStatus::Error => "error",
        TriggerReceiptStatus::Interrupted => "interrupted",
    }
}

fn status_from(code: &str) -> Result<TriggerReceiptStatus, StoreError> {
    match code {
        "running" => Ok(TriggerReceiptStatus::Running),
        "ok" => Ok(TriggerReceiptStatus::Ok),
        "error" => Ok(TriggerReceiptStatus::Error),
        "interrupted" => Ok(TriggerReceiptStatus::Interrupted),
        other => Err(StoreError::Incompatible(format!(
            "모르는 영수증 상태 값: {other}"
        ))),
    }
}

fn delivery_from(code: &str) -> Result<TriggerDelivery, StoreError> {
    match code {
        "not_started" => Ok(TriggerDelivery::NotStarted),
        "started" => Ok(TriggerDelivery::Started),
        other => Err(StoreError::Incompatible(format!(
            "모르는 넘기기 값: {other}"
        ))),
    }
}

fn receipt_from(row: &Row<'_>) -> rusqlite::Result<Result<TriggerReceipt, StoreError>> {
    let build = || -> Result<TriggerReceipt, StoreError> {
        Ok(TriggerReceipt {
            key: TriggerReceiptKey {
                job: row.get(0)?,
                slot: UnixMillis(row.get(1)?),
            },
            status: status_from(&row.get::<_, String>(2)?)?,
            delivery: delivery_from(&row.get::<_, String>(3)?)?,
            started_at: UnixMillis(row.get(4)?),
            finished_at: row.get::<_, Option<i64>>(5)?.map(UnixMillis),
        })
    };
    Ok(build())
}
