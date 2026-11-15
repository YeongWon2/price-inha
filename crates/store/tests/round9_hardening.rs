#![cfg(test)]
//! 9차 보강 1~3번: 표가 스스로 지킨다(어떤 SQL로도).
//!
//! 1. 한 승인 번호는 템플릿 실전중 · 개선안 냉각중 / 반영됨 전체에서 한 번만 쓰인다.
//! 2. 개선안이 냉각중 · 반영됨으로 들어갈 때 그 승인은 승인됨이고 소비된 것이어야 한다.
//! 3. 기준 통과 기록(`pass_revision` · `pass_record` · `pass_at`)은 모의중이 아닐 때 바뀌지 않고,
//!    `pass_revision`은 없거나 지금 판과 같다.
//!
//! 저장소 파일을 rusqlite로 직접 고쳐(우회) 막히는지 보고, 정상 API 경로는 계속 되는지 함께 본다.

mod common;

use std::path::Path;

use common::{t, temp_store};
use pi_store::{
    AccountKind, ApprovalId, ApprovalStatus, ApprovalUse, NewApprovalRequest, NewProposal,
    NewTemplate, NewTrade, Proposal, ProposalState, Store, TeamCause, TeamChange, TeamState,
    Template, TemplateState,
};
use serde_json::{Value, json};

const FP: &str = "sha256:round9-hardening";

fn sql(path: &Path, statement: &str) -> rusqlite::Result<usize> {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(statement, [])
}

fn approval_request(store: &Store) -> ApprovalId {
    store
        .create_approval(NewApprovalRequest {
            fingerprint: FP.to_string(),
            summary: json!({ "보강": 9 }),
            created_at: t(0),
            expires_at: t(100_000),
        })
        .unwrap()
        .id
}

/// 승인됨(아직 소비 안 됨).
fn approved(store: &Store) -> ApprovalId {
    let id = approval_request(store);
    store
        .resolve_approval(id, ApprovalStatus::Approved, t(1))
        .unwrap();
    id
}

/// 거절됨.
fn rejected(store: &Store) -> ApprovalId {
    let id = approval_request(store);
    store
        .resolve_approval(id, ApprovalStatus::Rejected, t(1))
        .unwrap();
    id
}

fn use_of(id: ApprovalId) -> ApprovalUse {
    ApprovalUse {
        id,
        fingerprint: FP.to_string(),
    }
}

fn template_value() -> Value {
    json!({ "스타일": "스윙", "R%": "1.0" })
}

fn pass_record() -> Value {
    json!({ "매매": 31, "거래일": 21, "손익": "12000" })
}

fn pending_template(store: &Store) -> Template {
    let made = store
        .create_template(NewTemplate {
            value: template_value(),
            at: t(0),
        })
        .unwrap();
    store
        .submit_template_for_promotion(made.id, &pass_record(), t(10))
        .unwrap()
}

fn live_template(store: &Store) -> Template {
    let pending = pending_template(store);
    let approval = approved(store);
    store
        .promote_template_live(pending.id, use_of(approval), t(20))
        .unwrap()
}

fn proposal(store: &Store, increases_risk: bool) -> Proposal {
    let mut evidence = Vec::new();
    for _ in 0..2 {
        evidence.push(
            store
                .create_trade(NewTrade {
                    symbol: "005930".to_string(),
                    account: AccountKind::Paper,
                    at: t(0),
                })
                .unwrap()
                .id,
        );
    }
    store
        .create_proposal(NewProposal {
            content: json!({ "규칙": "갭 상승 첫 5분에는 사지 않는다" }),
            evidence,
            increases_risk,
            at: t(10),
        })
        .unwrap()
}

fn applied_proposal(store: &Store) -> Proposal {
    let made = proposal(store, false);
    let approval = approved(store);
    store
        .approve_proposal(made.id, use_of(approval), t(30))
        .unwrap()
}

fn cooling_proposal(store: &Store) -> Proposal {
    let made = proposal(store, true);
    let approval = approved(store);
    store
        .approve_proposal(made.id, use_of(approval), t(30))
        .unwrap()
}

fn to_live(id: i64, approval: ApprovalId) -> String {
    format!(
        "UPDATE templates SET state = 'live', approval_id = {}, updated_at = 50 WHERE id = {id}",
        approval.0
    )
}

fn to_applied(id: i64, approval: ApprovalId) -> String {
    format!(
        "UPDATE proposals SET state = 'applied', approval_id = {}, updated_at = 50, applied_at = 50
         WHERE id = {id}",
        approval.0
    )
}

fn to_cooling(id: i64, approval: ApprovalId) -> String {
    format!(
        "UPDATE proposals SET state = 'cooling', approval_id = {}, updated_at = 50 WHERE id = {id}",
        approval.0
    )
}

