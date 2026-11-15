#![cfg(test)]
//! 9차 1번: 템플릿 표와 전이 강제 (M3 · E36 · V29 · V30).
//!
//! - 새 템플릿은 모의중이다(V29).
//! - 승격대기는 기준 통과 기록과 함께만, 실전중은 승격대기에서 승인을 소비해서만 된다(V30 · E36).
//! - 전체 정지가 되면 실전중 템플릿은 멈춤이 된다(M3, F6 9b).
//! - 표의 CHECK가 승인 번호나 기준 통과 기록 없는 실전중을 어떤 SQL로도 막는다(E36).

mod common;

use std::path::Path;

use common::{t, temp_store};
use pi_store::{
    ApprovalId, ApprovalStatus, ApprovalUse, DenyReason, NewApprovalRequest, NewTemplate, Store,
    StoreError, TeamCause, TeamChange, TeamState, Template, TemplateId, TemplateState, Transition,
};
use serde_json::{Value, json};

const FP: &str = "sha256:template-1-live";

fn value(r: &str) -> Value {
    json!({ "스타일": "스윙", "R%": r, "하루_한도": "2", "손절": "고정" })
}

fn new_template() -> NewTemplate {
    NewTemplate {
        value: value("1.0"),
        at: t(0),
    }
}

fn pass_record() -> Value {
    json!({ "매매": 31, "거래일": 21, "손익": "12000", "최대_낙폭": "3.1" })
}

/// 지문 [`FP`]로 승인까지 마친 요청 번호(만료 t(100_000)).
fn approved(store: &Store, fingerprint: &str) -> ApprovalId {
    let made = store
        .create_approval(NewApprovalRequest {
            fingerprint: fingerprint.to_string(),
            summary: json!({ "템플릿": 1, "실전": true }),
            created_at: t(0),
            expires_at: t(100_000),
        })
        .unwrap();
    store
        .resolve_approval(made.id, ApprovalStatus::Approved, t(1))
        .unwrap()
        .id
}

fn use_of(id: ApprovalId) -> ApprovalUse {
    ApprovalUse {
        id,
        fingerprint: FP.to_string(),
    }
}

/// 승격대기까지 올린 템플릿.
fn pending_promotion(store: &Store) -> Template {
    let made = store.create_template(new_template()).unwrap();
    store
        .submit_template_for_promotion(made.id, &pass_record(), t(10))
        .unwrap()
}

/// 실전중까지 올린 템플릿.
fn live(store: &Store) -> Template {
    let pending = pending_promotion(store);
    let approval = approved(store, FP);
    store
        .promote_template_live(pending.id, use_of(approval), t(20))
        .unwrap()
}

fn denied(err: StoreError, from: TemplateState, to: TemplateState) {
    match err {
        StoreError::TransitionDenied { transition, reason } => {
            assert_eq!(transition, Transition::Template { from, to });
            assert_eq!(reason, DenyReason::NotAllowed);
        }
        other => panic!("{from:?} → {to:?}: {other:?}"),
    }
}

fn full_stop(store: &Store, at: i64) {
    store
        .change_team_state(TeamChange {
            to: TeamState::Operating,
            at: t(at),
            cause: TeamCause::TradingStart,
            detail: "운용 시작".to_string(),
            owner_approved: false,
        })
        .unwrap();
    store
        .change_team_state(TeamChange {
            to: TeamState::FullStop,
            at: t(at + 1),
            cause: TeamCause::MaxLoss,
            detail: "최대 손실".to_string(),
            owner_approved: false,
        })
        .unwrap();
}

fn sql(path: &Path, statement: &str) -> rusqlite::Result<usize> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(statement, [])
}

