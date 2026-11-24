#![cfg(test)]
//! 15차(스키마 버전 10): 사용자 변경 개선안 · 사용자 변경 반영(잠금의 예외) · 사무실 외형.
//!
//! - 사용자 변경 개선안: 근거 매매 없이 제안됨으로 생기고, 늘 위험을 키우며, 출처 · 대상 템플릿이 남는다.
//! - 반영: 냉각중인 사용자 변경 개선안만, 한 쓰기 안에서 개선안 반영됨 + 템플릿 새 리비전. 실전중이어도 된다.
//! - `revise_template`의 잠금은 그대로다.
//! - 사무실 외형: 쓰고 읽고, 다시 열어도 같고, 백업에 따라가며, 옛 판(9) 파일을 열어도 비어 있다.

mod common;

use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{
    AccountKind, ApprovalId, ApprovalStatus, ApprovalUse, NewApprovalRequest, NewProposal,
    NewTemplate, NewTrade, NewUserProposal, Proposal, ProposalId, ProposalSource, ProposalState,
    SCHEMA_VERSION, Store, StoreError, Template, TemplateId, TemplateState, TradeId, Transition,
};
use serde_json::json;
use tempfile::TempDir;

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

fn live_template(store: &Store) -> Template {
    let made = simulating_template(store);
    store
        .submit_template_for_promotion(made.id, &json!({ "매매": 31 }), t(10))
        .unwrap();
    let use_ = approved(store, "sha256:template-live");
    store.promote_template_live(made.id, use_, t(20)).unwrap()
}

fn stopped_template(store: &Store) -> Template {
    let live = live_template(store);
    store.stop_template(live.id, t(25)).unwrap()
}

fn user_proposal(store: &Store, template: TemplateId, at: i64) -> Proposal {
    store
        .create_user_proposal(NewUserProposal {
            template,
            content: new_value(),
            at: t(at),
        })
        .unwrap()
}

/// 사용자 변경 개선안을 승인해 냉각중으로 만든다.
fn cooling_user_proposal(store: &Store, template: TemplateId) -> Proposal {
    let made = user_proposal(store, template, 30);
    let use_ = approved(store, "sha256:user-proposal");
    let cooling = store.approve_proposal(made.id, use_, t(31)).unwrap();
    assert_eq!(cooling.state, ProposalState::Cooling);
    cooling
}

fn trade(store: &Store, at: i64) -> TradeId {
    store
        .create_trade(NewTrade {
            symbol: "005930".to_string(),
            account: AccountKind::Paper,
            at: t(at),
        })
        .unwrap()
        .id
}

fn review_proposal(store: &Store, increases_risk: bool) -> Proposal {
    let a = trade(store, 1);
    let b = trade(store, 2);
    store
        .create_proposal(NewProposal {
            content: json!({ "고침": "한도" }),
            evidence: vec![a, b],
            increases_risk,
            at: t(3),
        })
        .unwrap()
}

fn revision_numbers(store: &Store, id: TemplateId) -> Vec<u32> {
    store
        .template_revisions(id)
        .unwrap()
        .iter()
        .map(|r| r.revision)
        .collect()
}

fn reopen(dir: &TempDir) -> (PathBuf, Store) {
    let path = dir.path().join("pi.sqlite");
    let store = Store::open(&path).unwrap();
    (path, store)
}

fn raw_user_version(path: &Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

fn copy_of_v9() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v9.db");
    let to = dir.path().join("pi.sqlite");
    std::fs::copy(from, &to).unwrap();
    (dir, to)
}

// ───────────── 사용자 변경 개선안 만들기 ─────────────

