#![cfg(test)]
//! 16차: 사용자 변경 반영이 승격 검증을 우회하지 못하게 막는다.
//!
//! - 승격대기 템플릿에는 사용자 변경 리비전을 쓰지 않는다(API와 표 트리거 모두). 실전중 · 모의중은 그대로 반영.
//! - 통과 기록(`pass_revision`) 뒤 리비전이 바뀐 템플릿은 실전으로 승격되지 않는다(직접 SQL로 흉내).
//! - 판 10 옮기기 뒤 `PRAGMA integrity_check`는 ok다.

mod common;

use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{
    ApprovalId, ApprovalStatus, ApprovalUse, NewApprovalRequest, NewTemplate, NewUserProposal,
    Proposal, ProposalState, Store, Template, TemplateId, TemplateState,
};
use serde_json::json;

// ───────────── 도우미 ─────────────

fn approved(store: &Store, fingerprint: &str) -> ApprovalUse {
    let id: ApprovalId = store
        .create_approval(NewApprovalRequest {
            fingerprint: fingerprint.to_string(),
            summary: json!({ "대상": fingerprint }),
            created_at: t(0),
            expires_at: t(1_000_000),
        })
        .unwrap()
        .id;
    store
        .resolve_approval(id, ApprovalStatus::Approved, t(1))
        .unwrap();
    ApprovalUse {
        id,
        fingerprint: fingerprint.to_string(),
    }
}

fn old_value() -> serde_json::Value {
    json!({ "스타일": "스윙", "R%": "1.0", "손절": "고정" })
}

fn new_value() -> serde_json::Value {
    json!({ "스타일": "스윙", "R%": "1.5", "손절": "넓게" })
}

fn simulating_template(store: &Store) -> Template {
    store
        .create_template(NewTemplate {
            value: old_value(),
            at: t(0),
        })
        .unwrap()
}

fn pending_template(store: &Store) -> Template {
    let made = simulating_template(store);
    store
        .submit_template_for_promotion(made.id, &json!({ "매매": 31 }), t(10))
        .unwrap()
}

fn live_template(store: &Store) -> Template {
    let made = pending_template(store);
    let use_ = approved(store, "sha256:template-live");
    store.promote_template_live(made.id, use_, t(20)).unwrap()
}

fn cooling_user_proposal(store: &Store, template: TemplateId, tag: &str) -> Proposal {
    let made = store
        .create_user_proposal(NewUserProposal {
            template,
            content: new_value(),
            at: t(30),
        })
        .unwrap();
    let use_ = approved(store, tag);
    let cooling = store.approve_proposal(made.id, use_, t(31)).unwrap();
    assert_eq!(cooling.state, ProposalState::Cooling);
    cooling
}

fn revision_numbers(store: &Store, id: TemplateId) -> Vec<u32> {
    store
        .template_revisions(id)
        .unwrap()
        .iter()
        .map(|r| r.revision)
        .collect()
}

fn raw(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(path).unwrap()
}

fn copy_of_v9() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v9.db");
    let to = dir.path().join("pi.sqlite");
    std::fs::copy(from, &to).unwrap();
    (dir, to)
}

// ───────────── 1. 승격대기 템플릿에는 사용자 변경을 반영하지 않는다 ─────────────

// 검증: E36 R14-1
#[test]
fn 승격대기_템플릿에_사용자_변경을_반영하면_거부되고_개선안_템플릿_모두_그대로다() {
    let (_dir, _path, store) = temp_store();
    let template = pending_template(&store);
    let cooling = cooling_user_proposal(&store, template.id, "sha256:pending-user");

    let result = store.apply_cooled_user_proposal(cooling.id, &new_value(), t(50));

    assert!(
        result.is_err(),
        "승격대기에는 사용자 변경 리비전을 쓰지 못한다"
    );
    assert_eq!(store.get_proposal(cooling.id).unwrap(), cooling);
    assert_eq!(store.get_template(template.id).unwrap(), template);
    assert_eq!(revision_numbers(&store, template.id), vec![1]);
    assert_eq!(
        store.get_template(template.id).unwrap().state,
        TemplateState::PromotionPending
    );
}