// 검증: V29, M3
#[test]
fn 새_템플릿은_모의중_판_1이고_승인도_통과_기록도_없다() {
    let (_dir, _path, store) = temp_store();
    let made = store.create_template(new_template()).unwrap();
    assert_eq!(made.id, TemplateId(1));
    assert_eq!(made.state, TemplateState::Simulating);
    assert_eq!(made.revision, 1);
    assert_eq!(made.value, value("1.0"));
    assert_eq!(made.criteria_pass, None);
    assert_eq!(made.approval, None);
    assert_eq!(made.created_at, t(0));
    assert_eq!(made.updated_at, t(0));
    assert_eq!(store.get_template(made.id).unwrap(), made);
    let second = store.create_template(new_template()).unwrap();
    assert_eq!(second.id, TemplateId(2));
    assert_eq!(second.state, TemplateState::Simulating);
}

// 검증: M3
#[test]
fn 없는_템플릿은_찾을_수_없다() {
    let (_dir, _path, store) = temp_store();
    assert!(matches!(
        store.get_template(TemplateId(9)),
        Err(StoreError::TemplateNotFound(TemplateId(9)))
    ));
    assert!(matches!(
        store.stop_template(TemplateId(9), t(1)),
        Err(StoreError::TemplateNotFound(TemplateId(9)))
    ));
    assert!(matches!(
        store.template_revisions(TemplateId(9)),
        Err(StoreError::TemplateNotFound(TemplateId(9)))
    ));
}

// 검증: M3
#[test]
fn 모의중_값은_판_번호를_붙여_쌓이고_옛_판이_남는다() {
    let (_dir, _path, store) = temp_store();
    let made = store.create_template(new_template()).unwrap();
    let second = store.revise_template(made.id, &value("0.8"), t(5)).unwrap();
    assert_eq!(second.revision, 2);
    assert_eq!(second.value, value("0.8"));
    assert_eq!(second.updated_at, t(5));
    let third = store.revise_template(made.id, &value("0.5"), t(6)).unwrap();
    assert_eq!(third.revision, 3);
    let revisions = store.template_revisions(made.id).unwrap();
    let seen: Vec<(u32, Value, i64)> = revisions
        .into_iter()
        .map(|r| (r.revision, r.value, r.at.0))
        .collect();
    assert_eq!(
        seen,
        vec![
            (1, value("1.0"), t(0).0),
            (2, value("0.8"), t(5).0),
            (3, value("0.5"), t(6).0),
        ]
    );
}

// 검증: M3, E36
#[test]
fn 모의중이_아니면_값을_바꿀_수_없다() {
    let (_dir, _path, store) = temp_store();
    let pending = pending_promotion(&store);
    let err = store
        .revise_template(pending.id, &value("3.0"), t(11))
        .unwrap_err();
    assert!(matches!(
        err,
        StoreError::TemplateLocked {
            state: TemplateState::PromotionPending
        }
    ));
    assert_eq!(store.get_template(pending.id).unwrap(), pending);
    assert_eq!(store.template_revisions(pending.id).unwrap().len(), 1);

    let running = live(&store);
    let err = store
        .revise_template(running.id, &value("3.0"), t(21))
        .unwrap_err();
    assert!(matches!(
        err,
        StoreError::TemplateLocked {
            state: TemplateState::Live
        }
    ));
    let stopped = store.stop_template(running.id, t(22)).unwrap();
    let err = store
        .revise_template(running.id, &value("3.0"), t(23))
        .unwrap_err();
    assert!(matches!(
        err,
        StoreError::TemplateLocked {
            state: TemplateState::Stopped
        }
    ));
    assert_eq!(store.get_template(running.id).unwrap(), stopped);
}

// 검증: V30, M3
#[test]
fn 승격대기로_갈_때_기준_통과_기록이_지금_판과_함께_남는다() {
    let (_dir, _path, store) = temp_store();
    let made = store.create_template(new_template()).unwrap();
    store.revise_template(made.id, &value("0.8"), t(5)).unwrap();
    let pending = store
        .submit_template_for_promotion(made.id, &pass_record(), t(10))
        .unwrap();
    assert_eq!(pending.state, TemplateState::PromotionPending);
    assert_eq!(pending.updated_at, t(10));
    let pass = pending.criteria_pass.clone().unwrap();
    assert_eq!(pass.revision, 2);
    assert_eq!(pass.record, pass_record());
    assert_eq!(pass.at, t(10));
    // 승격대기는 실전이 아니다: 승인 번호가 없다.
    assert_eq!(pending.approval, None);
}

