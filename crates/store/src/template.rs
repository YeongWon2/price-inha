//! 템플릿 표와 전이 (9차 1번, M3 · E36 · V29 · V30).
//!
//! 템플릿의 값(R% · 한도 · 손절 방식 · 승격 기준 등)이 무엇인지, 승격 기준을 넘었는지는 `pi-template`이
//! 판단한다. 저장소는 판 번호 붙은 값과 상태, 그리고 상태표에 없는 전이를 막는 것만 맡는다.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::approval::{self, ApprovalId, ApprovalUse};
use crate::error::{DenyReason, Transition};
use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 템플릿 번호. 저장소가 1부터 붙인다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TemplateId(pub i64);

/// 템플릿 상태(`spec.md` 상태표 · M3). 새 템플릿은 [`TemplateState::Simulating`]이다(V29).
///
/// 표 `templates`의 칸 `state`에 아래 글자로 둔다. 허용 전이(이 밖은 모두 거부):
///
/// | 에서 | 으로 | 함수 |
/// |---|---|---|
/// | 모의중 | 승격대기 | [`Store::submit_template_for_promotion`] (기준 통과 기록을 함께 받는다) |
/// | 승격대기 | 모의중 | [`Store::reject_template_promotion`] (내가 승격을 거절) |
/// | 승격대기 | 실전중 | [`Store::promote_template_live`] (승인을 소비한다) |
/// | 실전중 | 멈춤 | [`Store::stop_template`], 또는 투자 팀이 전체 정지가 될 때 저절로 |
///
/// 멈춤에서 나가는 전이는 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateState {
    /// 모의중 (`'simulating'`)
    Simulating,
    /// 승격대기 (`'promotion_pending'`)
    PromotionPending,
    /// 실전중 (`'live'`)
    Live,
    /// 멈춤 (`'stopped'`)
    Stopped,
}

impl TemplateState {
    /// 모든 상태.
    pub const ALL: [TemplateState; 4] = [
        TemplateState::Simulating,
        TemplateState::PromotionPending,
        TemplateState::Live,
        TemplateState::Stopped,
    ];
}

/// 새 템플릿.
#[derive(Debug, Clone, PartialEq)]
pub struct NewTemplate {
    /// 템플릿 값(JSON). 판 1이 된다. 그대로 저장한다.
    pub value: serde_json::Value,
    /// 만든 시각.
    pub at: UnixMillis,
}

/// 승격 기준을 통과했다는 기록.
#[derive(Debug, Clone, PartialEq)]
pub struct CriteriaPass {
    /// 통과한 값의 판 번호(승격대기로 갈 때의 판).
    pub revision: u32,
    /// 통과 기록(모의 결과와 기준의 비교, JSON). 받은 그대로다.
    pub record: serde_json::Value,
    /// 기록한 시각.
    pub at: UnixMillis,
}

/// 템플릿 값의 한 판.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateRevision {
    /// 판 번호. 1부터 빠짐없이 커진다.
    pub revision: u32,
    /// 그 판의 값.
    pub value: serde_json::Value,
    /// 그 판을 저장한 시각.
    pub at: UnixMillis,
}

/// 저장된 템플릿.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    /// 번호.
    pub id: TemplateId,
    /// 지금 상태.
    pub state: TemplateState,
    /// 지금 값의 판 번호.
    pub revision: u32,
    /// 지금 값.
    pub value: serde_json::Value,
    /// 기준 통과 기록. 승격대기 · 실전중이면 언제나 있고, 모의중이면 없다.
    /// 멈춤이면 실전중 때의 것이 남는다.
    pub criteria_pass: Option<CriteriaPass>,
    /// 실전중으로 올릴 때 소비한 승인 번호. 실전중이면 언제나 있다(E36). 멈춤이면 남는다.
    pub approval: Option<ApprovalId>,
    /// 만든 시각.
    pub created_at: UnixMillis,
    /// 마지막으로 바뀐 시각(값 · 상태).
    pub updated_at: UnixMillis,
}

