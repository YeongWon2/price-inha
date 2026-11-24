//! 공지 (19차, 결정 17 · additions.md M10 · T23).
//!
//! 공지(번호 · 글 · 보낸 시각)와 담당별 확인(판 11). 공지는 지우지 못하고 글을 고치지 못한다.
//! 담당별 확인은 기다림에서 한 번만 바뀐다. 글의 비밀키 가리기는 부르는 쪽이 한 뒤 넘긴다.
//! 판단 시각보다 늦게 보낸 공지를 논의 자료로 주지 않는 조건은 부르는 쪽이
//! [`Store::notices_between`]의 범위로 건다.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};

use crate::history::page_limit;
use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 공지 글의 가장 긴 길이(글자 수).
pub const NOTICE_TEXT_MAX_CHARS: usize = 500;

/// 공지 번호. 저장소가 붙이고 다시 쓰지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoticeId(pub i64);

/// 담당별 공지 확인 상태. 기다림에서 다른 하나로 한 번만 바뀐다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoticeAckStatus {
    /// 기다림
    Waiting,
    /// 답함
    Answered,
    /// 상한으로 못 함
    Capped,
    /// 실패
    Failed,
}

impl NoticeAckStatus {
    /// 모든 상태.
    pub const ALL: [NoticeAckStatus; 4] = [
        NoticeAckStatus::Waiting,
        NoticeAckStatus::Answered,
        NoticeAckStatus::Capped,
        NoticeAckStatus::Failed,
    ];
}

/// 담당 한 명의 공지 확인.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeAck {
    /// 담당 이름.
    pub role: String,
    /// 상태.
    pub status: NoticeAckStatus,
    /// 답 글. 기다림이면 `None`.
    pub answer: Option<String>,
    /// 기다림에서 바뀐 시각. 기다림이면 `None`.
    pub answered_at: Option<UnixMillis>,
}

/// 새 공지.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewNotice {
    /// 글(1~[`NOTICE_TEXT_MAX_CHARS`]자).
    pub text: String,
    /// 확인을 기다릴 담당들. 준 순서대로 기다림으로 남는다.
    pub roles: Vec<String>,
    /// 보낸 시각.
    pub at: UnixMillis,
}

/// 저장된 공지.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// 번호.
    pub id: NoticeId,
    /// 글.
    pub text: String,
    /// 보낸 시각.
    pub sent_at: UnixMillis,
    /// 담당별 확인. 공지를 만들 때 준 순서다.
    pub acks: Vec<NoticeAck>,
}

impl Store {
    /// 공지를 만든다. 담당별 확인은 모두 기다림이다.
    ///
    /// 글이 0자이거나 [`NOTICE_TEXT_MAX_CHARS`]자보다 길면 [`StoreError::InvalidNotice`]이고
    /// 아무것도 남기지 않는다. 담당 이름이 빈 글자이거나 같은 담당이 둘이어도 그렇다.
    pub fn create_notice(&self, new: NewNotice) -> Result<Notice, StoreError> {
        check_notice(&new)?;
        self.write(|conn| {
            conn.execute(
                "INSERT INTO notices (text, sent_at) VALUES (?1, ?2)",
                params![new.text, new.at.0],
            )?;
            let id = NoticeId(conn.last_insert_rowid());
            for role in &new.roles {
                conn.execute(
                    "INSERT INTO notice_acks (notice_id, role, status) VALUES (?1, ?2, 'waiting')",
                    params![id.0, role],
                )?;
            }
            load(conn, id)
        })
    }

    /// 공지 하나. 없는 번호면 [`StoreError::NoticeNotFound`].
    pub fn get_notice(&self, id: NoticeId) -> Result<Notice, StoreError> {
        self.read(|conn| load(conn, id))
    }

    /// 담당의 공지 확인을 기다림에서 `status`로 바꾸고 답 글을 남긴다(한 번만).
    ///
    /// - 없는 공지: [`StoreError::NoticeNotFound`]
    /// - 그 공지에 그 담당의 확인이 없음: [`StoreError::NoticeAckNotFound`]
    /// - 이미 바뀜: [`StoreError::NoticeAckLocked`](`status`가 무엇이든)
    /// - `status`가 기다림: [`StoreError::InvalidNotice`]
    ///
    /// 거부되면 아무것도 바꾸지 않는다.
    pub fn answer_notice(
        &self,
        id: NoticeId,
        role: &str,
        status: NoticeAckStatus,
        answer: &str,
        at: UnixMillis,
    ) -> Result<Notice, StoreError> {
        self.write(|conn| {
            load(conn, id)?;
            let now: Option<String> = conn
                .query_row(
                    "SELECT status FROM notice_acks WHERE notice_id = ?1 AND role = ?2",
                    params![id.0, role],
                    |r| r.get(0),
                )
                .optional()?;
            let ack_missing = || StoreError::NoticeAckNotFound {
                notice: id,
                role: role.to_owned(),
            };
            if ack_status_from(&now.ok_or_else(ack_missing)?)? != NoticeAckStatus::Waiting {
                return Err(StoreError::NoticeAckLocked {
                    notice: id,
                    role: role.to_owned(),
                });
            }
            if status == NoticeAckStatus::Waiting {
                return Err(StoreError::InvalidNotice(
                    "기다림으로는 답하지 못한다".to_owned(),
                ));
            }
            conn.execute(
                "UPDATE notice_acks SET status = ?3, answer = ?4, answered_at = ?5 \
                 WHERE notice_id = ?1 AND role = ?2",
                params![id.0, role, ack_status_code(status), answer, at.0],
            )?;
            load(conn, id)
        })
    }

