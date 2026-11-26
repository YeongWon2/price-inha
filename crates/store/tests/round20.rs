#![cfg(test)]
//! 20차(결정 25): 사람이 내리는 전체 정지 — 사유 `OwnerStop`과 그 전이.
//!
//! - 쉬는 중 · 운용 중 · 하루 한도 멈춤 · 이상 감지 멈춤 → 전체 정지를 `OwnerStop` + 내 승인으로 받는다.
//! - 내 승인 표시가 없으면 승인 필요로 거부한다. 쉬는 중에서 `MaxLoss`는 여전히 안 된다.
//! - 전체 정지에 딸린 일(실전중 템플릿 멈춤 · 실전 승격 막음 · 나갈 때 내 승인)은 사유와 무관하게 같다.
//! - 사유 글자에 표의 제약이 없어 저장소 판은 그대로다.

mod common;

use common::{t, temp_store};
use pi_store::{
    ApprovalStatus, ApprovalUse, DenyReason, NewApprovalRequest, NewTemplate, SCHEMA_VERSION,
    Store, StoreError, TeamCause, TeamChange, TeamState, TeamStateRecord, Template, TemplateState,
    Transition,
};
use serde_json::json;

use TeamState::{AnomalyHalt, DailyLimitHalt, FullStop, Operating, Resting};

/// 전체 정지가 아닌 네 상태.
const NOT_STOPPED: [TeamState; 4] = [Resting, Operating, DailyLimitHalt, AnomalyHalt];

fn change(to: TeamState, cause: TeamCause, owner_approved: bool, at: i64) -> TeamChange {
    TeamChange {
        to,
        at: t(at),
        cause,
        detail: format!("{cause:?} 때문에 {to:?}로"),
        owner_approved,
    }
}

/// 저장소를 허용된 길로만 `state`까지 데려간다. 돌려주는 수는 남은 기록 줄 수다.
fn bring_to(store: &Store, state: TeamState) -> usize {
    let path: &[(TeamState, TeamCause)] = match state {
        Resting => &[],
        Operating => &[(Operating, TeamCause::TradingStart)],
        DailyLimitHalt => &[
            (Operating, TeamCause::TradingStart),
            (DailyLimitHalt, TeamCause::DailyLossLimit),
        ],
        AnomalyHalt => &[(AnomalyHalt, TeamCause::AnomalyDetected)],
        FullStop => &[
            (Operating, TeamCause::TradingStart),
            (FullStop, TeamCause::MaxLoss),
        ],
    };
    for (i, (to, cause)) in path.iter().enumerate() {
        store
            .change_team_state(change(*to, *cause, false, i as i64))
            .unwrap();
    }
    assert_eq!(store.team_state().unwrap(), state);
    path.len()
}

fn denied_reason(result: Result<TeamStateRecord, StoreError>, from: TeamState) -> DenyReason {
    match result {
        Err(StoreError::TransitionDenied { transition, reason }) => {
            assert_eq!(transition, Transition::Team { from, to: FullStop });
            reason
        }
        other => panic!("{from:?} → 전체 정지는 거부여야 한다: {other:?}"),
    }
}

/// 실전중까지 올린 템플릿(승인 지문은 `fp`).
fn live_template(store: &Store, fp: &str) -> Template {
    let made = store
        .create_template(NewTemplate {
            value: json!({ "스타일": "스윙", "R%": "1.0" }),
            at: t(0),
        })
        .unwrap();
    let pending = store
        .submit_template_for_promotion(made.id, &json!({ "매매": 31 }), t(10))
        .unwrap();
    let approval = approve(store, fp);
    store
        .promote_template_live(
            pending.id,
            ApprovalUse {
                id: approval,
                fingerprint: fp.to_string(),
            },
            t(20),
        )
        .unwrap()
}

fn approve(store: &Store, fp: &str) -> pi_store::ApprovalId {
    let made = store
        .create_approval(NewApprovalRequest {
            fingerprint: fp.to_string(),
            summary: json!({ "실전": true }),
            created_at: t(0),
            expires_at: t(100_000),
        })
        .unwrap();
    store
        .resolve_approval(made.id, ApprovalStatus::Approved, t(1))
        .unwrap()
        .id
}

