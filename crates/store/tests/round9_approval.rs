#![cfg(test)]
//! 9차 2번: 승인 표 (M5). 원본: OpenClaw `operator_approvals`
//! (`docs/research/triggers-approval-memory-deep.md` 3.2 · 3.4) — 상태 CHECK, 기다림에서 한 번만 바뀜,
//! 한 번만 소비(`consumed_at`), 답하려는 순간 만료 시각이 지났으면 만료로 닫음.
//!
//! 표에서 지키는 것(CHECK · 트리거)은 저장소 파일을 `rusqlite`로 직접 열어 본다.

mod common;

use std::path::Path;

use common::{t, temp_store};
use pi_store::{
    ApprovalId, ApprovalRequest, ApprovalStatus, ApprovalUse, DenyReason, NewApprovalRequest,
    Store, StoreError, Transition,
};
use serde_json::json;

const FP: &str = "sha256:order-005930-buy-10-limit-70000";

fn request(created: i64, expires: i64) -> NewApprovalRequest {
    NewApprovalRequest {
        fingerprint: FP.to_string(),
        summary: json!({ "종목": "005930", "방향": "사기", "수량": 10 }),
        created_at: t(created),
        expires_at: t(expires),
    }
}

fn use_of(id: ApprovalId, fingerprint: &str) -> ApprovalUse {
    ApprovalUse {
        id,
        fingerprint: fingerprint.to_string(),
    }
}

/// 승인까지 마친 요청 하나를 만든다(만료는 t(1000)).
fn approved(store: &Store) -> ApprovalRequest {
    let made = store.create_approval(request(0, 1000)).unwrap();
    store
        .resolve_approval(made.id, ApprovalStatus::Approved, t(10))
        .unwrap()
}

fn sql(path: &Path, statement: &str) -> rusqlite::Result<usize> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(statement, [])
}

// 검증: M5
#[test]
fn 새_승인_요청은_기다림이고_번호는_1부터_빠짐없이_커진다() {
    let (_dir, _path, store) = temp_store();
    let first = store.create_approval(request(0, 1000)).unwrap();
    let second = store.create_approval(request(1, 1000)).unwrap();
    assert_eq!(first.id, ApprovalId(1));
    assert_eq!(second.id, ApprovalId(2));
    assert_eq!(first.status, ApprovalStatus::Pending);
    assert_eq!(first.fingerprint, FP);
    assert_eq!(
        first.summary,
        json!({ "종목": "005930", "방향": "사기", "수량": 10 })
    );
    assert_eq!(first.created_at, t(0));
    assert_eq!(first.expires_at, t(1000));
    assert_eq!(first.resolved_at, None);
    assert_eq!(first.consumed_at, None);
    assert_eq!(store.get_approval(first.id).unwrap(), first);
}

// 검증: M5
#[test]
fn 없는_승인_번호는_찾을_수_없다() {
    let (_dir, _path, store) = temp_store();
    assert!(matches!(
        store.get_approval(ApprovalId(7)),
        Err(StoreError::ApprovalNotFound(ApprovalId(7)))
    ));
    assert!(matches!(
        store.resolve_approval(ApprovalId(7), ApprovalStatus::Approved, t(1)),
        Err(StoreError::ApprovalNotFound(ApprovalId(7)))
    ));
    assert!(matches!(
        store.consume_approval(use_of(ApprovalId(7), FP), t(1)),
        Err(StoreError::ApprovalNotFound(ApprovalId(7)))
    ));
}

// 검증: M5
#[test]
fn 기다림에서는_네_결과_어느_것으로든_한_번_바뀐다() {
    for to in [
        ApprovalStatus::Approved,
        ApprovalStatus::Rejected,
        ApprovalStatus::NoAnswer,
        ApprovalStatus::Expired,
    ] {
        let (_dir, _path, store) = temp_store();
        let made = store.create_approval(request(0, 1000)).unwrap();
        let done = store.resolve_approval(made.id, to, t(5)).unwrap();
        assert_eq!(done.status, to);
        assert_eq!(done.resolved_at, Some(t(5)));
        assert_eq!(store.get_approval(made.id).unwrap(), done);
    }
}

// 검증: M5
#[test]
fn 답이_난_요청은_다시_바뀌지_않는다() {
    for first in [
        ApprovalStatus::Approved,
        ApprovalStatus::Rejected,
        ApprovalStatus::NoAnswer,
        ApprovalStatus::Expired,
    ] {
        let (_dir, _path, store) = temp_store();
        let made = store.create_approval(request(0, 1000)).unwrap();
        let done = store.resolve_approval(made.id, first, t(5)).unwrap();
        for second in ApprovalStatus::ALL {
            let err = store.resolve_approval(made.id, second, t(6)).unwrap_err();
            match err {
                StoreError::TransitionDenied { transition, reason } => {
                    assert_eq!(
                        transition,
                        Transition::Approval {
                            from: first,
                            to: second
                        }
                    );
                    assert_eq!(reason, DenyReason::NotAllowed);
                }
                other => panic!("{first:?} → {second:?}: {other:?}"),
            }
            assert_eq!(store.get_approval(made.id).unwrap(), done);
        }
    }
}

