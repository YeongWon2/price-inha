//! 규칙 개선안 표 (9차 3번, M4).
//!
//! 무엇을 고치자는지와 그 변경이 위험을 키우는지는 복기(`pi-journal`)가 정한다. 저장소는 상태표에 없는
//! 전이를 막고, 반영은 승인을 소비한 뒤에만 되게 한다.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::approval::{self, ApprovalId, ApprovalUse};
use crate::error::{DenyReason, Transition};
use crate::model::{TradeId, UnixMillis};
use crate::template::{TemplateId, TemplateState};
use crate::{Store, StoreError};

/// 개선안을 내는 데 필요한 근거 매매의 가장 적은 수(M4 "2건 이상이어야 제안된다").
pub const PROPOSAL_MIN_EVIDENCE: usize = 2;

/// 규칙 개선안 번호. 저장소가 1부터 붙인다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProposalId(pub i64);

/// 규칙 개선안 상태(`spec.md` 상태표 · M4, F5).
///
/// 허용 전이(이 밖은 모두 거부):
///
/// | 에서 | 으로 | 함수 |
/// |---|---|---|
/// | 제안됨 | 반영됨 | [`Store::approve_proposal`] (위험을 키우지 않는 개선안) |
/// | 제안됨 | 냉각중 | [`Store::approve_proposal`] (위험을 키우는 개선안) |
/// | 제안됨 | 거절됨 | [`Store::reject_proposal`] |
/// | 냉각중 | 반영됨 | [`Store::apply_cooled_proposal`] |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProposalState {
    /// 제안됨
    Proposed,
    /// 냉각중
    Cooling,
    /// 반영됨
    Applied,
    /// 거절됨
    Rejected,
}

impl ProposalState {
    /// 모든 상태.
    pub const ALL: [ProposalState; 4] = [
        ProposalState::Proposed,
        ProposalState::Cooling,
        ProposalState::Applied,
        ProposalState::Rejected,
    ];
}

/// 새 규칙 개선안.
#[derive(Debug, Clone, PartialEq)]
pub struct NewProposal {
    /// 고치자는 내용(JSON). 그대로 저장한다.
    pub content: serde_json::Value,
    /// 같은 실수가 나온 매매들. 서로 다른 매매가 [`PROPOSAL_MIN_EVIDENCE`]건 이상이어야 한다.
    pub evidence: Vec<TradeId>,
    /// 한도나 손절을 넓히는 변경인가. 참이면 승인 뒤 냉각중을 거친다.
    pub increases_risk: bool,
    /// 낸 시각.
    pub at: UnixMillis,
}

/// 새 사용자 변경 개선안(15차 1번, 스펙 R14-1 · E28 · V15). 근거 매매가 없다.
#[derive(Debug, Clone, PartialEq)]
pub struct NewUserProposal {
    /// 바꾸려는 템플릿.
    pub template: TemplateId,
    /// 바꾸려는 새 값(JSON). 승인되고 냉각을 지나면 새 리비전의 값이 된다.
    pub content: serde_json::Value,
    /// 낸 시각.
    pub at: UnixMillis,
}

/// 개선안이 어디서 왔는가(15차). 옛 행은 모두 [`ProposalSource::Review`]다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProposalSource {
    /// 복기가 낸 개선안.
    Review,
    /// 사용자가 템플릿을 바꾸자고 한 개선안.
    User {
        /// 바꾸려는 템플릿.
        template: TemplateId,
    },
}

/// 저장된 규칙 개선안.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// 번호.
    pub id: ProposalId,
    /// 지금 상태.
    pub state: ProposalState,
    /// 고치자는 내용.
    pub content: serde_json::Value,
    /// 근거 매매. 겹친 번호는 한 번만, 처음 나온 순서대로다.
    pub evidence: Vec<TradeId>,
    /// 위험을 키우는 변경인가.
    pub increases_risk: bool,
    /// 승인할 때 소비한 승인 번호. 냉각중 · 반영됨이면 언제나 있다.
    pub approval: Option<ApprovalId>,
    /// 낸 시각.
    pub created_at: UnixMillis,
    /// 마지막으로 바뀐 시각.
    pub updated_at: UnixMillis,
    /// 반영된 시각. 반영됨일 때만 있다.
    pub applied_at: Option<UnixMillis>,
}

impl Store {
    /// 규칙 개선안을 제안됨으로 만든다.
    ///
    /// - 서로 다른 근거 매매가 [`PROPOSAL_MIN_EVIDENCE`]건보다 적으면 [`StoreError::ProposalEvidenceTooFew`].
    /// - 없는 매매 번호가 있으면 [`StoreError::TradeNotFound`].
    /// - 거부되면 아무것도 만들지 않는다(번호도 쓰지 않는다).
    pub fn create_proposal(&self, new: NewProposal) -> Result<Proposal, StoreError> {
        self.write(|conn| create(conn, new))
    }