// 검증: E36 R14-1
#[test]
fn 승격대기에서_거부된_뒤에도_승격은_원래_통과_기록_그대로_된다() {
    let (_dir, _path, store) = temp_store();
    let template = pending_template(&store);
    let cooling = cooling_user_proposal(&store, template.id, "sha256:pending-user-2");
    assert!(
        store
            .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
            .is_err()
    );

    let use_ = approved(&store, "sha256:template-live-after-deny");
    let live = store
        .promote_template_live(template.id, use_, t(60))
        .unwrap();
    assert_eq!(live.state, TemplateState::Live);
    assert_eq!(live.revision, 1);
    assert_eq!(live.value, old_value());
}

// 검증: E36 R14-1
#[test]
fn 실전중_템플릿에는_지금처럼_사용자_변경이_반영된다() {
    let (_dir, _path, store) = temp_store();
    let template = live_template(&store);
    let cooling = cooling_user_proposal(&store, template.id, "sha256:live-user");

    store
        .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
        .unwrap();

    let now = store.get_template(template.id).unwrap();
    assert_eq!(now.state, TemplateState::Live);
    assert_eq!(now.revision, 2);
    assert_eq!(now.value, new_value());
    assert_eq!(
        store.get_proposal(cooling.id).unwrap().state,
        ProposalState::Applied
    );
}

// 검증: E36 R14-1
#[test]
fn 모의중_템플릿에는_지금처럼_사용자_변경이_반영된다() {
    let (_dir, _path, store) = temp_store();
    let template = simulating_template(&store);
    let cooling = cooling_user_proposal(&store, template.id, "sha256:sim-user");

    store
        .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
        .unwrap();

    let now = store.get_template(template.id).unwrap();
    assert_eq!(now.state, TemplateState::Simulating);
    assert_eq!(now.revision, 2);
    assert_eq!(now.value, new_value());
    assert_eq!(revision_numbers(&store, template.id), vec![1, 2]);
}

// 검증: E36 R14-1
#[test]
fn 멈춤_템플릿에는_지금처럼_사용자_변경이_거부된다() {
    let (_dir, _path, store) = temp_store();
    let live = live_template(&store);
    let cooling = cooling_user_proposal(&store, live.id, "sha256:stopped-user");
    let stopped = store.stop_template(live.id, t(40)).unwrap();

    assert!(
        store
            .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
            .is_err()
    );
    assert_eq!(store.get_proposal(cooling.id).unwrap(), cooling);
    assert_eq!(store.get_template(live.id).unwrap(), stopped);
    assert_eq!(revision_numbers(&store, live.id), vec![1]);
}

// ───────────── 1'. 표 트리거도 승격대기의 사용자 변경 리비전을 막는다 ─────────────

// 검증: E36 R14-1
#[test]
fn 직접_sql로도_승격대기_템플릿에_사용자_변경_리비전을_쓸_수_없다() {
    let (_dir, path, store) = temp_store();
    let template = pending_template(&store);
    let cooling = cooling_user_proposal(&store, template.id, "sha256:pending-raw");
    drop(store);

    // API가 하는 쓰기(개선안 반영됨 → user_revisions → template_revisions → templates)를 손으로 흉내 낸다.
    let conn = raw(&path);
    let result = conn.execute_batch(&format!(
        "BEGIN;
         UPDATE proposals SET state = 'applied', updated_at = {at}, applied_at = {at}
             WHERE id = {pid};
         INSERT INTO user_revisions (proposal_id, template_id, revision)
             VALUES ({pid}, {tid}, 2);
         INSERT INTO template_revisions (template_id, revision, value, at)
             VALUES ({tid}, 2, '{{\"R%\":\"9\"}}', {at});
         UPDATE templates SET revision = 2, updated_at = {at} WHERE id = {tid};
         COMMIT;",
        at = t(50).0,
        pid = cooling.id.0,
        tid = template.id.0,
    ));
    assert!(
        result.is_err(),
        "표 트리거가 승격대기의 사용자 변경 리비전을 막아야 한다"
    );
    drop(conn);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.get_template(template.id).unwrap().revision, 1);
    assert_eq!(revision_numbers(&store, template.id), vec![1]);
}

// ───────────── 2. 통과 기록 뒤 리비전이 앞선 템플릿은 실전이 되지 않는다 ─────────────