// 검증: M5
#[test]
fn 기다림으로_바꾸라는_답은_거부된다() {
    let (_dir, _path, store) = temp_store();
    let made = store.create_approval(request(0, 1000)).unwrap();
    let err = store
        .resolve_approval(made.id, ApprovalStatus::Pending, t(5))
        .unwrap_err();
    assert!(matches!(
        err,
        StoreError::TransitionDenied {
            transition: Transition::Approval {
                from: ApprovalStatus::Pending,
                to: ApprovalStatus::Pending
            },
            reason: DenyReason::NotAllowed
        }
    ));
    assert_eq!(store.get_approval(made.id).unwrap(), made);
}

// 검증: M5
#[test]
fn 만료_시각에_닿은_뒤의_답은_만료로_닫힌다() {
    // 경계: 만료 시각 바로 전은 그 답 그대로, 만료 시각과 같거나 뒤면 만료.
    let (_dir, _path, store) = temp_store();
    let before = store.create_approval(request(0, 1000)).unwrap();
    let done = store
        .resolve_approval(before.id, ApprovalStatus::Approved, t(999))
        .unwrap();
    assert_eq!(done.status, ApprovalStatus::Approved);

    for to in [
        ApprovalStatus::Approved,
        ApprovalStatus::Rejected,
        ApprovalStatus::NoAnswer,
    ] {
        for at in [1000, 1001] {
            let made = store.create_approval(request(0, 1000)).unwrap();
            let done = store.resolve_approval(made.id, to, t(at)).unwrap();
            assert_eq!(done.status, ApprovalStatus::Expired, "{to:?} at {at}");
            assert_eq!(done.resolved_at, Some(t(at)));
            assert_eq!(store.get_approval(made.id).unwrap(), done);
        }
    }
}

// 검증: M5
#[test]
fn 승인은_한_번만_소비된다() {
    let (_dir, _path, store) = temp_store();
    let ok = approved(&store);
    let used = store.consume_approval(use_of(ok.id, FP), t(20)).unwrap();
    assert_eq!(used.consumed_at, Some(t(20)));
    assert_eq!(used.status, ApprovalStatus::Approved);
    assert_eq!(store.get_approval(ok.id).unwrap(), used);

    let err = store
        .consume_approval(use_of(ok.id, FP), t(21))
        .unwrap_err();
    assert!(matches!(err, StoreError::ApprovalAlreadyConsumed { id } if id == ok.id));
    assert_eq!(store.get_approval(ok.id).unwrap(), used);
}

// 검증: M5
#[test]
fn 승인이_아닌_요청은_소비할_수_없다() {
    let (_dir, _path, store) = temp_store();
    let pending = store.create_approval(request(0, 1000)).unwrap();
    let err = store
        .consume_approval(use_of(pending.id, FP), t(20))
        .unwrap_err();
    assert!(matches!(
        err,
        StoreError::ApprovalNotUsable { id, status: ApprovalStatus::Pending } if id == pending.id
    ));
    assert_eq!(store.get_approval(pending.id).unwrap(), pending);

    for to in [
        ApprovalStatus::Rejected,
        ApprovalStatus::NoAnswer,
        ApprovalStatus::Expired,
    ] {
        let made = store.create_approval(request(0, 1000)).unwrap();
        let done = store.resolve_approval(made.id, to, t(5)).unwrap();
        let err = store
            .consume_approval(use_of(made.id, FP), t(20))
            .unwrap_err();
        assert!(
            matches!(err, StoreError::ApprovalNotUsable { id, status } if id == made.id && status == to),
            "{to:?}"
        );
        assert_eq!(store.get_approval(made.id).unwrap(), done);
    }
}

// 검증: M5
#[test]
fn 지문이_다르면_소비하지_않는다() {
    let (_dir, _path, store) = temp_store();
    let ok = approved(&store);
    // 한 글자만 달라도(뒤 공백 · 대소문자) 다르다.
    for other in [
        "sha256:order-005930-buy-10-limit-70001",
        "sha256:order-005930-buy-10-limit-70000 ",
        "SHA256:order-005930-buy-10-limit-70000",
        "",
    ] {
        let err = store
            .consume_approval(use_of(ok.id, other), t(20))
            .unwrap_err();
        assert!(
            matches!(err, StoreError::ApprovalFingerprintMismatch { id } if id == ok.id),
            "{other:?}"
        );
        assert_eq!(store.get_approval(ok.id).unwrap(), ok);
    }
    // 맞는 지문이면 그 뒤에도 쓸 수 있다(거부가 아무것도 바꾸지 않았다).
    let used = store.consume_approval(use_of(ok.id, FP), t(21)).unwrap();
    assert_eq!(used.consumed_at, Some(t(21)));
}