    /// 규칙 개선안 한 건. 없으면 [`StoreError::ProposalNotFound`].
    pub fn get_proposal(&self, id: ProposalId) -> Result<Proposal, StoreError> {
        self.read(|conn| get(conn, id))
    }

    /// 그 상태에 있는 규칙 개선안들. 번호 순서다.
    pub fn proposals_in_state(&self, state: ProposalState) -> Result<Vec<Proposal>, StoreError> {
        self.read(|conn| in_state(conn, state))
    }

    /// 내가 승인한 개선안을 넘긴다: 위험을 키우지 않으면 반영됨(`applied_at = at`), 키우면 냉각중.
    /// 승인을 [`Store::consume_approval`]과 같은 규칙으로 같은 묶음 안에서 소비한다.
    ///
    /// - 제안됨이 아니면 [`StoreError::TransitionDenied`]([`crate::Transition::Proposal`],
    ///   [`crate::DenyReason::NotAllowed`])이고 개선안도 승인도 그대로다. 오류의 `to`는 그 개선안이
    ///   갔을 상태다(위험을 키우면 냉각중, 아니면 반영됨).
    /// - 상태를 먼저 보고 그다음 승인을 본다.
    /// - 승인을 쓸 수 없으면 그 오류이고 개선안도 승인도 그대로다(반영됨은 내 승인 뒤에만).
    pub fn approve_proposal(
        &self,
        id: ProposalId,
        approval: ApprovalUse,
        at: UnixMillis,
    ) -> Result<Proposal, StoreError> {
        self.write(|conn| approve(conn, id, &approval, at))
    }

    /// 제안됨 → 거절됨. 제안됨이 아니면 [`StoreError::TransitionDenied`]이고 그대로다.
    pub fn reject_proposal(&self, id: ProposalId, at: UnixMillis) -> Result<Proposal, StoreError> {
        self.write(|conn| reject(conn, id, at))
    }

    /// 제안됨인 개선안을 위험을 키우는 개선안으로 표시한다(후보 일괄 처리). 그 뒤 승인하면 냉각중을 거친다.
    ///
    /// - 이미 위험 표시가 있으면 아무것도 바꾸지 않고 그대로 돌려준다.
    /// - 제안됨이 아니면 [`StoreError::ProposalLocked`]이고 그대로다. 표시를 내리는 길은 없다
    ///   (내리면 냉각을 건너뛴다). 표의 트리거도 거짓 → 참(제안됨일 때)만 허락한다.
    pub fn mark_proposal_risky(
        &self,
        id: ProposalId,
        at: UnixMillis,
    ) -> Result<Proposal, StoreError> {
        self.write(|conn| {
            let now = get(conn, id)?;
            if now.state != ProposalState::Proposed {
                return Err(StoreError::ProposalLocked {
                    id,
                    state: now.state,
                });
            }
            if !now.increases_risk {
                conn.execute(
                    "UPDATE proposals SET increases_risk = 1, updated_at = ?2 WHERE id = ?1",
                    params![id.0, at.0],
                )?;
            }
            get(conn, id)
        })
    }

    /// 사용자 변경 개선안을 제안됨으로 만든다(15차 1번). 근거 매매 없이 되고(최소 근거 수 검사 없음),
    /// `increases_risk`는 늘 참이다. 출처는 [`ProposalSource::User`]로 남는다.
    ///
    /// 템플릿이 없으면 [`StoreError::TemplateNotFound`]이고 아무것도 만들지 않는다.
    pub fn create_user_proposal(&self, new: NewUserProposal) -> Result<Proposal, StoreError> {
        self.write(|conn| {
            let exists = conn
                .query_row(
                    "SELECT 1 FROM templates WHERE id = ?1",
                    [new.template.0],
                    |_| Ok(()),
                )
                .optional()?;
            if exists.is_none() {
                return Err(StoreError::TemplateNotFound(new.template));
            }
            conn.execute(
                "INSERT INTO proposals (state, content, increases_risk, created_at, updated_at)
                 VALUES ('proposed', ?1, 1, ?2, ?2)",
                params![serde_json::to_string(&new.content)?, new.at.0],
            )?;
            let id = ProposalId(conn.last_insert_rowid());
            conn.execute(
                "INSERT INTO proposal_sources (proposal_id, template_id) VALUES (?1, ?2)",
                params![id.0, new.template.0],
            )?;
            get(conn, id)
        })
    }

    /// 개선안의 출처(15차 1번). 없으면 [`StoreError::ProposalNotFound`].
    pub fn proposal_source(&self, id: ProposalId) -> Result<ProposalSource, StoreError> {
        self.read(|conn| {
            get(conn, id)?;
            source(conn, id)
        })
    }