// 검증: R14-1 E28 V15
#[test]
fn 근거_매매_없이_사용자_변경_개선안이_제안됨으로_생기고_늘_위험을_키운다() {
    let (_dir, _path, store) = temp_store();
    let template = simulating_template(&store);

    let made = store
        .create_user_proposal(NewUserProposal {
            template: template.id,
            content: new_value(),
            at: t(30),
        })
        .unwrap();

    assert_eq!(made.state, ProposalState::Proposed);
    assert!(made.increases_risk);
    assert!(made.evidence.is_empty());
    assert_eq!(made.content, new_value());
    assert_eq!(made.approval, None);
    assert_eq!(made.created_at, t(30));
    assert_eq!(made.updated_at, t(30));
    assert_eq!(made.applied_at, None);
    assert_eq!(store.get_proposal(made.id).unwrap(), made);
    assert_eq!(
        store.proposals_in_state(ProposalState::Proposed).unwrap(),
        vec![made]
    );
}

// 검증: R14-1 E28
#[test]
fn 사용자_변경_개선안은_출처와_대상_템플릿이_남고_번호가_이어진다() {
    let (_dir, _path, store) = temp_store();
    let first = simulating_template(&store);
    let second = simulating_template(&store);

    let a = user_proposal(&store, first.id, 30);
    let b = user_proposal(&store, second.id, 31);

    assert_eq!(b.id, ProposalId(a.id.0 + 1));
    assert_eq!(
        store.proposal_source(a.id).unwrap(),
        ProposalSource::User { template: first.id }
    );
    assert_eq!(
        store.proposal_source(b.id).unwrap(),
        ProposalSource::User {
            template: second.id
        }
    );
}

// 검증: R14-1 E28
#[test]
fn 복기_개선안의_출처는_복기이고_없는_개선안의_출처는_오류다() {
    let (_dir, _path, store) = temp_store();
    let made = review_proposal(&store, true);
    assert_eq!(
        store.proposal_source(made.id).unwrap(),
        ProposalSource::Review
    );
    assert!(matches!(
        store.proposal_source(ProposalId(999)),
        Err(StoreError::ProposalNotFound(ProposalId(999)))
    ));
}

// 검증: R14-1 E28
#[test]
fn 없는_템플릿에_대한_사용자_변경_개선안은_거부되고_아무것도_만들지_않는다() {
    let (_dir, _path, store) = temp_store();
    let result = store.create_user_proposal(NewUserProposal {
        template: TemplateId(999),
        content: new_value(),
        at: t(30),
    });
    assert!(result.is_err());
    for state in ProposalState::ALL {
        assert!(store.proposals_in_state(state).unwrap().is_empty());
    }
}

// 검증: R14-1 E28
#[test]
fn 사용자_변경_개선안은_다시_열어도_출처와_내용이_그대로다() {
    let (dir, _path, store) = temp_store();
    let template = simulating_template(&store);
    let made = user_proposal(&store, template.id, 30);
    drop(store);

    let (_path, store) = reopen(&dir);
    assert_eq!(store.get_proposal(made.id).unwrap(), made);
    assert_eq!(
        store.proposal_source(made.id).unwrap(),
        ProposalSource::User {
            template: template.id
        }
    );
}

// 검증: R14-1 E28
#[test]
fn 사용자_변경_개선안도_거절할_수_있고_거절되면_템플릿은_그대로다() {
    let (_dir, _path, store) = temp_store();
    let template = live_template(&store);
    let made = user_proposal(&store, template.id, 30);
    let rejected = store.reject_proposal(made.id, t(31)).unwrap();
    assert_eq!(rejected.state, ProposalState::Rejected);
    assert_eq!(store.get_template(template.id).unwrap(), template);
}

// ───────────── 승인 → 냉각중 → 반영 ─────────────

// 검증: R14-1 E28 V15
#[test]
fn 승인하면_냉각중이_되고_템플릿은_아직_그대로다() {
    let (_dir, _path, store) = temp_store();
    let template = live_template(&store);
    let cooling = cooling_user_proposal(&store, template.id);
    assert_eq!(cooling.applied_at, None);
    assert!(cooling.approval.is_some());
    assert_eq!(store.get_template(template.id).unwrap(), template);
    assert_eq!(revision_numbers(&store, template.id), vec![1]);
}

