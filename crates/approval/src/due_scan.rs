//! (5차) [`Approvals::unhandled_due`](crate::Approvals::unhandled_due)가 읽는 닫힌 주문 요청.
//!
//! 닫힌 요청을 매번 처음부터 다 읽으면 몇 달 뒤에는 읽을 것이 끝없이 늘어난다. 그래서 관리자마다 바닥 번호를
//! 기억한다: 바닥 아래의 요청은 모두 기다림이 아니고(결과는 한 번만 바뀐다), 그 가운데 답 없음 · 만료인 주문
//! 요청은 처리 기록이 있다(처리 기록도 한 번 남으면 그대로다). 그러니 바닥 아래는 다시 `unhandled_due`에 나올 일이
//! 없다. 읽을 때마다 아래 차례로 본다:
//!
//! 1. 지금 가장 큰 요청 번호(`top`) — 이 뒤에 생기는 요청은 모두 이보다 크다.
//! 2. 기다리는 요청 가운데 가장 작은 번호 — 뒤에 닫힐 수 있다.
//! 3. 바닥 위의 답 없음 · 만료 요청(번호가 큰 것부터 쪽 단위로, 바닥 아래에 닿으면 멈춘다).
//!
//! 새 바닥은 `top + 1`, 2번, 3번에서 처리 기록이 없는 것의 번호 가운데 가장 작은 것이다. 1 → 2 → 3 차례라
//! 그 사이에 닫힌 요청은 2번(기다림이었다)이나 3번(이미 닫혔다)에 잡힌다.

use pi_store::{
    ApprovalLine, ApprovalStatus, HISTORY_PAGE_MAX, Store, StoreError, UnixMillis,
};
use serde_json::Value;

use crate::approvals::ApprovalNumber;
use crate::record::{self, Memo};
use crate::subject::{OrderSubject, Subject};

/// 처리 기록이 없는 답 없음 · 만료 주문 요청 한 건.
#[derive(Debug, Clone)]
pub(crate) struct Closed {
    pub(crate) id: pi_store::ApprovalId,
    pub(crate) number: ApprovalNumber,
    pub(crate) order: OrderSubject,
    pub(crate) status: ApprovalStatus,
    pub(crate) expires_at: Option<UnixMillis>,
    pub(crate) resolved_at: Option<UnixMillis>,
}

impl Closed {
    /// 이 시각에 만료인가(만료 시각이 없으면 아니다).
    pub(crate) fn expired_at(&self, now: UnixMillis) -> bool {
        self.expires_at.is_some_and(|end| now.0 >= end.0)
    }
}

/// 바닥 위에서 처리 기록이 없는 답 없음 · 만료 주문 요청들을 읽고 바닥을 올린다(순서는 정하지 않는다).
pub(crate) fn scan(store: &Store, memo: &mut Memo) -> Result<Vec<Closed>, StoreError> {
    let floor = memo.due_floor();
    let top = store
        .approval_page(None, 1, None)?
        .first()
        .map_or(0, |line| line.id.0);
    let pending_min = store
        .approvals_in_status(ApprovalStatus::Pending)?
        .iter()
        .map(|row| row.id.0)
        .min();
    let mut closed = Vec::new();
    for status in [ApprovalStatus::NoAnswer, ApprovalStatus::Expired] {
        for line in lines_from(store, status, floor)? {
            if let Some(found) = decode(&line)
                && memo.handling(&found.number).is_none()
            {
                closed.push(found);
            }
        }
    }
    let unhandled_min = closed.iter().map(|found| found.id.0).min();
    let next = [Some(top.saturating_add(1)), pending_min, unhandled_min]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(floor);
    memo.raise_due_floor(next);
    Ok(closed)
}

/// 그 결과인 요청 가운데 번호가 `floor` 이상인 것(번호가 큰 것부터).
fn lines_from(
    store: &Store,
    status: ApprovalStatus,
    floor: i64,
) -> Result<Vec<ApprovalLine>, StoreError> {
    let mut found = Vec::new();
    let mut before = None;
    loop {
        let page = store.approval_page(before, HISTORY_PAGE_MAX, Some(status))?;
        let full = page.len() >= HISTORY_PAGE_MAX;
        let Some(last) = page.last().map(|line| line.id) else {
            return Ok(found);
        };
        found.extend(page.into_iter().filter(|line| line.id.0 >= floor));
        if last.0 < floor || !full {
            return Ok(found);
        }
        before = Some(last);
    }
}

/// 이 크레이트가 만든 주문 요청 줄이면 푼다.
fn decode(line: &ApprovalLine) -> Option<Closed> {
    let s = &line.summary;
    let Subject::Order(order) = record::subject_from(s.get("subject")?)? else {
        return None;
    };
    Some(Closed {
        id: line.id,
        number: ApprovalNumber::new(s.get("number")?.as_str()?),
        order,
        status: line.status,
        expires_at: s.get("expires_at").and_then(Value::as_i64).map(UnixMillis),
        resolved_at: line.resolved_at,
    })
}