// 검증: M5
#[test]
fn 효력이_끝난_승인은_소비하지_않는다() {
    let (_dir, _path, store) = temp_store();
    let ok = approved(&store);
    for at in [1000, 1001] {
        let err = store
            .consume_approval(use_of(ok.id, FP), t(at))
            .unwrap_err();
        assert!(matches!(err, StoreError::ApprovalExpired { id } if id == ok.id));
        assert_eq!(store.get_approval(ok.id).unwrap(), ok);
    }
    // 경계: 만료 바로 전은 된다.
    let used = store.consume_approval(use_of(ok.id, FP), t(999)).unwrap();
    assert_eq!(used.consumed_at, Some(t(999)));
}

// 검증: M5
#[test]
fn 결과별로_찾을_수_있고_번호_순서다() {
    let (_dir, _path, store) = temp_store();
    let a = store.create_approval(request(0, 1000)).unwrap();
    let b = store.create_approval(request(0, 1000)).unwrap();
    let c = store.create_approval(request(0, 1000)).unwrap();
    store
        .resolve_approval(b.id, ApprovalStatus::Rejected, t(5))
        .unwrap();
    let pending: Vec<ApprovalId> = store
        .approvals_in_status(ApprovalStatus::Pending)
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(pending, vec![a.id, c.id]);
    let rejected = store.approvals_in_status(ApprovalStatus::Rejected).unwrap();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].id, b.id);
    assert!(
        store
            .approvals_in_status(ApprovalStatus::Approved)
            .unwrap()
            .is_empty()
    );
}

// 검증: M5, D6
#[test]
fn 다시_열어도_결과와_소비가_그대로다() {
    let (_dir, path, store) = temp_store();
    let ok = approved(&store);
    let used = store.consume_approval(use_of(ok.id, FP), t(20)).unwrap();
    let pending = store.create_approval(request(30, 2000)).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_approval(ok.id).unwrap(), used);
    assert_eq!(store.get_approval(pending.id).unwrap(), pending);
    // 다시 열어도 번호는 이어진다(다시 쓰지 않는다).
    let next = store.create_approval(request(40, 2000)).unwrap();
    assert_eq!(next.id, ApprovalId(3));
    // 다시 열어도 한 번만 소비된다.
    assert!(matches!(
        store.consume_approval(use_of(ok.id, FP), t(41)),
        Err(StoreError::ApprovalAlreadyConsumed { .. })
    ));
}

// 검증: M5
#[test]
fn 표의_check가_모르는_결과_글자를_막는다() {
    let (_dir, path, store) = temp_store();
    let made = store.create_approval(request(0, 1000)).unwrap();
    for bad in ["bogus", "Pending", "allowed", ""] {
        let statement = format!("UPDATE approvals SET status = '{bad}' WHERE id = 1");
        assert!(sql(&path, &statement).is_err(), "{bad:?}");
    }
    assert_eq!(store.get_approval(made.id).unwrap(), made);
}

// 검증: M5
#[test]
fn 표에서도_답이_난_결과는_바뀌지_않고_줄은_지워지지_않는다() {
    let (_dir, path, store) = temp_store();
    let ok = approved(&store);
    for other in ["pending", "rejected", "no_answer", "expired"] {
        let statement = format!("UPDATE approvals SET status = '{other}' WHERE id = 1");
        assert!(sql(&path, &statement).is_err(), "{other}");
    }
    assert!(sql(&path, "DELETE FROM approvals").is_err());
    assert!(sql(&path, "DELETE FROM approvals WHERE id = 1").is_err());
    assert_eq!(store.get_approval(ok.id).unwrap(), ok);
}

// 검증: M5
#[test]
fn 표에서도_소비는_승인에서만_한_번_적힌다() {
    let (_dir, path, store) = temp_store();
    // 기다림인 줄(1번)에는 소비 칸을 적을 수 없다.
    let pending = store.create_approval(request(0, 1000)).unwrap();
    assert!(sql(&path, "UPDATE approvals SET consumed_at = 5 WHERE id = 1").is_err());
    assert_eq!(store.get_approval(pending.id).unwrap(), pending);

    // 거절인 줄(2번)에도 적을 수 없다.
    let made = store.create_approval(request(0, 1000)).unwrap();
    let rejected = store
        .resolve_approval(made.id, ApprovalStatus::Rejected, t(5))
        .unwrap();
    assert!(sql(&path, "UPDATE approvals SET consumed_at = 5 WHERE id = 2").is_err());
    assert_eq!(store.get_approval(made.id).unwrap(), rejected);

    // 한 번 적힌 소비 칸(3번)은 비우거나 바꿀 수 없다.
    let ok = approved(&store);
    let used = store.consume_approval(use_of(ok.id, FP), t(20)).unwrap();
    assert!(
        sql(
            &path,
            "UPDATE approvals SET consumed_at = NULL WHERE id = 3"
        )
        .is_err()
    );
    assert!(sql(&path, "UPDATE approvals SET consumed_at = 99 WHERE id = 3").is_err());
    assert_eq!(store.get_approval(ok.id).unwrap(), used);
}