// 검증: V30, M3
#[test]
fn 승격대기는_모의중에서만_간다() {
    let (_dir, _path, store) = temp_store();
    let pending = pending_promotion(&store);
    let err = store
        .submit_template_for_promotion(pending.id, &pass_record(), t(11))
        .unwrap_err();
    denied(
        err,
        TemplateState::PromotionPending,
        TemplateState::PromotionPending,
    );
    assert_eq!(store.get_template(pending.id).unwrap(), pending);

    let running = live(&store);
    let err = store
        .submit_template_for_promotion(running.id, &pass_record(), t(21))
        .unwrap_err();
    denied(err, TemplateState::Live, TemplateState::PromotionPending);
    assert_eq!(store.get_template(running.id).unwrap(), running);
}

// 검증: V30, M3
#[test]
fn 승격을_거절하면_모의중으로_돌아가고_통과_기록이_지워진다() {
    let (_dir, _path, store) = temp_store();
    let pending = pending_promotion(&store);
    let back = store.reject_template_promotion(pending.id, t(12)).unwrap();
    assert_eq!(back.state, TemplateState::Simulating);
    assert_eq!(back.criteria_pass, None);
    assert_eq!(back.approval, None);
    assert_eq!(back.updated_at, t(12));
    // 모의중이니 값을 다시 바꿀 수 있고, 실전으로 바로 갈 수는 없다.
    store
        .revise_template(back.id, &value("0.7"), t(13))
        .unwrap();
    let approval = approved(&store, FP);
    let err = store
        .promote_template_live(back.id, use_of(approval), t(14))
        .unwrap_err();
    denied(err, TemplateState::Simulating, TemplateState::Live);
    // 승인은 소비되지 않았다.
    assert_eq!(store.get_approval(approval).unwrap().consumed_at, None);
}

// 검증: M3
#[test]
fn 승격대기가_아니면_승격_거절은_거부된다() {
    let (_dir, _path, store) = temp_store();
    let made = store.create_template(new_template()).unwrap();
    let err = store.reject_template_promotion(made.id, t(1)).unwrap_err();
    denied(err, TemplateState::Simulating, TemplateState::Simulating);
    assert_eq!(store.get_template(made.id).unwrap(), made);
}

// 검증: E36, V30, M3, M5
#[test]
fn 승격대기에서_승인을_소비해야_실전중이_된다() {
    let (_dir, _path, store) = temp_store();
    let pending = pending_promotion(&store);
    let approval = approved(&store, FP);
    let running = store
        .promote_template_live(pending.id, use_of(approval), t(20))
        .unwrap();
    assert_eq!(running.state, TemplateState::Live);
    assert_eq!(running.approval, Some(approval));
    assert_eq!(running.criteria_pass, pending.criteria_pass);
    assert_eq!(running.updated_at, t(20));
    assert_eq!(store.get_template(pending.id).unwrap(), running);
    assert_eq!(
        store.get_approval(approval).unwrap().consumed_at,
        Some(t(20))
    );
}

// 검증: E36, V29, V30, M3
#[test]
fn 모의중에서_바로_실전중은_없다() {
    let (_dir, _path, store) = temp_store();
    let made = store.create_template(new_template()).unwrap();
    let approval = approved(&store, FP);
    let err = store
        .promote_template_live(made.id, use_of(approval), t(5))
        .unwrap_err();
    denied(err, TemplateState::Simulating, TemplateState::Live);
    assert_eq!(store.get_template(made.id).unwrap(), made);
    assert_eq!(store.get_approval(approval).unwrap().consumed_at, None);
    assert!(
        store
            .templates_in_state(TemplateState::Live)
            .unwrap()
            .is_empty()
    );
}