    /// 냉각중인 사용자 변경 개선안을 반영됨으로 옮기고 같은 쓰기 안에서 그 템플릿에 새 리비전(`value`)을
    /// 쓴다(15차 2번, 잠금의 예외). 템플릿이 실전중 · 모의중이어도 된다. 템플릿이 멈춤 · 승격대기(16차: 통과 기록과
    /// 다른 리비전이 실전이 되지 못하게)이거나 냉각중이 아니거나 복기 개선안이면 거부하고 개선안도 템플릿도 그대로다.
    ///
    /// - 냉각중이 아니면 [`StoreError::TransitionDenied`].
    /// - 복기 개선안이면 [`StoreError::ProposalLocked`].
    /// - 템플릿이 멈춤이나 승격대기면 [`StoreError::TemplateLocked`].
    pub fn apply_cooled_user_proposal(
        &self,
        id: ProposalId,
        value: &serde_json::Value,
        at: UnixMillis,
    ) -> Result<Proposal, StoreError> {
        self.write(|conn| {
            let now = expect(conn, id, ProposalState::Cooling, |_| ProposalState::Applied)?;
            let ProposalSource::User { template } = source(conn, id)? else {
                return Err(StoreError::ProposalLocked {
                    id,
                    state: now.state,
                });
            };
            let target = crate::template::get(conn, template)?;
            if matches!(
                target.state,
                TemplateState::Stopped | TemplateState::PromotionPending
            ) {
                return Err(StoreError::TemplateLocked {
                    state: target.state,
                });
            }
            conn.execute(
                "UPDATE proposals SET state = 'applied', updated_at = ?2, applied_at = ?2
                 WHERE id = ?1",
                params![id.0, at.0],
            )?;
            let next = target.revision + 1;
            conn.execute(
                "INSERT INTO user_revisions (proposal_id, template_id, revision) VALUES (?1, ?2, ?3)",
                params![id.0, template.0, next],
            )?;
            conn.execute(
                "INSERT INTO template_revisions (template_id, revision, value, at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![template.0, next, serde_json::to_string(value)?, at.0],
            )?;
            conn.execute(
                "UPDATE templates SET revision = ?2, updated_at = ?3 WHERE id = ?1",
                params![template.0, next, at.0],
            )?;
            get(conn, id)
        })
    }

    /// 냉각중 → 반영됨(`applied_at = at`). 냉각중이 아니면 [`StoreError::TransitionDenied`]이고 그대로다.
    ///
    /// "다음 거래일이 됐는가"는 저장소가 알 수 없다(달력을 모른다). 호출하는 쪽 책임이다.
    pub fn apply_cooled_proposal(
        &self,
        id: ProposalId,
        at: UnixMillis,
    ) -> Result<Proposal, StoreError> {
        self.write(|conn| apply_cooled(conn, id, at))
    }
}

// ───────────── 안에서만 쓰는 것 ─────────────

const COLUMNS: &str =
    "id, state, content, increases_risk, approval_id, created_at, updated_at, applied_at";

fn state_code(state: ProposalState) -> &'static str {
    match state {
        ProposalState::Proposed => "proposed",
        ProposalState::Cooling => "cooling",
        ProposalState::Applied => "applied",
        ProposalState::Rejected => "rejected",
    }
}