// 검증: R14-1 E28 V15
#[test]
fn 냉각중_사용자_변경_개선안을_반영하면_실전중_템플릿에_새_리비전이_쓰이고_개선안은_반영됨이다() {
    let (_dir, _path, store) = temp_store();
    let template = live_template(&store);
    let cooling = cooling_user_proposal(&store, template.id);

    store
        .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
        .unwrap();

    let proposal = store.get_proposal(cooling.id).unwrap();
    assert_eq!(proposal.state, ProposalState::Applied);
    assert_eq!(proposal.applied_at, Some(t(50)));
    assert_eq!(proposal.updated_at, t(50));
    assert_eq!(proposal.approval, cooling.approval);

    let now = store.get_template(template.id).unwrap();
    assert_eq!(now.state, TemplateState::Live);
    assert_eq!(now.revision, 2);
    assert_eq!(now.value, new_value());
    assert_eq!(now.updated_at, t(50));
    assert_eq!(now.approval, template.approval);
    assert_eq!(now.criteria_pass, template.criteria_pass);

    let revisions = store.template_revisions(template.id).unwrap();
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].value, old_value());
    assert_eq!(revisions[1].value, new_value());
    assert_eq!(revisions[1].revision, 2);
    assert_eq!(revisions[1].at, t(50));
}

// 검증: R14-1 E28
#[test]
fn 반영한_뒤_다시_열어도_개선안과_템플릿이_같다() {
    let (dir, _path, store) = temp_store();
    let template = live_template(&store);
    let cooling = cooling_user_proposal(&store, template.id);
    store
        .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
        .unwrap();
    let proposal = store.get_proposal(cooling.id).unwrap();
    let now = store.get_template(template.id).unwrap();
    drop(store);

    let (_path, store) = reopen(&dir);
    assert_eq!(store.get_proposal(cooling.id).unwrap(), proposal);
    assert_eq!(store.get_template(template.id).unwrap(), now);
    assert_eq!(
        store.proposal_source(cooling.id).unwrap(),
        ProposalSource::User {
            template: template.id
        }
    );
}

// 검증: R14-1 E28
#[test]
fn 모의중_템플릿에도_반영된다() {
    let (_dir, _path, store) = temp_store();
    let template = simulating_template(&store);
    let cooling = cooling_user_proposal(&store, template.id);
    store
        .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
        .unwrap();
    let now = store.get_template(template.id).unwrap();
    assert_eq!(now.state, TemplateState::Simulating);
    assert_eq!(now.revision, 2);
    assert_eq!(now.value, new_value());
}

// 검증: R14-1 E28 V15
#[test]
fn 반영은_한_쓰기라서_템플릿_쪽이_실패하면_개선안도_냉각중으로_그대로다() {
    let (dir, path, store) = temp_store();
    let template = live_template(&store);
    let cooling = cooling_user_proposal(&store, template.id);
    drop(store);

    // 새 리비전 줄을 쓰는 순간 실패하게 만든다. 개선안만 먼저 반영됨이 되는 틈이 있으면 여기서 드러난다.
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "CREATE TRIGGER fail_new_revision BEFORE INSERT ON template_revisions
         WHEN NEW.revision = 2
         BEGIN SELECT RAISE(ABORT, 'test: revision write fails'); END;",
    )
    .unwrap();
    drop(raw);

    let (_path, store) = reopen(&dir);
    assert!(
        store
            .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
            .is_err()
    );
    assert_eq!(store.get_proposal(cooling.id).unwrap(), cooling);
    assert_eq!(store.get_template(template.id).unwrap(), template);
    assert_eq!(revision_numbers(&store, template.id), vec![1]);
}

// ───────────── 거부되는 경우 ─────────────

