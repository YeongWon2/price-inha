//! 승인 표 (9차 2번, M5). 원본: OpenClaw `operator_approvals`
//! (`docs/research/triggers-approval-memory-deep.md` 3.2 · 3.4).

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::{DenyReason, Transition};
use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 승인 요청 번호. 저장소가 1부터 붙이고 다시 쓰지 않는다(M5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApprovalId(pub i64);

/// 승인 요청의 결과(M5). 처음은 [`ApprovalStatus::Pending`]이고, 기다림에서 **한 번만** 바뀐다.
///
/// 표 `approvals`의 칸 `status`에 아래 글자로 두고, 표의 CHECK가 이 다섯 글자만 받는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApprovalStatus {
    /// 기다림 (`'pending'`)
    Pending,
    /// 승인 (`'approved'`)
    Approved,
    /// 거절 (`'rejected'`). 승인 확인에 실패한 것도 거절이다(호출하는 쪽이 정한다).
    Rejected,
    /// 답 없음 (`'no_answer'`)
    NoAnswer,
    /// 만료 (`'expired'`)
    Expired,
}

impl ApprovalStatus {
    /// 모든 결과.
    pub const ALL: [ApprovalStatus; 5] = [
        ApprovalStatus::Pending,
        ApprovalStatus::Approved,
        ApprovalStatus::Rejected,
        ApprovalStatus::NoAnswer,
        ApprovalStatus::Expired,
    ];
}

/// 새 승인 요청.
#[derive(Debug, Clone, PartialEq)]
pub struct NewApprovalRequest {
    /// 무엇을 승인하는지의 지문(예: 주문 내용의 SHA-256 글자). 그대로 저장한다.
    pub fingerprint: String,
    /// 사람에게 보일 내용(JSON). 그대로 저장한다.
    pub summary: serde_json::Value,
    /// 만든 시각.
    pub created_at: UnixMillis,
    /// 이 시각부터는 승인이 효력을 잃는다(답 · 소비 모두).
    pub expires_at: UnixMillis,
}

/// 저장된 승인 요청 한 건.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalRequest {
    /// 번호.
    pub id: ApprovalId,
    /// 내용 지문.
    pub fingerprint: String,
    /// 사람에게 보일 내용.
    pub summary: serde_json::Value,
    /// 지금 결과.
    pub status: ApprovalStatus,
    /// 만든 시각.
    pub created_at: UnixMillis,
    /// 효력이 끝나는 시각.
    pub expires_at: UnixMillis,
    /// 기다림에서 바뀐 시각. 기다림이면 없음.
    pub resolved_at: Option<UnixMillis>,
    /// 실행에 쓴(소비한) 시각. 승인이고 소비했을 때만 있다.
    pub consumed_at: Option<UnixMillis>,
}

/// 승인을 실행에 쓰겠다는 요청: 번호와 **실행할 내용**의 지문.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalUse {
    /// 쓸 승인 번호.
    pub id: ApprovalId,
    /// 실행할 내용의 지문. 승인한 내용의 지문과 글자까지 같아야 한다.
    pub fingerprint: String,
}

impl Store {
    /// 승인 요청을 기다림으로 만든다. 번호는 1부터 빠짐없이 커지고 다시 쓰지 않는다.
    pub fn create_approval(&self, new: NewApprovalRequest) -> Result<ApprovalRequest, StoreError> {
        self.write(|conn| create(conn, new))
    }

    /// 승인 요청 한 건. 없으면 [`StoreError::ApprovalNotFound`].
    pub fn get_approval(&self, id: ApprovalId) -> Result<ApprovalRequest, StoreError> {
        self.read(|conn| get(conn, id))
    }

    /// 그 결과인 승인 요청들. 번호 순서다.
    pub fn approvals_in_status(
        &self,
        status: ApprovalStatus,
    ) -> Result<Vec<ApprovalRequest>, StoreError> {
        self.read(|conn| in_status(conn, status))
    }