// ───────────── 1번: 한 승인은 한 번만 쓰인다 ─────────────

// 검증: E36, M5, M3
#[test]
fn 실전중_템플릿이_쓴_승인으로_다른_템플릿을_sql로_실전에_올릴_수_없다() {
    let (_dir, path, store) = temp_store();
    let running = live_template(&store);
    let used = running.approval.unwrap();
    let pending = pending_template(&store);
    let err = sql(&path, &to_live(pending.id.0, used));
    assert!(err.is_err(), "쓴 승인 다시 쓰기가 통과했다");
    assert_eq!(store.get_template(pending.id).unwrap(), pending);
    assert_eq!(store.get_template(running.id).unwrap(), running);
}

// 검증: E36, M5, M3
#[test]
fn 멈춤_템플릿이_쓴_승인도_다른_템플릿에_다시_쓸_수_없다() {
    let (_dir, path, store) = temp_store();
    let running = live_template(&store);
    let stopped = store.stop_template(running.id, t(40)).unwrap();
    let used = stopped.approval.unwrap();
    let pending = pending_template(&store);
    assert!(sql(&path, &to_live(pending.id.0, used)).is_err());
    assert_eq!(store.get_template(pending.id).unwrap(), pending);
}

// 검증: M4, M5, E36
#[test]
fn 템플릿이_쓴_승인으로_개선안을_sql로_반영하거나_냉각할_수_없다() {
    let (_dir, path, store) = temp_store();
    let running = live_template(&store);
    let used = running.approval.unwrap();
    let safe = proposal(&store, false);
    let risky = proposal(&store, true);
    assert!(sql(&path, &to_applied(safe.id.0, used)).is_err());
    assert!(sql(&path, &to_cooling(risky.id.0, used)).is_err());
    assert_eq!(store.get_proposal(safe.id).unwrap(), safe);
    assert_eq!(store.get_proposal(risky.id).unwrap(), risky);
}

// 검증: M4, M5, E36
#[test]
fn 개선안이_쓴_승인으로_다른_개선안이나_템플릿을_sql로_넘길_수_없다() {
    let (_dir, path, store) = temp_store();
    let applied = applied_proposal(&store);
    let cooling = cooling_proposal(&store);
    for used in [applied.approval.unwrap(), cooling.approval.unwrap()] {
        let safe = proposal(&store, false);
        let risky = proposal(&store, true);
        let pending = pending_template(&store);
        assert!(
            sql(&path, &to_applied(safe.id.0, used)).is_err(),
            "{used:?}"
        );
        assert!(
            sql(&path, &to_cooling(risky.id.0, used)).is_err(),
            "{used:?}"
        );
        assert!(
            sql(&path, &to_live(pending.id.0, used)).is_err(),
            "{used:?}"
        );
        assert_eq!(store.get_proposal(safe.id).unwrap(), safe);
        assert_eq!(store.get_proposal(risky.id).unwrap(), risky);
        assert_eq!(store.get_template(pending.id).unwrap(), pending);
    }
    assert_eq!(store.get_proposal(applied.id).unwrap(), applied);
    assert_eq!(store.get_proposal(cooling.id).unwrap(), cooling);
}

// ───────────── 2번: 개선안은 승인되고 소비된 승인으로만 넘어간다 ─────────────

// 검증: M4, M5
#[test]
fn 거절된_승인으로_개선안을_sql로_반영하거나_냉각할_수_없다() {
    let (_dir, path, store) = temp_store();
    let no = rejected(&store);
    let safe = proposal(&store, false);
    let risky = proposal(&store, true);
    assert!(
        sql(&path, &to_applied(safe.id.0, no)).is_err(),
        "거절된 승인으로 반영됐다"
    );
    assert!(sql(&path, &to_cooling(risky.id.0, no)).is_err());
    assert_eq!(store.get_proposal(safe.id).unwrap(), safe);
    assert_eq!(store.get_proposal(risky.id).unwrap(), risky);
    assert_eq!(store.get_approval(no).unwrap().consumed_at, None);
}

// 검증: M4, M5
#[test]
fn 기다리는_승인으로_개선안을_sql로_넘길_수_없다() {
    let (_dir, path, store) = temp_store();
    let waiting = approval_request(&store);
    let safe = proposal(&store, false);
    let risky = proposal(&store, true);
    assert!(sql(&path, &to_applied(safe.id.0, waiting)).is_err());
    assert!(sql(&path, &to_cooling(risky.id.0, waiting)).is_err());
    assert_eq!(store.get_proposal(safe.id).unwrap(), safe);
    assert_eq!(store.get_proposal(risky.id).unwrap(), risky);
}