// 검증: R14-1 E28 V15
#[test]
fn 멈춤_템플릿이면_반영을_거부하고_개선안도_템플릿도_그대로다() {
    let (_dir, _path, store) = temp_store();
    let live = live_template(&store);
    let cooling = cooling_user_proposal(&store, live.id);
    let stopped = store.stop_template(live.id, t(40)).unwrap();
    assert_eq!(stopped.state, TemplateState::Stopped);

    assert!(
        store
            .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
            .is_err()
    );
    assert_eq!(store.get_proposal(cooling.id).unwrap(), cooling);
    assert_eq!(store.get_template(live.id).unwrap(), stopped);
    assert_eq!(revision_numbers(&store, live.id), vec![1]);
}

// 검증: R14-1 E28
#[test]
fn 처음부터_멈춤인_템플릿_대상_개선안도_반영되지_않는다() {
    let (_dir, _path, store) = temp_store();
    let stopped = stopped_template(&store);
    let made = user_proposal(&store, stopped.id, 30);
    let use_ = approved(&store, "sha256:user-proposal");
    let cooling = store.approve_proposal(made.id, use_, t(31)).unwrap();

    assert!(
        store
            .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
            .is_err()
    );
    assert_eq!(
        store.get_proposal(cooling.id).unwrap().state,
        ProposalState::Cooling
    );
    assert_eq!(revision_numbers(&store, stopped.id), vec![1]);
}

// 검증: R14-1 E28
#[test]
fn 복기_개선안은_냉각중이어도_이_길로_반영할_수_없고_그대로다() {
    let (_dir, _path, store) = temp_store();
    let template = live_template(&store);
    let review = review_proposal(&store, true);
    let use_ = approved(&store, "sha256:review-proposal");
    let cooling = store.approve_proposal(review.id, use_, t(31)).unwrap();
    assert_eq!(cooling.state, ProposalState::Cooling);

    assert!(
        store
            .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
            .is_err()
    );
    assert_eq!(store.get_proposal(cooling.id).unwrap(), cooling);
    assert_eq!(store.get_template(template.id).unwrap(), template);
    assert_eq!(revision_numbers(&store, template.id), vec![1]);

    // 복기 개선안의 원래 길(apply_cooled_proposal)은 그대로 된다.
    let applied = store.apply_cooled_proposal(cooling.id, t(60)).unwrap();
    assert_eq!(applied.state, ProposalState::Applied);
}

// 검증: R14-1 E28
#[test]
fn 냉각중이_아닌_개선안은_반영할_수_없다() {
    let (_dir, _path, store) = temp_store();
    let template = live_template(&store);

    // 제안됨
    let proposed = user_proposal(&store, template.id, 30);
    let err = store
        .apply_cooled_user_proposal(proposed.id, &new_value(), t(50))
        .unwrap_err();
    assert!(matches!(
        err,
        StoreError::TransitionDenied {
            transition: Transition::Proposal { .. },
            ..
        }
    ));
    assert_eq!(store.get_proposal(proposed.id).unwrap(), proposed);

    // 거절됨
    let rejected = store.reject_proposal(proposed.id, t(51)).unwrap();
    assert!(
        store
            .apply_cooled_user_proposal(rejected.id, &new_value(), t(52))
            .is_err()
    );
    assert_eq!(store.get_proposal(rejected.id).unwrap(), rejected);

    // 없는 개선안
    assert!(matches!(
        store.apply_cooled_user_proposal(ProposalId(999), &new_value(), t(53)),
        Err(StoreError::ProposalNotFound(ProposalId(999)))
    ));

    assert_eq!(store.get_template(template.id).unwrap(), template);
    assert_eq!(revision_numbers(&store, template.id), vec![1]);
}

// 검증: R14-1 E28
#[test]
fn 이미_반영된_개선안을_또_반영하면_거부되고_리비전이_더_늘지_않는다() {
    let (_dir, _path, store) = temp_store();
    let template = live_template(&store);
    let cooling = cooling_user_proposal(&store, template.id);
    store
        .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
        .unwrap();
    let applied = store.get_proposal(cooling.id).unwrap();

    let err = store
        .apply_cooled_user_proposal(cooling.id, &json!({ "다른": "값" }), t(60))
        .unwrap_err();
    assert!(matches!(err, StoreError::TransitionDenied { .. }));
    assert_eq!(store.get_proposal(cooling.id).unwrap(), applied);
    assert_eq!(revision_numbers(&store, template.id), vec![1, 2]);
}