    /// 기다림인 승인 요청의 결과를 정한다. 기다림에서 **한 번만** 바뀐다.
    ///
    /// - 기다림이 아니면(이미 답이 났으면) [`StoreError::TransitionDenied`]
    ///   ([`crate::Transition::Approval`], [`crate::DenyReason::NotAllowed`])이고 그대로다.
    ///   두 번째 답은 첫 답을 바꾸지 못한다.
    /// - `to`가 기다림이면 같은 오류다.
    /// - (원본과 같게) `at`이 `expires_at` 이상이면 어떤 답이든 [`ApprovalStatus::Expired`]로 닫고
    ///   그것을 돌려준다(오류가 아니다).
    /// - 되면 `resolved_at`은 `at`이다.
    pub fn resolve_approval(
        &self,
        id: ApprovalId,
        to: ApprovalStatus,
        at: UnixMillis,
    ) -> Result<ApprovalRequest, StoreError> {
        self.write(|conn| resolve(conn, id, to, at))
    }

    /// 승인을 실행에 쓴다(한 번만). `consumed_at`을 `at`으로 적는다.
    ///
    /// 아래를 이 순서로 보고, 하나라도 걸리면 아무것도 바꾸지 않는다:
    ///
    /// 1. 없으면 [`StoreError::ApprovalNotFound`].
    /// 2. 승인이 아니면 [`StoreError::ApprovalNotUsable`].
    /// 3. 이미 썼으면 [`StoreError::ApprovalAlreadyConsumed`].
    /// 4. `at`이 `expires_at` 이상이면 [`StoreError::ApprovalExpired`].
    /// 5. 지문이 다르면 [`StoreError::ApprovalFingerprintMismatch`](승인 뒤 바뀐 내용은 실행하지 않는다).
    ///
    /// (11차) 4번과 5번 사이에서 하나 더 본다: 가장 최근 되살리기 표시([`Store::mark_restored`])의
    /// 시각과 같거나 그보다 먼저 만든(`created_at`) 승인은 승인됨이어도 [`StoreError::ApprovalExpired`]다
    /// (되살리면 전부 만료, M5 · V51).
    ///
    /// 표에서도 지킨다: `approvals`의 CHECK와 트리거가 승인이 아닌 줄의 소비 칸, 한 번 적힌 소비 칸의
    /// 고치기, 기다림이 아닌 줄의 결과 바꾸기, 줄 지우기를 어떤 SQL로도 막는다.
    pub fn consume_approval(
        &self,
        approval: ApprovalUse,
        at: UnixMillis,
    ) -> Result<ApprovalRequest, StoreError> {
        self.write(|conn| consume(conn, &approval, at))
    }
}

// ───────────── 안에서만 쓰는 것 ─────────────

const COLUMNS: &str =
    "id, fingerprint, summary, status, created_at, expires_at, resolved_at, consumed_at";

fn status_code(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::Pending => "pending",
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Rejected => "rejected",
        ApprovalStatus::NoAnswer => "no_answer",
        ApprovalStatus::Expired => "expired",
    }
}

fn status_from(code: &str) -> Result<ApprovalStatus, StoreError> {
    ApprovalStatus::ALL
        .into_iter()
        .find(|status| status_code(*status) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 승인 결과 값: {code}")))
}

fn request_from(row: &Row<'_>) -> Result<ApprovalRequest, StoreError> {
    Ok(ApprovalRequest {
        id: ApprovalId(row.get(0)?),
        fingerprint: row.get(1)?,
        summary: serde_json::from_str(&row.get::<_, String>(2)?)?,
        status: status_from(&row.get::<_, String>(3)?)?,
        created_at: UnixMillis(row.get(4)?),
        expires_at: UnixMillis(row.get(5)?),
        resolved_at: row.get::<_, Option<i64>>(6)?.map(UnixMillis),
        consumed_at: row.get::<_, Option<i64>>(7)?.map(UnixMillis),
    })
}

fn create(conn: &Connection, new: NewApprovalRequest) -> Result<ApprovalRequest, StoreError> {
    let summary = serde_json::to_string(&new.summary)?;
    conn.execute(
        "INSERT INTO approvals (fingerprint, summary, status, created_at, expires_at)
         VALUES (?1, ?2, 'pending', ?3, ?4)",
        params![new.fingerprint, summary, new.created_at.0, new.expires_at.0],
    )?;
    get(conn, ApprovalId(conn.last_insert_rowid()))
}

/// 승인 요청 한 건. 없으면 [`StoreError::ApprovalNotFound`].
pub(crate) fn get(conn: &Connection, id: ApprovalId) -> Result<ApprovalRequest, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM approvals WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let found = stmt
        .query_row([id.0], |row| Ok(request_from(row)))
        .optional()?;
    found.ok_or(StoreError::ApprovalNotFound(id))?
}