    /// 공지 목록. 번호가 큰 것(최근)부터 쪽 단위로 돌려준다.
    ///
    /// `before`가 있으면 그 번호보다 작은 공지만. `max`는 [`crate::HISTORY_PAGE_MAX`]로 줄인다.
    /// 0이면 빈 목록이다.
    pub fn notices_page(
        &self,
        before: Option<NoticeId>,
        max: usize,
    ) -> Result<Vec<Notice>, StoreError> {
        self.read(|conn| {
            let ids = notice_ids(
                conn,
                "SELECT id FROM notices WHERE ?1 IS NULL OR id < ?1 ORDER BY id DESC LIMIT ?2",
                params![before.map(|b| b.0), page_limit(max)],
            )?;
            ids.into_iter().map(|id| load(conn, id)).collect()
        })
    }

    /// 보낸 시각이 `from` 이상 `to` 이하인 공지. 보낸 시각 순서이고, 같은 시각이면 번호 순서다.
    pub fn notices_between(
        &self,
        from: UnixMillis,
        to: UnixMillis,
    ) -> Result<Vec<Notice>, StoreError> {
        self.read(|conn| {
            let ids = notice_ids(
                conn,
                "SELECT id FROM notices WHERE sent_at BETWEEN ?1 AND ?2 ORDER BY sent_at, id",
                params![from.0, to.0],
            )?;
            ids.into_iter().map(|id| load(conn, id)).collect()
        })
    }
}

// ───────────── 안에서만 쓰는 것 ─────────────

fn check_notice(new: &NewNotice) -> Result<(), StoreError> {
    let chars = new.text.chars().count();
    if chars == 0 || chars > NOTICE_TEXT_MAX_CHARS {
        return Err(StoreError::InvalidNotice(format!(
            "글은 1~{NOTICE_TEXT_MAX_CHARS}자여야 한다({chars}자)"
        )));
    }
    let mut seen = HashSet::new();
    for role in &new.roles {
        if role.is_empty() || !seen.insert(role.as_str()) {
            return Err(StoreError::InvalidNotice(format!(
                "담당 이름이 비었거나 겹친다: {role:?}"
            )));
        }
    }
    Ok(())
}

fn notice_ids(
    conn: &Connection,
    sql: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<NoticeId>, StoreError> {
    let mut stmt = conn.prepare(sql)?;
    let ids = stmt
        .query_map(args, |r| r.get(0).map(NoticeId))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// 공지 하나와 그 확인들(만든 순서). 없으면 [`StoreError::NoticeNotFound`].
fn load(conn: &Connection, id: NoticeId) -> Result<Notice, StoreError> {
    let (text, sent_at): (String, i64) = conn
        .query_row(
            "SELECT text, sent_at FROM notices WHERE id = ?1",
            [id.0],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or(StoreError::NoticeNotFound(id))?;
    let mut stmt = conn.prepare(
        "SELECT role, status, answer, answered_at FROM notice_acks WHERE notice_id = ?1 ORDER BY id",
    )?;
    let mut rows = stmt.query([id.0])?;
    let mut acks = Vec::new();
    while let Some(row) = rows.next()? {
        let status: String = row.get(1)?;
        let answered_at: Option<i64> = row.get(3)?;
        acks.push(NoticeAck {
            role: row.get(0)?,
            status: ack_status_from(&status)?,
            answer: row.get(2)?,
            answered_at: answered_at.map(UnixMillis),
        });
    }
    Ok(Notice {
        id,
        text,
        sent_at: UnixMillis(sent_at),
        acks,
    })
}

fn ack_status_code(status: NoticeAckStatus) -> &'static str {
    match status {
        NoticeAckStatus::Waiting => "waiting",
        NoticeAckStatus::Answered => "answered",
        NoticeAckStatus::Capped => "capped",
        NoticeAckStatus::Failed => "failed",
    }
}

fn ack_status_from(code: &str) -> Result<NoticeAckStatus, StoreError> {
    NoticeAckStatus::ALL
        .into_iter()
        .find(|s| ack_status_code(*s) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 공지 확인 상태 값: {code}")))
}