// ───────────── 잠금은 그대로 ─────────────

// 검증: R14-1 E28 V29
#[test]
fn 예외는_이_길뿐이라서_revise_template의_잠금은_실전중_멈춤_승격대기에서_그대로다() {
    let (_dir, _path, store) = temp_store();

    let live = live_template(&store);
    assert!(matches!(
        store.revise_template(live.id, &new_value(), t(70)),
        Err(StoreError::TemplateLocked {
            state: TemplateState::Live
        })
    ));

    let pending = simulating_template(&store);
    store
        .submit_template_for_promotion(pending.id, &json!({ "매매": 31 }), t(71))
        .unwrap();
    assert!(matches!(
        store.revise_template(pending.id, &new_value(), t(72)),
        Err(StoreError::TemplateLocked {
            state: TemplateState::PromotionPending
        })
    ));

    let stopped = stopped_template(&store);
    assert!(matches!(
        store.revise_template(stopped.id, &new_value(), t(73)),
        Err(StoreError::TemplateLocked {
            state: TemplateState::Stopped
        })
    ));

    assert_eq!(revision_numbers(&store, live.id), vec![1]);
    assert_eq!(revision_numbers(&store, pending.id), vec![1]);
    assert_eq!(revision_numbers(&store, stopped.id), vec![1]);
}

// 검증: R14-1 E28 V29
#[test]
fn 사용자_변경이_반영된_뒤에도_실전중_템플릿은_revise_template이_잠겨_있다() {
    let (_dir, _path, store) = temp_store();
    let template = live_template(&store);
    let cooling = cooling_user_proposal(&store, template.id);
    store
        .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
        .unwrap();

    assert!(matches!(
        store.revise_template(template.id, &json!({ "또": "다른" }), t(60)),
        Err(StoreError::TemplateLocked {
            state: TemplateState::Live
        })
    ));
    assert_eq!(revision_numbers(&store, template.id), vec![1, 2]);
}

// 검증: R14-1 E28
#[test]
fn 모의중_템플릿은_revise_template이_여전히_된다() {
    let (_dir, _path, store) = temp_store();
    let template = simulating_template(&store);
    let revised = store
        .revise_template(template.id, &new_value(), t(5))
        .unwrap();
    assert_eq!(revised.revision, 2);
}

// ───────────── 사무실 외형 ─────────────

// 검증: R14-1 V15
#[test]
fn 사무실_외형은_처음에_비어_있고_쓰면_읽힌다() {
    let (_dir, _path, store) = temp_store();
    assert!(store.office_looks().unwrap().is_empty());

    store.set_office_look("시세 담당", "look-a", t(1)).unwrap();
    store.set_office_look("매매 담당", "look-b", t(2)).unwrap();

    let looks = store.office_looks().unwrap();
    assert_eq!(looks.len(), 2);
    assert_eq!(looks.get("시세 담당").map(String::as_str), Some("look-a"));
    assert_eq!(looks.get("매매 담당").map(String::as_str), Some("look-b"));
}

// 검증: R14-1 V15
#[test]
fn 같은_담당의_외형을_다시_정하면_바뀌고_다른_담당은_그대로다() {
    let (_dir, _path, store) = temp_store();
    store.set_office_look("시세 담당", "look-a", t(1)).unwrap();
    store.set_office_look("매매 담당", "look-b", t(2)).unwrap();
    store.set_office_look("시세 담당", "look-c", t(3)).unwrap();

    let looks = store.office_looks().unwrap();
    assert_eq!(looks.len(), 2);
    assert_eq!(looks["시세 담당"], "look-c");
    assert_eq!(looks["매매 담당"], "look-b");
}