impl Store {
    /// 템플릿을 모의중 · 판 1로 만든다(V29). 기준 통과 기록과 승인은 없다.
    pub fn create_template(&self, new: NewTemplate) -> Result<Template, StoreError> {
        self.write(|conn| create(conn, new))
    }

    /// 템플릿 한 건. 없으면 [`StoreError::TemplateNotFound`].
    pub fn get_template(&self, id: TemplateId) -> Result<Template, StoreError> {
        self.read(|conn| get(conn, id))
    }

    /// 그 상태에 있는 템플릿들. 번호 순서다.
    pub fn templates_in_state(&self, state: TemplateState) -> Result<Vec<Template>, StoreError> {
        self.read(|conn| in_state(conn, state))
    }

    /// 템플릿 값을 새 판으로 저장한다(판 번호 + 1). 옛 판은 그대로 남는다.
    ///
    /// 모의중일 때만 된다. 승격대기 · 실전중 · 멈춤이면 [`StoreError::TemplateLocked`]이고 그대로다
    /// (기준을 통과하고 승인받은 값이 몰래 바뀌지 않게).
    pub fn revise_template(
        &self,
        id: TemplateId,
        value: &serde_json::Value,
        at: UnixMillis,
    ) -> Result<Template, StoreError> {
        self.write(|conn| revise(conn, id, value, at))
    }

    /// 그 템플릿 값의 모든 판. 판 번호 순서다.
    pub fn template_revisions(&self, id: TemplateId) -> Result<Vec<TemplateRevision>, StoreError> {
        self.read(|conn| revisions(conn, id))
    }

    /// 모의중 → 승격대기. 기준 통과 기록을 지금 판 번호와 함께 남긴다.
    ///
    /// 기준을 넘었는지는 호출하는 쪽(`pi-template`)이 판단한다(V30: 못 미치면 부르지 않는다).
    /// 모의중이 아니면 [`StoreError::TransitionDenied`]([`crate::Transition::Template`],
    /// [`crate::DenyReason::NotAllowed`])이고 그대로다.
    pub fn submit_template_for_promotion(
        &self,
        id: TemplateId,
        record: &serde_json::Value,
        at: UnixMillis,
    ) -> Result<Template, StoreError> {
        self.write(|conn| submit(conn, id, record, at))
    }

    /// 승격대기 → 모의중(내가 실전 승격을 거절). 기준 통과 기록은 지운다(다시 통과해야 한다).
    /// 승격대기가 아니면 [`StoreError::TransitionDenied`]이고 그대로다.
    pub fn reject_template_promotion(
        &self,
        id: TemplateId,
        at: UnixMillis,
    ) -> Result<Template, StoreError> {
        self.write(|conn| reject(conn, id, at))
    }

    /// 승격대기 → 실전중. 승인을 [`Store::consume_approval`]과 같은 규칙으로 같은 묶음 안에서 소비한다.
    ///
    /// - 승격대기가 아니면 [`StoreError::TransitionDenied`]이고 템플릿도 승인도 그대로다
    ///   (모의중에서 바로 실전중은 없다).
    /// - 승인이 쓸 수 없으면 그 오류([`StoreError::ApprovalNotUsable`] 등)이고 템플릿도 승인도 그대로다(E36).
    /// - 되면 `approval`에 승인 번호가 남는다.
    ///
    /// 표에서도 지킨다(E36): `templates`의 CHECK가 승인 번호나 기준 통과 기록이 없는 줄을 `'live'`로
    /// 두는 것을 어떤 SQL로도 막는다.
    pub fn promote_template_live(
        &self,
        id: TemplateId,
        approval: ApprovalUse,
        at: UnixMillis,
    ) -> Result<Template, StoreError> {
        self.write(|conn| promote(conn, id, &approval, at))
    }