// 검증: E36, V30, M3, M5
#[test]
fn 쓸_수_없는_승인으로는_실전중이_되지_않는다() {
    let (_dir, _path, store) = temp_store();
    let pending = pending_promotion(&store);

    // 없는 승인.
    let err = store
        .promote_template_live(pending.id, use_of(ApprovalId(77)), t(20))
        .unwrap_err();
    assert!(matches!(err, StoreError::ApprovalNotFound(ApprovalId(77))));

    // 기다림 · 거절인 승인.
    let waiting = store
        .create_approval(NewApprovalRequest {
            fingerprint: FP.to_string(),
            summary: json!({}),
            created_at: t(0),
            expires_at: t(100_000),
        })
        .unwrap();
    let err = store
        .promote_template_live(pending.id, use_of(waiting.id), t(20))
        .unwrap_err();
    assert!(matches!(
        err,
        StoreError::ApprovalNotUsable {
            status: ApprovalStatus::Pending,
            ..
        }
    ));
    store
        .resolve_approval(waiting.id, ApprovalStatus::Rejected, t(21))
        .unwrap();
    let err = store
        .promote_template_live(pending.id, use_of(waiting.id), t(22))
        .unwrap_err();
    assert!(matches!(
        err,
        StoreError::ApprovalNotUsable {
            status: ApprovalStatus::Rejected,
            ..
        }
    ));

    // 다른 내용을 승인한 것.
    let other = approved(&store, "sha256:something-else");
    let err = store
        .promote_template_live(pending.id, use_of(other), t(23))
        .unwrap_err();
    assert!(matches!(err, StoreError::ApprovalFingerprintMismatch { id } if id == other));
    assert_eq!(store.get_approval(other).unwrap().consumed_at, None);

    // 효력이 끝난 것.
    let late = approved(&store, FP);
    let err = store
        .promote_template_live(pending.id, use_of(late), t(100_000))
        .unwrap_err();
    assert!(matches!(err, StoreError::ApprovalExpired { id } if id == late));

    assert_eq!(store.get_template(pending.id).unwrap(), pending);
}

// 검증: E36, M3, M5
#[test]
fn 한_승인으로_두_템플릿을_실전에_올릴_수_없다() {
    let (_dir, _path, store) = temp_store();
    let first = pending_promotion(&store);
    let second = pending_promotion(&store);
    let approval = approved(&store, FP);
    store
        .promote_template_live(first.id, use_of(approval), t(20))
        .unwrap();
    let err = store
        .promote_template_live(second.id, use_of(approval), t(21))
        .unwrap_err();
    assert!(matches!(err, StoreError::ApprovalAlreadyConsumed { id } if id == approval));
    assert_eq!(store.get_template(second.id).unwrap(), second);
}

// 검증: M3
#[test]
fn 실전중만_멈출_수_있고_멈춤에서는_나가지_않는다() {
    let (_dir, _path, store) = temp_store();
    let made = store.create_template(new_template()).unwrap();
    denied(
        store.stop_template(made.id, t(1)).unwrap_err(),
        TemplateState::Simulating,
        TemplateState::Stopped,
    );
    let pending = pending_promotion(&store);
    denied(
        store.stop_template(pending.id, t(11)).unwrap_err(),
        TemplateState::PromotionPending,
        TemplateState::Stopped,
    );

    let running = live(&store);
    let stopped = store.stop_template(running.id, t(30)).unwrap();
    assert_eq!(stopped.state, TemplateState::Stopped);
    assert_eq!(stopped.updated_at, t(30));
    // 멈춰도 어떤 승인으로 실전에 올랐는지는 남는다.
    assert_eq!(stopped.approval, running.approval);

    denied(
        store.stop_template(running.id, t(31)).unwrap_err(),
        TemplateState::Stopped,
        TemplateState::Stopped,
    );
    denied(
        store
            .submit_template_for_promotion(running.id, &pass_record(), t(32))
            .unwrap_err(),
        TemplateState::Stopped,
        TemplateState::PromotionPending,
    );
    denied(
        store
            .reject_template_promotion(running.id, t(33))
            .unwrap_err(),
        TemplateState::Stopped,
        TemplateState::Simulating,
    );
    let approval = approved(&store, FP);
    denied(
        store
            .promote_template_live(running.id, use_of(approval), t(34))
            .unwrap_err(),
        TemplateState::Stopped,
        TemplateState::Live,
    );
    assert_eq!(store.get_template(running.id).unwrap(), stopped);
}