// 검증: M4, M5
#[test]
fn 소비되지_않은_승인으로_개선안을_sql로_넘길_수_없다() {
    let (_dir, path, store) = temp_store();
    let fresh = approved(&store);
    let safe = proposal(&store, false);
    let risky = proposal(&store, true);
    assert!(sql(&path, &to_applied(safe.id.0, fresh)).is_err());
    assert!(sql(&path, &to_cooling(risky.id.0, fresh)).is_err());
    assert_eq!(store.get_proposal(safe.id).unwrap(), safe);
    assert_eq!(store.get_proposal(risky.id).unwrap(), risky);
    assert_eq!(store.get_approval(fresh).unwrap().consumed_at, None);
}

// 검증: M4, M5
#[test]
fn 없는_승인_번호로_개선안을_sql로_넘길_수_없다() {
    let (_dir, path, store) = temp_store();
    let safe = proposal(&store, false);
    let risky = proposal(&store, true);
    assert!(sql(&path, &to_applied(safe.id.0, ApprovalId(999))).is_err());
    assert!(sql(&path, &to_cooling(risky.id.0, ApprovalId(999))).is_err());
    assert_eq!(store.get_proposal(safe.id).unwrap(), safe);
    assert_eq!(store.get_proposal(risky.id).unwrap(), risky);
}

// ───────────── 3번: 기준 통과 기록은 모의중이 아닐 때 바뀌지 않는다 ─────────────

// 검증: V30, E36, M3
#[test]
fn 실전중_템플릿의_통과_기록을_sql로_고칠_수_없다() {
    let (_dir, path, store) = temp_store();
    let running = live_template(&store);
    let id = running.id.0;
    for statement in [
        format!(
            "UPDATE templates SET pass_record = '{{\"forged\":1}}', pass_revision = 99 WHERE id = {id}"
        ),
        format!("UPDATE templates SET pass_record = '{{\"forged\":1}}' WHERE id = {id}"),
        format!("UPDATE templates SET pass_revision = 99 WHERE id = {id}"),
        format!("UPDATE templates SET pass_at = 12345 WHERE id = {id}"),
    ] {
        assert!(sql(&path, &statement).is_err(), "{statement}");
    }
    assert_eq!(store.get_template(running.id).unwrap(), running);
}

// 검증: V30, M3
#[test]
fn 승격대기와_멈춤_템플릿의_통과_기록도_sql로_고칠_수_없다() {
    let (_dir, path, store) = temp_store();
    let pending = pending_template(&store);
    let running = live_template(&store);
    let stopped = store.stop_template(running.id, t(40)).unwrap();
    for id in [pending.id.0, stopped.id.0] {
        for statement in [
            format!("UPDATE templates SET pass_record = '{{\"forged\":1}}' WHERE id = {id}"),
            format!("UPDATE templates SET pass_at = 12345 WHERE id = {id}"),
            format!("UPDATE templates SET pass_revision = 99 WHERE id = {id}"),
        ] {
            assert!(sql(&path, &statement).is_err(), "{statement}");
        }
    }
    assert_eq!(store.get_template(pending.id).unwrap(), pending);
    assert_eq!(store.get_template(stopped.id).unwrap(), stopped);
}

// 검증: V30, E36, M3
#[test]
fn 실전으로_올리면서_통과_기록을_바꿀_수_없다() {
    let (_dir, path, store) = temp_store();
    let pending = pending_template(&store);
    // 아무 데도 쓰이지 않은, 승인되고 소비된 승인.
    let approval = approved(&store);
    store.consume_approval(use_of(approval), t(15)).unwrap();
    let statement = format!(
        "UPDATE templates SET state = 'live', approval_id = {}, updated_at = 50,
                pass_record = '{{\"forged\":1}}'
         WHERE id = {}",
        approval.0, pending.id.0
    );
    assert!(sql(&path, &statement).is_err());
    assert_eq!(store.get_template(pending.id).unwrap(), pending);
}

// 검증: V30, M3
#[test]
fn 승격대기로_들어갈_때_통과_판은_지금_판이어야_한다() {
    let (_dir, path, store) = temp_store();
    let made = store
        .create_template(NewTemplate {
            value: template_value(),
            at: t(0),
        })
        .unwrap();
    let id = made.id.0;
    for revision in ["revision + 1", "revision + 98", "0"] {
        let statement = format!(
            "UPDATE templates SET state = 'promotion_pending', pass_revision = {revision},
                    pass_record = '{{}}', pass_at = 10, updated_at = 10
             WHERE id = {id}"
        );
        assert!(sql(&path, &statement).is_err(), "{statement}");
    }
    assert_eq!(store.get_template(made.id).unwrap(), made);
}

// ───────────── 대조: 정상 API 경로는 계속 된다 ─────────────