    /// 실전중 → 멈춤(내가 실전 템플릿을 멈춤). 실전중이 아니면 [`StoreError::TransitionDenied`]이고 그대로다.
    ///
    /// 투자 팀이 전체 정지가 되면([`Store::change_team_state`]) 실전중인 템플릿은 모두 같은 묶음 안에서
    /// 멈춤이 되고 `updated_at`은 그 변경의 시각이 된다. 다른 상태의 템플릿은 그대로다.
    pub fn stop_template(&self, id: TemplateId, at: UnixMillis) -> Result<Template, StoreError> {
        self.write(|conn| stop(conn, id, at))
    }
}

// ───────────── 안에서만 쓰는 것 ─────────────

const SELECT: &str = "SELECT t.id, t.state, t.revision, t.pass_revision, t.pass_record, t.pass_at,
            t.approval_id, t.created_at, t.updated_at, r.value
     FROM templates t
     JOIN template_revisions r ON r.template_id = t.id AND r.revision = t.revision";

fn state_code(state: TemplateState) -> &'static str {
    match state {
        TemplateState::Simulating => "simulating",
        TemplateState::PromotionPending => "promotion_pending",
        TemplateState::Live => "live",
        TemplateState::Stopped => "stopped",
    }
}

fn state_from(code: &str) -> Result<TemplateState, StoreError> {
    TemplateState::ALL
        .into_iter()
        .find(|state| state_code(*state) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 템플릿 상태 값: {code}")))
}

fn revision_from(raw: i64) -> Result<u32, StoreError> {
    u32::try_from(raw).map_err(|_| StoreError::Incompatible(format!("판 번호가 이상하다: {raw}")))
}

fn template_from(row: &Row<'_>) -> Result<Template, StoreError> {
    let pass_revision: Option<i64> = row.get(3)?;
    let pass_record: Option<String> = row.get(4)?;
    let pass_at: Option<i64> = row.get(5)?;
    let criteria_pass = match (pass_revision, pass_record, pass_at) {
        (Some(revision), Some(record), Some(at)) => Some(CriteriaPass {
            revision: revision_from(revision)?,
            record: serde_json::from_str(&record)?,
            at: UnixMillis(at),
        }),
        _ => None,
    };
    Ok(Template {
        id: TemplateId(row.get(0)?),
        state: state_from(&row.get::<_, String>(1)?)?,
        revision: revision_from(row.get(2)?)?,
        value: serde_json::from_str(&row.get::<_, String>(9)?)?,
        criteria_pass,
        approval: row.get::<_, Option<i64>>(6)?.map(ApprovalId),
        created_at: UnixMillis(row.get(7)?),
        updated_at: UnixMillis(row.get(8)?),
    })
}

fn get(conn: &Connection, id: TemplateId) -> Result<Template, StoreError> {
    let sql = format!("{SELECT} WHERE t.id = ?1");
    let found = conn
        .query_row(&sql, [id.0], |row| Ok(template_from(row)))
        .optional()?;
    found.ok_or(StoreError::TemplateNotFound(id))?
}

fn in_state(conn: &Connection, state: TemplateState) -> Result<Vec<Template>, StoreError> {
    let sql = format!("{SELECT} WHERE t.state = ?1 ORDER BY t.id");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([state_code(state)])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(template_from(row)?);
    }
    Ok(out)
}

fn insert_revision(
    conn: &Connection,
    id: TemplateId,
    revision: u32,
    value: &serde_json::Value,
    at: UnixMillis,
) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO template_revisions (template_id, revision, value, at) VALUES (?1, ?2, ?3, ?4)",
        params![id.0, revision, serde_json::to_string(value)?, at.0],
    )?;
    Ok(())
}

fn create(conn: &Connection, new: NewTemplate) -> Result<Template, StoreError> {
    conn.execute(
        "INSERT INTO templates (state, revision, created_at, updated_at)
         VALUES ('simulating', 1, ?1, ?1)",
        [new.at.0],
    )?;
    let id = TemplateId(conn.last_insert_rowid());
    insert_revision(conn, id, 1, &new.value, new.at)?;
    get(conn, id)
}