fn in_status(
    conn: &Connection,
    status: ApprovalStatus,
) -> Result<Vec<ApprovalRequest>, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM approvals WHERE status = ?1 ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([status_code(status)])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(request_from(row)?);
    }
    Ok(out)
}

fn resolve(
    conn: &Connection,
    id: ApprovalId,
    to: ApprovalStatus,
    at: UnixMillis,
) -> Result<ApprovalRequest, StoreError> {
    let now = get(conn, id)?;
    if now.status != ApprovalStatus::Pending || to == ApprovalStatus::Pending {
        return Err(StoreError::TransitionDenied {
            transition: Transition::Approval {
                from: now.status,
                to,
            },
            reason: DenyReason::NotAllowed,
        });
    }
    // 답하려는 순간 효력이 끝났으면 어떤 답이든 만료로 닫는다(원본과 같다).
    let status = if at.0 >= now.expires_at.0 {
        ApprovalStatus::Expired
    } else {
        to
    };
    conn.execute(
        "UPDATE approvals SET status = ?2, resolved_at = ?3
         WHERE id = ?1 AND status = 'pending'",
        params![id.0, status_code(status), at.0],
    )?;
    get(conn, id)
}

/// 가장 최근 되살리기 표시([`Store::mark_restored`]의 `restore_marks`)와 같거나 그보다 먼저 만든 승인인지.
/// 승인의 결과는 한 번 정해지면 바꿀 수 없으므로(승인됨이어도) 되살린 뒤에는 그 전에 만든 승인을
/// 쓸 수 없게 하는 길은 소비 때 거르는 것이다(M5 "되살리면 전부 만료", 조정자 결정 2026-11-16).
/// 표시는 팀 상태와 상관없이 늘 남으므로 이미 전체 정지였던 파일에서도 기준점이 있다. 같은 밀리초도 거부한다.
fn made_before_restore(conn: &Connection, created_at: UnixMillis) -> Result<bool, StoreError> {
    let restored_at: Option<i64> =
        conn.query_row("SELECT MAX(at) FROM restore_marks", [], |row| row.get(0))?;
    Ok(restored_at.is_some_and(|at| created_at.0 <= at))
}

/// 승인을 실행에 쓴다. 부르는 쪽의 묶음 안에서 돈다: 뒤에서 실패하면 소비도 함께 되돌려진다.
/// 순서와 오류는 [`Store::consume_approval`]과 같다.
pub(crate) fn consume(
    conn: &Connection,
    approval: &ApprovalUse,
    at: UnixMillis,
) -> Result<ApprovalRequest, StoreError> {
    let id = approval.id;
    let now = get(conn, id)?;
    if now.status != ApprovalStatus::Approved {
        return Err(StoreError::ApprovalNotUsable {
            id,
            status: now.status,
        });
    }
    if now.consumed_at.is_some() {
        return Err(StoreError::ApprovalAlreadyConsumed { id });
    }
    if at.0 >= now.expires_at.0 {
        return Err(StoreError::ApprovalExpired { id });
    }
    if made_before_restore(conn, now.created_at)? {
        return Err(StoreError::ApprovalExpired { id });
    }
    if now.fingerprint != approval.fingerprint {
        return Err(StoreError::ApprovalFingerprintMismatch { id });
    }
    // 조건을 다시 걸어 한 번만 적힌다(트리거도 막는다).
    let changed = conn.execute(
        "UPDATE approvals SET consumed_at = ?2
         WHERE id = ?1 AND status = 'approved' AND consumed_at IS NULL",
        params![id.0, at.0],
    )?;
    if changed != 1 {
        return Err(StoreError::ApprovalAlreadyConsumed { id });
    }
    get(conn, id)
}

/// 기다림인 승인을 모두 만료로 닫는다(`resolved_at = at`). 닫은 번호를 번호 순서로 준다.
/// 이미 답이 난 승인은 건드리지 않는다. 되살리기 표시([`Store::mark_restored`])가 쓴다.
pub(crate) fn expire_pending(
    conn: &Connection,
    at: UnixMillis,
) -> Result<Vec<ApprovalId>, StoreError> {
    let mut stmt = conn.prepare(
        "UPDATE approvals SET status = 'expired', resolved_at = ?1
         WHERE status = 'pending' RETURNING id",
    )?;
    let mut ids = stmt
        .query_map([at.0], |row| row.get(0).map(ApprovalId))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.sort();
    Ok(ids)
}