// 검증: R14-1 V15
#[test]
fn 사무실_외형은_다시_열어도_같다() {
    let (dir, _path, store) = temp_store();
    store.set_office_look("시세 담당", "look-a", t(1)).unwrap();
    store.set_office_look("매매 담당", "look-b", t(2)).unwrap();
    let before = store.office_looks().unwrap();
    assert_eq!(before.len(), 2);
    drop(store);

    let (_path, store) = reopen(&dir);
    assert_eq!(store.office_looks().unwrap(), before);
}

// 검증: R14-1 V15
#[test]
fn 담당이나_외형이_빈_글자면_거부하고_아무것도_바꾸지_않는다() {
    let (_dir, _path, store) = temp_store();
    store.set_office_look("시세 담당", "look-a", t(1)).unwrap();
    let before = store.office_looks().unwrap();
    assert_eq!(before.len(), 1);

    assert!(store.set_office_look("", "look-a", t(2)).is_err());
    assert!(store.set_office_look("시세 담당", "", t(3)).is_err());
    assert!(store.set_office_look("새 담당", "", t(4)).is_err());
    assert_eq!(store.office_looks().unwrap(), before);
}

// 검증: R14-1 V15
#[test]
fn 사무실_외형은_저장소_스냅샷에_들어가서_백업_되살리기에_그대로_따라간다() {
    let (dir, _path, store) = temp_store();
    store.set_office_look("시세 담당", "look-a", t(1)).unwrap();
    store.set_office_look("매매 담당", "look-b", t(2)).unwrap();
    let before = store.office_looks().unwrap();
    assert_eq!(before.len(), 2);

    let copy = dir.path().join("backup.sqlite");
    store.snapshot_to(&copy).unwrap();
    // 백업 뒤에 바꾼 것은 백업에 없다.
    store.set_office_look("시세 담당", "look-z", t(3)).unwrap();

    let restored = Store::open(&copy).unwrap();
    assert_eq!(restored.office_looks().unwrap(), before);
    assert_eq!(store.office_looks().unwrap()["시세 담당"], "look-z");
}

// ───────────── 스키마 판 · 옛 판 열기 ─────────────

// 검증: D6 V36
#[test]
fn 스키마_버전은_10이고_새_저장소는_10으로_만들어진다() {
    assert_eq!(SCHEMA_VERSION, 10);
    let (_dir, path, store) = temp_store();
    assert_eq!(store.info().unwrap().schema_version, 10);
    drop(store);
    assert_eq!(raw_user_version(&path), 10);
}

// 검증: D6 V36 M4
#[test]
fn 옛_판_9_파일을_열면_10으로_올라가고_개선안은_복기_출처이고_외형은_비어_있다() {
    let (_dir, path) = copy_of_v9();
    assert_eq!(raw_user_version(&path), 9);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 10);

    // 있던 개선안은 그대로 읽히고 출처는 복기다.
    let proposals = store.proposals_in_state(ProposalState::Proposed).unwrap();
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].content, json!({ "고침": "손절 좁히기" }));
    assert_eq!(proposals[0].evidence.len(), 2);
    assert!(!proposals[0].increases_risk);
    assert_eq!(
        store.proposal_source(proposals[0].id).unwrap(),
        ProposalSource::Review
    );
    assert!(store.office_looks().unwrap().is_empty());

    // 올라간 뒤 새 길이 바로 된다.
    store.set_office_look("시세 담당", "look-a", t(1)).unwrap();
    let template = store
        .templates_in_state(TemplateState::Simulating)
        .unwrap()
        .remove(0);
    let made = user_proposal(&store, template.id, 40);
    assert_eq!(
        store.proposal_source(made.id).unwrap(),
        ProposalSource::User {
            template: template.id
        }
    );
    drop(store);

    // 다시 열어도 같다.
    assert_eq!(raw_user_version(&path), 10);
    let again = Store::open(&path).unwrap();
    assert_eq!(again.office_looks().unwrap().len(), 1);
    assert_eq!(
        again.proposal_source(proposals[0].id).unwrap(),
        ProposalSource::Review
    );
}