// 검증: M3, M1
#[test]
fn 전체_정지가_되면_실전중_템플릿만_멈춤이_된다() {
    let (_dir, _path, store) = temp_store();
    let simulating = store.create_template(new_template()).unwrap();
    let pending = pending_promotion(&store);
    let first = live(&store);
    let second = live(&store);
    full_stop(&store, 100);

    for id in [first.id, second.id] {
        let now = store.get_template(id).unwrap();
        assert_eq!(now.state, TemplateState::Stopped);
        assert_eq!(now.updated_at, t(101));
    }
    assert_eq!(store.get_template(simulating.id).unwrap(), simulating);
    assert_eq!(store.get_template(pending.id).unwrap(), pending);
    assert!(
        store
            .templates_in_state(TemplateState::Live)
            .unwrap()
            .is_empty()
    );
}

// 검증: M3, M1
#[test]
fn 거부된_팀_상태_변경은_템플릿을_바꾸지_않는다() {
    let (_dir, _path, store) = temp_store();
    let running = live(&store);
    // 쉬는 중에서 전체 정지는 표에 없다.
    let err = store
        .change_team_state(TeamChange {
            to: TeamState::FullStop,
            at: t(50),
            cause: TeamCause::MaxLoss,
            detail: "최대 손실".to_string(),
            owner_approved: false,
        })
        .unwrap_err();
    assert!(matches!(err, StoreError::TransitionDenied { .. }));
    assert_eq!(store.get_template(running.id).unwrap(), running);
}

// 검증: M3
#[test]
fn 상태별로_찾을_수_있고_번호_순서다() {
    let (_dir, _path, store) = temp_store();
    let a = store.create_template(new_template()).unwrap();
    let b = pending_promotion(&store);
    let c = store.create_template(new_template()).unwrap();
    let ids = |state| -> Vec<TemplateId> {
        store
            .templates_in_state(state)
            .unwrap()
            .into_iter()
            .map(|x| x.id)
            .collect()
    };
    assert_eq!(ids(TemplateState::Simulating), vec![a.id, c.id]);
    assert_eq!(ids(TemplateState::PromotionPending), vec![b.id]);
    assert!(ids(TemplateState::Live).is_empty());
    assert!(ids(TemplateState::Stopped).is_empty());
}

// 검증: M3, V30, D6
#[test]
fn 다시_열어도_템플릿과_판과_통과_기록이_그대로다() {
    let (_dir, path, store) = temp_store();
    let simulating = store.create_template(new_template()).unwrap();
    store
        .revise_template(simulating.id, &value("0.9"), t(3))
        .unwrap();
    let simulating = store.get_template(simulating.id).unwrap();
    let running = live(&store);
    let revisions = store.template_revisions(simulating.id).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_template(simulating.id).unwrap(), simulating);
    assert_eq!(store.get_template(running.id).unwrap(), running);
    assert_eq!(store.template_revisions(simulating.id).unwrap(), revisions);
    let next = store.create_template(new_template()).unwrap();
    assert_eq!(next.id, TemplateId(3));
}

// 검증: E36, M3
#[test]
fn 표의_check가_승인과_통과_기록_없는_실전중을_막는다() {
    let (_dir, path, store) = temp_store();
    // 1번: 모의중(통과 기록도 승인도 없다), 2번: 승격대기(통과 기록만 있다).
    let simulating = store.create_template(new_template()).unwrap();
    let pending = pending_promotion(&store);
    for id in [simulating.id.0, pending.id.0] {
        let statement = format!("UPDATE templates SET state = 'live' WHERE id = {id}");
        assert!(sql(&path, &statement).is_err(), "{id}");
    }
    assert!(sql(&path, "UPDATE templates SET state = 'bogus' WHERE id = 1").is_err());
    assert_eq!(store.get_template(simulating.id).unwrap(), simulating);
    assert_eq!(store.get_template(pending.id).unwrap(), pending);
    assert!(
        store
            .templates_in_state(TemplateState::Live)
            .unwrap()
            .is_empty()
    );
}