// 검증: E36, V30, M3, M5
#[test]
fn 정상_경로로_승격_실전_멈춤이_된다() {
    let (_dir, _path, store) = temp_store();
    let pending = pending_template(&store);
    let pass = pending.criteria_pass.clone().unwrap();
    assert_eq!(pass.revision, pending.revision);
    let approval = approved(&store);
    let running = store
        .promote_template_live(pending.id, use_of(approval), t(20))
        .unwrap();
    assert_eq!(running.state, TemplateState::Live);
    assert_eq!(running.approval, Some(approval));
    assert_eq!(running.criteria_pass, Some(pass.clone()));
    let stopped = store.stop_template(running.id, t(30)).unwrap();
    assert_eq!(stopped.state, TemplateState::Stopped);
    assert_eq!(stopped.approval, Some(approval));
    assert_eq!(stopped.criteria_pass, Some(pass));
}

// 검증: V30, M3
#[test]
fn 정상_경로로_승격을_거절하면_모의중으로_돌아가고_다시_올릴_수_있다() {
    let (_dir, _path, store) = temp_store();
    let pending = pending_template(&store);
    let back = store.reject_template_promotion(pending.id, t(12)).unwrap();
    assert_eq!(back.state, TemplateState::Simulating);
    assert_eq!(back.criteria_pass, None);
    let revised = store
        .revise_template(back.id, &json!({ "R%": "0.7" }), t(13))
        .unwrap();
    let again = store
        .submit_template_for_promotion(back.id, &pass_record(), t(14))
        .unwrap();
    assert_eq!(again.state, TemplateState::PromotionPending);
    assert_eq!(again.criteria_pass.unwrap().revision, revised.revision);
}

// 검증: M3, M1
#[test]
fn 정상_경로로_전체_정지가_실전중_템플릿을_멈춘다() {
    let (_dir, _path, store) = temp_store();
    let running = live_template(&store);
    for (to, at, cause) in [
        (TeamState::Operating, 100, TeamCause::TradingStart),
        (TeamState::FullStop, 101, TeamCause::MaxLoss),
    ] {
        store
            .change_team_state(TeamChange {
                to,
                at: t(at),
                cause,
                detail: "보강 대조".to_string(),
                owner_approved: false,
            })
            .unwrap();
    }
    let now = store.get_template(running.id).unwrap();
    assert_eq!(now.state, TemplateState::Stopped);
    assert_eq!(now.approval, running.approval);
    assert_eq!(now.criteria_pass, running.criteria_pass);
}

// 검증: M4, M5
#[test]
fn 정상_경로로_개선안이_반영되고_냉각을_거쳐_반영된다() {
    let (_dir, _path, store) = temp_store();
    // 위험이 없는 개선안은 승인하면 바로 반영된다.
    let safe = proposal(&store, false);
    let a = approved(&store);
    let applied = store.approve_proposal(safe.id, use_of(a), t(30)).unwrap();
    assert_eq!(applied.state, ProposalState::Applied);
    assert_eq!(applied.approval, Some(a));
    assert_eq!(applied.applied_at, Some(t(30)));
    assert_eq!(store.get_approval(a).unwrap().consumed_at, Some(t(30)));

    // 위험을 키우는 개선안은 냉각중을 거쳐, 같은 승인 번호를 지닌 채 반영된다.
    let risky = proposal(&store, true);
    let b = approved(&store);
    let cooling = store.approve_proposal(risky.id, use_of(b), t(31)).unwrap();
    assert_eq!(cooling.state, ProposalState::Cooling);
    assert_eq!(cooling.approval, Some(b));
    let done = store.apply_cooled_proposal(risky.id, t(40)).unwrap();
    assert_eq!(done.state, ProposalState::Applied);
    assert_eq!(done.approval, Some(b));
    assert_eq!(done.applied_at, Some(t(40)));
}

// 검증: M4, M5, E36
#[test]
fn 정상_경로로_템플릿과_개선안이_각자_다른_승인을_쓴다() {
    let (_dir, path, store) = temp_store();
    let running = live_template(&store);
    let applied = applied_proposal(&store);
    let cooling = cooling_proposal(&store);
    let ids = [
        running.approval.unwrap(),
        applied.approval.unwrap(),
        cooling.approval.unwrap(),
    ];
    assert_ne!(ids[0], ids[1]);
    assert_ne!(ids[0], ids[2]);
    assert_ne!(ids[1], ids[2]);
    let done = store.apply_cooled_proposal(cooling.id, t(50)).unwrap();
    assert_eq!(done.state, ProposalState::Applied);
    assert_eq!(done.approval, cooling.approval);
    // 다시 열어도 그대로다.
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_template(running.id).unwrap(), running);
    assert_eq!(store.get_proposal(applied.id).unwrap(), applied);
    assert_eq!(store.get_proposal(cooling.id).unwrap(), done);
}