/// 실전중 템플릿에 사용자 변경을 반영해(리비전 2, 통과 기록은 리비전 1) 그 줄을 승격대기로 되돌린 상태를
/// 직접 SQL로 만든다 — 통과 기록 뒤 리비전이 앞선 템플릿.
fn pending_with_stale_pass(store: &Store, path: &Path) -> TemplateId {
    let template = live_template(store);
    let cooling = cooling_user_proposal(store, template.id, "sha256:stale-pass");
    store
        .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
        .unwrap();
    // 실전중 → 승격대기와 승인 비우기는 전이 · 고정 트리거가 막는 길이라, 그 둘만 잠시 빼고 줄을 옮긴 뒤
    // 같은 글로 되살린다(옛 파일 · 우회로 생겼을 상태를 흉내 낸다. 다른 트리거는 그대로 둔다).
    let conn = raw(path);
    let saved: Vec<String> = ["templates_transition", "templates_fixed"]
        .iter()
        .map(|name| {
            conn.query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
                [name],
                |row| row.get(0),
            )
            .unwrap()
        })
        .collect();
    conn.execute_batch("DROP TRIGGER templates_transition; DROP TRIGGER templates_fixed;")
        .unwrap();
    conn.execute(
        "UPDATE templates SET state = 'promotion_pending', approval_id = NULL WHERE id = ?1",
        [template.id.0],
    )
    .unwrap();
    for sql in &saved {
        conn.execute_batch(sql).unwrap();
    }
    drop(conn);
    let now = store.get_template(template.id).unwrap();
    assert_eq!(now.state, TemplateState::PromotionPending);
    assert_eq!(now.revision, 2);
    template.id
}

// 검증: E36 R14-1
#[test]
fn 통과_기록_뒤_리비전이_앞선_템플릿은_승격이_거부되고_그대로다() {
    let (_dir, path, store) = temp_store();
    let id = pending_with_stale_pass(&store, &path);
    let before = store.get_template(id).unwrap();

    let use_ = approved(&store, "sha256:promote-stale");
    let result = store.promote_template_live(id, use_.clone(), t(60));

    assert!(result.is_err());
    assert_eq!(store.get_template(id).unwrap(), before);
    assert_eq!(before.state, TemplateState::PromotionPending);
    // 승인은 쓰이지 않았다 — 다음 승격에 그대로 쓸 수 있는 상태다.
    assert_eq!(
        store.get_approval(use_.id).unwrap().status,
        ApprovalStatus::Approved
    );
}

// 검증: E36 R14-1
#[test]
fn 통과_기록_뒤_리비전이_앞선_템플릿은_검사_승격도_거부되고_그대로다() {
    let (_dir, path, store) = temp_store();
    let id = pending_with_stale_pass(&store, &path);
    let before = store.get_template(id).unwrap();

    let use_ = approved(&store, "sha256:promote-stale-checked");
    let result = store.promote_template_live_checked(id, before.revision, use_.clone(), t(60));

    assert!(result.is_err());
    assert_eq!(store.get_template(id).unwrap(), before);
    assert_eq!(
        store.get_approval(use_.id).unwrap().status,
        ApprovalStatus::Approved
    );
}

// 검증: E36 R14-1
#[test]
fn 직접_sql로도_통과_기록과_다른_리비전의_템플릿을_실전으로_옮길_수_없다() {
    let (_dir, path, store) = temp_store();
    let id = pending_with_stale_pass(&store, &path);
    let use_ = approved(&store, "sha256:promote-stale-raw");

    let result = raw(&path).execute(
        "UPDATE templates SET state = 'live', approval = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id.0, use_.id.0, t(60).0],
    );

    assert!(
        result.is_err(),
        "표 트리거가 통과 기록과 다른 리비전의 승격을 막아야 한다"
    );
    assert_eq!(
        store.get_template(id).unwrap().state,
        TemplateState::PromotionPending
    );
}

// 검증: E36 R14-1
#[test]
fn 통과_기록과_리비전이_같은_템플릿은_지금처럼_승격된다() {
    let (_dir, _path, store) = temp_store();
    let template = pending_template(&store);
    let use_ = approved(&store, "sha256:promote-same");
    let live = store
        .promote_template_live_checked(template.id, template.revision, use_, t(60))
        .unwrap();
    assert_eq!(live.state, TemplateState::Live);
    assert_eq!(live.revision, 1);
}

// ───────────── 3. 판 10 옮기기 뒤 무결성 ─────────────

// 검증: E36 R14-1
#[test]
fn 판_9_파일을_옮긴_뒤_무결성_검사가_ok이고_저장소가_그대로_열린다() {
    let (_dir, path) = copy_of_v9();
    let store = Store::open(&path).unwrap();
    drop(store);

    let check: String = raw(&path)
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(check, "ok");
    assert!(Store::open(&path).is_ok());
}