// 검증: M1
#[test]
fn 사유에_내_정지가_있고_모든_사유_목록에_든다() {
    assert_eq!(TeamCause::ALL.len(), 10);
    assert!(TeamCause::ALL.contains(&TeamCause::OwnerStop));
    // 목록에 같은 사유가 두 번 들지 않는다.
    for (i, a) in TeamCause::ALL.iter().enumerate() {
        for b in &TeamCause::ALL[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

// 검증: M1
#[test]
fn 네_상태_각각에서_내_정지와_내_승인이면_전체_정지가_되고_기록에_사유가_남는다() {
    for from in NOT_STOPPED {
        let (_dir, _path, store) = temp_store();
        let steps = bring_to(&store, from);
        let rec = store
            .change_team_state(TeamChange {
                to: FullStop,
                at: t(500),
                cause: TeamCause::OwnerStop,
                detail: "내가 정지를 눌렀다".to_string(),
                owner_approved: true,
            })
            .unwrap_or_else(|e| panic!("{from:?} → 전체 정지(내 정지)는 되어야 한다: {e}"));
        let want = TeamStateRecord {
            seq: steps as u64 + 1,
            at: t(500),
            from,
            to: FullStop,
            cause: TeamCause::OwnerStop,
            detail: "내가 정지를 눌렀다".to_string(),
            owner_approved: true,
        };
        assert_eq!(rec, want);
        assert_eq!(store.team_state().unwrap(), FullStop);
        let history = store.team_history().unwrap();
        assert_eq!(history.len(), steps + 1);
        assert_eq!(history.last().unwrap(), &want);
    }
}

// 검증: M1
#[test]
fn 내_정지라도_내_승인_표시가_없으면_승인_필요로_거부되고_상태도_기록도_그대로다() {
    for from in NOT_STOPPED {
        let (_dir, _path, store) = temp_store();
        bring_to(&store, from);
        let before = store.team_history().unwrap();
        let result = store.change_team_state(change(FullStop, TeamCause::OwnerStop, false, 500));
        assert_eq!(
            denied_reason(result, from),
            DenyReason::OwnerApprovalRequired,
            "{from:?}"
        );
        assert_eq!(store.team_state().unwrap(), from);
        assert_eq!(store.team_history().unwrap(), before);
    }
}

// 검증: M1
#[test]
fn 쉬는_중에서_최대_손실로는_여전히_전체_정지가_되지_않는다() {
    for owner_approved in [false, true] {
        let (_dir, _path, store) = temp_store();
        let result =
            store.change_team_state(change(FullStop, TeamCause::MaxLoss, owner_approved, 5));
        assert_eq!(denied_reason(result, Resting), DenyReason::NotAllowed);
        assert_eq!(store.team_state().unwrap(), Resting);
        assert!(store.team_history().unwrap().is_empty());
    }
}

// 검증: M1
#[test]
fn 최대_손실과_내_정지_말고_다른_사유로는_어디서도_전체_정지가_되지_않는다() {
    for from in NOT_STOPPED {
        for cause in TeamCause::ALL {
            if matches!(cause, TeamCause::MaxLoss | TeamCause::OwnerStop) {
                continue;
            }
            for owner_approved in [false, true] {
                let (_dir, _path, store) = temp_store();
                bring_to(&store, from);
                let before = store.team_history().unwrap();
                let result = store.change_team_state(change(FullStop, cause, owner_approved, 500));
                let want = if from == Resting {
                    DenyReason::NotAllowed
                } else {
                    DenyReason::MaxLossOnly
                };
                assert_eq!(denied_reason(result, from), want, "{from:?} · {cause:?}");
                assert_eq!(store.team_state().unwrap(), from);
                assert_eq!(store.team_history().unwrap(), before);
            }
        }
    }
}

// 검증: M1
#[test]
fn 최대_손실로_가는_지금_길은_그대로다() {
    for from in [Operating, DailyLimitHalt, AnomalyHalt] {
        let (_dir, _path, store) = temp_store();
        bring_to(&store, from);
        let rec = store
            .change_team_state(change(FullStop, TeamCause::MaxLoss, false, 500))
            .unwrap_or_else(|e| panic!("{from:?} → 전체 정지(최대 손실)는 되어야 한다: {e}"));
        assert_eq!(
            (rec.from, rec.to, rec.cause),
            (from, FullStop, TeamCause::MaxLoss)
        );
    }
}

// 검증: M1
#[test]
fn 이미_전체_정지면_다시_정지는_거부된다() {
    for first in [TeamCause::MaxLoss, TeamCause::OwnerStop] {
        let (_dir, _path, store) = temp_store();
        bring_to(&store, Operating);
        store
            .change_team_state(change(FullStop, first, true, 100))
            .unwrap();
        let before = store.team_history().unwrap();
        for owner_approved in [false, true] {
            let result = store.change_team_state(change(
                FullStop,
                TeamCause::OwnerStop,
                owner_approved,
                200,
            ));
            assert_eq!(denied_reason(result, FullStop), DenyReason::NotAllowed);
        }
        assert_eq!(store.team_state().unwrap(), FullStop);
        assert_eq!(store.team_history().unwrap(), before);
    }
}

// 검증: M1
#[test]
fn 내_정지_뒤_다시_시작은_지금처럼_내_승인이_있어야_쉬는_중이_된다() {
    for from in NOT_STOPPED {
        let (_dir, _path, store) = temp_store();
        bring_to(&store, from);
        store
            .change_team_state(change(FullStop, TeamCause::OwnerStop, true, 100))
            .unwrap();

        match store.change_team_state(change(Resting, TeamCause::OwnerRestart, false, 200)) {
            Err(StoreError::TransitionDenied { reason, .. }) => {
                assert_eq!(reason, DenyReason::OwnerApprovalRequired);
            }
            other => panic!("승인 없이는 전체 정지에서 나가지 못한다: {other:?}"),
        }
        assert_eq!(store.team_state().unwrap(), FullStop);

        // 쉬는 중 말고 다른 상태로는 승인이 있어도 못 간다.
        for to in [Operating, DailyLimitHalt, AnomalyHalt] {
            assert!(
                store
                    .change_team_state(change(to, TeamCause::OwnerRestart, true, 250))
                    .is_err()
            );
        }

        let rec = store
            .change_team_state(change(Resting, TeamCause::OwnerRestart, true, 300))
            .unwrap();
        assert_eq!((rec.from, rec.to), (FullStop, Resting));
        assert!(rec.owner_approved);
        assert_eq!(store.team_state().unwrap(), Resting);
    }
}

// 검증: M3, M1
#[test]
fn 내_정지로_전체_정지가_되어도_실전중_템플릿만_멈춤이_된다() {
    for from in NOT_STOPPED {
        let (_dir, _path, store) = temp_store();
        let simulating = store
            .create_template(NewTemplate {
                value: json!({ "스타일": "단타" }),
                at: t(0),
            })
            .unwrap();
        let running = live_template(&store, "sha256:owner-stop-live");
        bring_to(&store, from);
        store
            .change_team_state(change(FullStop, TeamCause::OwnerStop, true, 700))
            .unwrap();
        let now = store.get_template(running.id).unwrap();
        assert_eq!(now.state, TemplateState::Stopped, "{from:?}");
        assert_eq!(now.updated_at, t(700));
        assert_eq!(store.get_template(simulating.id).unwrap(), simulating);
        assert!(
            store
                .templates_in_state(TemplateState::Live)
                .unwrap()
                .is_empty()
        );
    }
}

// 검증: M3, M1
#[test]
fn 승인_없는_내_정지는_템플릿을_바꾸지_않는다() {
    let (_dir, _path, store) = temp_store();
    let running = live_template(&store, "sha256:owner-stop-denied");
    bring_to(&store, Operating);
    assert!(
        store
            .change_team_state(change(FullStop, TeamCause::OwnerStop, false, 700))
            .is_err()
    );
    assert_eq!(store.get_template(running.id).unwrap(), running);
}

// 검증: M3, M1
#[test]
fn 내_정지로_전체_정지인_동안_실전_승격은_거부된다() {
    let (_dir, _path, store) = temp_store();
    let made = store
        .create_template(NewTemplate {
            value: json!({ "스타일": "스윙" }),
            at: t(0),
        })
        .unwrap();
    let pending = store
        .submit_template_for_promotion(made.id, &json!({ "매매": 31 }), t(10))
        .unwrap();
    let fp = "sha256:owner-stop-promote";
    let use_ = ApprovalUse {
        id: approve(&store, fp),
        fingerprint: fp.to_string(),
    };
    store
        .change_team_state(change(FullStop, TeamCause::OwnerStop, true, 15))
        .unwrap();
    let err = store
        .promote_template_live_checked(pending.id, pending.revision, use_.clone(), t(20))
        .unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::TransitionDenied {
                reason: DenyReason::TeamFullStop,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(store.get_template(pending.id).unwrap(), pending);
    assert_eq!(store.get_approval(use_.id).unwrap().consumed_at, None);
}

// 검증: M1
#[test]
fn 다시_열어도_내_정지_기록이_그대로_읽히고_판은_그대로다() {
    let (_dir, path, store) = temp_store();
    bring_to(&store, Operating);
    let rec = store
        .change_team_state(change(FullStop, TeamCause::OwnerStop, true, 100))
        .unwrap();
    let history = store.team_history().unwrap();
    drop(store);

    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.team_state().unwrap(), FullStop);
    assert_eq!(reopened.team_history().unwrap(), history);
    assert_eq!(reopened.team_history().unwrap().last().unwrap(), &rec);
    // 사유 글자에 표의 제약이 없어 판을 올리지 않았다(19차의 판 11 그대로).
    assert_eq!(SCHEMA_VERSION, 12);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 11);
}