fn revise(
    conn: &Connection,
    id: TemplateId,
    value: &serde_json::Value,
    at: UnixMillis,
) -> Result<Template, StoreError> {
    let now = get(conn, id)?;
    if now.state != TemplateState::Simulating {
        return Err(StoreError::TemplateLocked { state: now.state });
    }
    let next = now.revision + 1;
    insert_revision(conn, id, next, value, at)?;
    conn.execute(
        "UPDATE templates SET revision = ?2, updated_at = ?3 WHERE id = ?1",
        params![id.0, next, at.0],
    )?;
    get(conn, id)
}

fn revisions(conn: &Connection, id: TemplateId) -> Result<Vec<TemplateRevision>, StoreError> {
    get(conn, id)?;
    let mut stmt = conn.prepare(
        "SELECT revision, value, at FROM template_revisions
         WHERE template_id = ?1 ORDER BY revision",
    )?;
    let mut rows = stmt.query([id.0])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(TemplateRevision {
            revision: revision_from(row.get(0)?)?,
            value: serde_json::from_str(&row.get::<_, String>(1)?)?,
            at: UnixMillis(row.get(2)?),
        });
    }
    Ok(out)
}

/// 템플릿을 읽고, 지금 상태가 `from`이 아니면 `to`로 가는 전이를 거부한다.
fn expect(
    conn: &Connection,
    id: TemplateId,
    from: TemplateState,
    to: TemplateState,
) -> Result<Template, StoreError> {
    let now = get(conn, id)?;
    if now.state != from {
        return Err(StoreError::TransitionDenied {
            transition: Transition::Template {
                from: now.state,
                to,
            },
            reason: DenyReason::NotAllowed,
        });
    }
    Ok(now)
}

fn submit(
    conn: &Connection,
    id: TemplateId,
    record: &serde_json::Value,
    at: UnixMillis,
) -> Result<Template, StoreError> {
    let now = expect(
        conn,
        id,
        TemplateState::Simulating,
        TemplateState::PromotionPending,
    )?;
    conn.execute(
        "UPDATE templates SET state = 'promotion_pending', pass_revision = ?2, pass_record = ?3,
             pass_at = ?4, updated_at = ?4
         WHERE id = ?1",
        params![id.0, now.revision, serde_json::to_string(record)?, at.0],
    )?;
    get(conn, id)
}

fn reject(conn: &Connection, id: TemplateId, at: UnixMillis) -> Result<Template, StoreError> {
    expect(
        conn,
        id,
        TemplateState::PromotionPending,
        TemplateState::Simulating,
    )?;
    conn.execute(
        "UPDATE templates SET state = 'simulating', pass_revision = NULL, pass_record = NULL,
             pass_at = NULL, updated_at = ?2
         WHERE id = ?1",
        params![id.0, at.0],
    )?;
    get(conn, id)
}

fn promote(
    conn: &Connection,
    id: TemplateId,
    approval: &ApprovalUse,
    at: UnixMillis,
) -> Result<Template, StoreError> {
    expect(
        conn,
        id,
        TemplateState::PromotionPending,
        TemplateState::Live,
    )?;
    approval::consume(conn, approval, at)?;
    conn.execute(
        "UPDATE templates SET state = 'live', approval_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![id.0, approval.id.0, at.0],
    )?;
    get(conn, id)
}

fn stop(conn: &Connection, id: TemplateId, at: UnixMillis) -> Result<Template, StoreError> {
    expect(conn, id, TemplateState::Live, TemplateState::Stopped)?;
    conn.execute(
        "UPDATE templates SET state = 'stopped', updated_at = ?2 WHERE id = ?1",
        params![id.0, at.0],
    )?;
    get(conn, id)
}

/// 실전중인 템플릿을 모두 멈춤으로 만든다. 투자 팀이 전체 정지가 되는 묶음 안에서 부른다(M3).
pub(crate) fn stop_all_live(conn: &Connection, at: UnixMillis) -> Result<(), StoreError> {
    conn.execute(
        "UPDATE templates SET state = 'stopped', updated_at = ?1 WHERE state = 'live'",
        [at.0],
    )?;
    Ok(())
}