fn state_from(code: &str) -> Result<ProposalState, StoreError> {
    ProposalState::ALL
        .into_iter()
        .find(|state| state_code(*state) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 개선안 상태 값: {code}")))
}

fn proposal_from(conn: &Connection, row: &Row<'_>) -> Result<Proposal, StoreError> {
    let id = ProposalId(row.get(0)?);
    Ok(Proposal {
        id,
        state: state_from(&row.get::<_, String>(1)?)?,
        content: serde_json::from_str(&row.get::<_, String>(2)?)?,
        evidence: evidence(conn, id)?,
        increases_risk: row.get(3)?,
        approval: row.get::<_, Option<i64>>(4)?.map(ApprovalId),
        created_at: UnixMillis(row.get(5)?),
        updated_at: UnixMillis(row.get(6)?),
        applied_at: row.get::<_, Option<i64>>(7)?.map(UnixMillis),
    })
}

fn evidence(conn: &Connection, id: ProposalId) -> Result<Vec<TradeId>, StoreError> {
    let mut stmt =
        conn.prepare("SELECT trade_id FROM proposal_evidence WHERE proposal_id = ?1 ORDER BY pos")?;
    let ids = stmt
        .query_map([id.0], |row| row.get(0).map(TradeId))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

/// 겹친 번호를 빼고 처음 나온 순서대로 둔다.
fn distinct(evidence: &[TradeId]) -> Vec<TradeId> {
    let mut out: Vec<TradeId> = Vec::with_capacity(evidence.len());
    for id in evidence {
        if !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

fn create(conn: &Connection, new: NewProposal) -> Result<Proposal, StoreError> {
    let evidence = distinct(&new.evidence);
    if evidence.len() < PROPOSAL_MIN_EVIDENCE {
        return Err(StoreError::ProposalEvidenceTooFew {
            count: evidence.len(),
        });
    }
    for trade in &evidence {
        let exists = conn
            .query_row("SELECT 1 FROM trades WHERE id = ?1", [trade.0], |_| Ok(()))
            .optional()?;
        if exists.is_none() {
            return Err(StoreError::TradeNotFound(*trade));
        }
    }
    conn.execute(
        "INSERT INTO proposals (state, content, increases_risk, created_at, updated_at)
         VALUES ('proposed', ?1, ?2, ?3, ?3)",
        params![
            serde_json::to_string(&new.content)?,
            new.increases_risk,
            new.at.0
        ],
    )?;
    let id = ProposalId(conn.last_insert_rowid());
    for (pos, trade) in (0_i64..).zip(evidence.iter()) {
        conn.execute(
            "INSERT INTO proposal_evidence (proposal_id, pos, trade_id) VALUES (?1, ?2, ?3)",
            params![id.0, pos, trade.0],
        )?;
    }
    get(conn, id)
}

fn get(conn: &Connection, id: ProposalId) -> Result<Proposal, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM proposals WHERE id = ?1");
    let found = conn
        .query_row(&sql, [id.0], |row| Ok(proposal_from(conn, row)))
        .optional()?;
    found.ok_or(StoreError::ProposalNotFound(id))?
}

fn in_state(conn: &Connection, state: ProposalState) -> Result<Vec<Proposal>, StoreError> {
    let sql = format!("SELECT {COLUMNS} FROM proposals WHERE state = ?1 ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([state_code(state)])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(proposal_from(conn, row)?);
    }
    Ok(out)
}

/// 개선안을 읽고, 지금 상태가 `from`이 아니면 `to`로 가는 전이를 거부한다.
fn expect(
    conn: &Connection,
    id: ProposalId,
    from: ProposalState,
    to: impl FnOnce(&Proposal) -> ProposalState,
) -> Result<Proposal, StoreError> {
    let now = get(conn, id)?;
    if now.state != from {
        return Err(StoreError::TransitionDenied {
            transition: Transition::Proposal {
                from: now.state,
                to: to(&now),
            },
            reason: DenyReason::NotAllowed,
        });
    }
    Ok(now)
}

/// 승인하면 가는 상태: 위험을 키우면 냉각중, 아니면 반영됨.
fn approved_state(proposal: &Proposal) -> ProposalState {
    if proposal.increases_risk {
        ProposalState::Cooling
    } else {
        ProposalState::Applied
    }
}

fn approve(
    conn: &Connection,
    id: ProposalId,
    approval: &ApprovalUse,
    at: UnixMillis,
) -> Result<Proposal, StoreError> {
    let now = expect(conn, id, ProposalState::Proposed, approved_state)?;
    approval::consume(conn, approval, at)?;
    let to = approved_state(&now);
    let applied_at = (to == ProposalState::Applied).then_some(at.0);
    conn.execute(
        "UPDATE proposals SET state = ?2, approval_id = ?3, updated_at = ?4, applied_at = ?5
         WHERE id = ?1",
        params![id.0, state_code(to), approval.id.0, at.0, applied_at],
    )?;
    get(conn, id)
}

fn reject(conn: &Connection, id: ProposalId, at: UnixMillis) -> Result<Proposal, StoreError> {
    expect(conn, id, ProposalState::Proposed, |_| {
        ProposalState::Rejected
    })?;
    conn.execute(
        "UPDATE proposals SET state = 'rejected', updated_at = ?2 WHERE id = ?1",
        params![id.0, at.0],
    )?;
    get(conn, id)
}

fn apply_cooled(conn: &Connection, id: ProposalId, at: UnixMillis) -> Result<Proposal, StoreError> {
    expect(conn, id, ProposalState::Cooling, |_| ProposalState::Applied)?;
    conn.execute(
        "UPDATE proposals SET state = 'applied', updated_at = ?2, applied_at = ?2 WHERE id = ?1",
        params![id.0, at.0],
    )?;
    get(conn, id)
}

/// 개선안의 출처. 대상 템플릿 줄이 없으면 복기가 낸 것이다.
fn source(conn: &Connection, id: ProposalId) -> Result<ProposalSource, StoreError> {
    let template: Option<i64> = conn
        .query_row(
            "SELECT template_id FROM proposal_sources WHERE proposal_id = ?1",
            [id.0],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match template {
        Some(template) => ProposalSource::User {
            template: TemplateId(template),
        },
        None => ProposalSource::Review,
    })
}
