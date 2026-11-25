#![cfg(test)]
//! 투자 팀 상태 (M1): 처음 상태, 허용 표 전체, 전체 정지 · 하루 한도 멈춤의 조건, 기록.
//! 2차: 멈춤 → 전체 정지 두 길, 전체 정지로 가는 사유는 최대 손실뿐.

mod common;

use common::{t, temp_store};
use pi_store::{
    DenyReason, Store, StoreError, TeamCause, TeamChange, TeamState, TeamStateRecord, Transition,
};

use TeamState::{AnomalyHalt, DailyLimitHalt, FullStop, Operating, Resting};

/// 허용되는 전이와 그 전이의 자연스러운 사유. `spec.md`의 흐름(F1 · F2 · F3 · F4)과
/// `dev.md` C8에 글로 적힌 것, 그리고 지시서 2차의 "더하는 전이" 둘(멈춤 → 전체 정지)을 넣었다.
/// 이 밖의 14쌍(같은 상태 5쌍 포함)은 모두 거부다.
const ALLOWED: [(TeamState, TeamState, TeamCause); 11] = [
    (Resting, Operating, TeamCause::TradingStart),
    (Resting, AnomalyHalt, TeamCause::AnomalyDetected),
    (Operating, Resting, TeamCause::MarketClose),
    (Operating, DailyLimitHalt, TeamCause::DailyLossLimit),
    (Operating, AnomalyHalt, TeamCause::AnomalyDetected),
    (Operating, FullStop, TeamCause::MaxLoss),
    (DailyLimitHalt, Resting, TeamCause::NextTradingDayStart),
    (DailyLimitHalt, FullStop, TeamCause::MaxLoss),
    (AnomalyHalt, Resting, TeamCause::AnomalyCleared),
    (AnomalyHalt, FullStop, TeamCause::MaxLoss),
    (FullStop, Resting, TeamCause::OwnerRestart),
];

const STATES: [TeamState; 5] = [Operating, Resting, DailyLimitHalt, AnomalyHalt, FullStop];

fn change(to: TeamState, cause: TeamCause, owner_approved: bool, at: i64) -> TeamChange {
    TeamChange {
        to,
        at: t(at),
        cause,
        detail: format!("{cause:?} 때문에 {to:?}로"),
        owner_approved,
    }
}

/// 새 저장소를 허용된 길로만 `state`까지 데려간다.
fn store_in(state: TeamState) -> (tempfile::TempDir, Store) {
    let (dir, _path, store) = temp_store();
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
    (dir, store)
}

fn assert_denied(result: Result<TeamStateRecord, StoreError>, from: TeamState, to: TeamState) {
    match result {
        Err(StoreError::TransitionDenied { transition, .. }) => {
            assert_eq!(transition, Transition::Team { from, to });
        }
        other => panic!("{from:?} → {to:?} 는 전이 거부여야 한다: {other:?}"),
    }
}

// 검증: M1
#[test]
fn 상태는_다섯_개다() {
    assert_eq!(TeamState::ALL.len(), 5);
    for s in STATES {
        assert!(TeamState::ALL.contains(&s));
    }
}

// 검증: M1
#[test]
fn 처음_상태는_쉬는_중이고_기록은_비어_있다() {
    let (_dir, _path, store) = temp_store();
    assert_eq!(store.team_state().unwrap(), Resting);
    assert!(store.team_history().unwrap().is_empty());
}

// 검증: M1
#[test]
fn 허용_표에_있는_전이는_모두_된다() {
    for (from, to, cause) in ALLOWED {
        let (_dir, store) = store_in(from);
        let before = store.team_history().unwrap().len();
        // 전체 정지에서 나갈 때만 내 승인 표시를 붙인다.
        let rec = store
            .change_team_state(change(to, cause, from == FullStop, 100))
            .unwrap_or_else(|e| panic!("{from:?} → {to:?} 는 되어야 한다: {e}"));
        assert_eq!((rec.from, rec.to), (from, to));
        assert_eq!(store.team_state().unwrap(), to);
        assert_eq!(store.team_history().unwrap().len(), before + 1);
    }
}

// 검증: M1
#[test]
fn 허용_표에_없는_전이는_어떤_사유와_승인_표시로도_거부된다() {
    let mut checked = 0;
    for from in STATES {
        for to in STATES {
            if ALLOWED.iter().any(|(f, x, _)| (*f, *x) == (from, to)) {
                continue;
            }
            let (_dir, store) = store_in(from);
            let before = store.team_history().unwrap();
            for cause in TeamCause::ALL {
                for owner_approved in [false, true] {
                    // 20차: 쉬는 중 → 전체 정지는 내 정지 + 내 승인으로만 된다(최대 손실로는 안 된다).
                    if (from, to, cause, owner_approved)
                        == (Resting, FullStop, TeamCause::OwnerStop, true)
                    {
                        continue;
                    }
                    let result = store.change_team_state(change(to, cause, owner_approved, 100));
                    assert_denied(result, from, to);
                }
            }
            // 거부는 상태도 기록도 바꾸지 않는다.
            assert_eq!(store.team_state().unwrap(), from);
            assert_eq!(store.team_history().unwrap(), before);
            checked += 1;
        }
    }
    assert_eq!(checked, 25 - ALLOWED.len());
}

// 검증: M1
#[test]
fn 같은_상태로_가는_것은_거부된다() {
    for state in STATES {
        let (_dir, store) = store_in(state);
        let result = store.change_team_state(change(state, TeamCause::OwnerRestart, true, 100));
        match result {
            Err(StoreError::TransitionDenied { reason, .. }) => {
                assert_eq!(reason, DenyReason::NotAllowed);
            }
            other => panic!("{state:?} → {state:?} 는 거부여야 한다: {other:?}"),
        }
    }
}

// 검증: M1
#[test]
fn 전체_정지는_내_승인_표시_없이는_어떤_사유로도_나가지_못한다() {
    for to in STATES {
        let (_dir, store) = store_in(FullStop);
        for cause in TeamCause::ALL {
            let result = store.change_team_state(change(to, cause, false, 100));
            assert_denied(result, FullStop, to);
        }
        assert_eq!(store.team_state().unwrap(), FullStop);
        assert_eq!(store.team_history().unwrap().len(), 2);
    }
}

// 검증: M1
#[test]
fn 전체_정지에서_승인_없는_다시_시작은_승인_필요로_구분된다() {
    let (_dir, store) = store_in(FullStop);
    let result = store.change_team_state(change(Resting, TeamCause::OwnerRestart, false, 100));
    match result {
        Err(StoreError::TransitionDenied { transition, reason }) => {
            assert_eq!(
                transition,
                Transition::Team {
                    from: FullStop,
                    to: Resting
                }
            );
            assert_eq!(reason, DenyReason::OwnerApprovalRequired);
        }
        other => panic!("승인 필요로 거부여야 한다: {other:?}"),
    }
}

// 검증: M1
#[test]
fn 전체_정지는_내_승인_표시가_있으면_쉬는_중으로_나가고_표시가_기록에_남는다() {
    let (_dir, store) = store_in(FullStop);
    let rec = store
        .change_team_state(change(Resting, TeamCause::OwnerRestart, true, 100))
        .unwrap();
    assert!(rec.owner_approved);
    assert_eq!(store.team_state().unwrap(), Resting);
    let last = store.team_history().unwrap().pop().unwrap();
    assert_eq!(last, rec);
    assert!(last.owner_approved);
}

// 검증: M1
#[test]
fn 전체_정지는_승인이_있어도_쉬는_중_말고는_가지_못한다() {
    for to in [Operating, DailyLimitHalt, AnomalyHalt, FullStop] {
        let (_dir, store) = store_in(FullStop);
        let result = store.change_team_state(change(to, TeamCause::OwnerRestart, true, 100));
        assert_denied(result, FullStop, to);
        assert_eq!(store.team_state().unwrap(), FullStop);
    }
}

// 검증: M1
#[test]
fn 하루_한도_멈춤은_다음_거래일_시작_사유로만_풀린다() {
    for cause in TeamCause::ALL {
        if cause == TeamCause::NextTradingDayStart {
            continue;
        }
        for owner_approved in [false, true] {
            let (_dir, store) = store_in(DailyLimitHalt);
            let result = store.change_team_state(change(Resting, cause, owner_approved, 100));
            match result {
                Err(StoreError::TransitionDenied { transition, reason }) => {
                    assert_eq!(
                        transition,
                        Transition::Team {
                            from: DailyLimitHalt,
                            to: Resting
                        }
                    );
                    assert_eq!(reason, DenyReason::NextTradingDayOnly);
                }
                other => panic!("{cause:?} 사유로는 풀리지 않아야 한다: {other:?}"),
            }
            assert_eq!(store.team_state().unwrap(), DailyLimitHalt);
        }
    }

    let (_dir, store) = store_in(DailyLimitHalt);
    store
        .change_team_state(change(Resting, TeamCause::NextTradingDayStart, false, 100))
        .unwrap();
    assert_eq!(store.team_state().unwrap(), Resting);
}

// 검증: M1
#[test]
fn 바뀔_때마다_언제_무엇에서_무엇으로_왜가_순서대로_남는다() {
    let (_dir, _path, store) = temp_store();
    let steps = [
        (
            Operating,
            TeamCause::TradingStart,
            "오늘은 매매한다",
            false,
            10,
        ),
        (
            DailyLimitHalt,
            TeamCause::DailyLossLimit,
            "하루 손실 한도 도달",
            false,
            20,
        ),
        (
            Resting,
            TeamCause::NextTradingDayStart,
            "다음 거래일",
            false,
            30,
        ),
        (
            Operating,
            TeamCause::TradingStart,
            "다시 매매한다",
            false,
            40,
        ),
        (FullStop, TeamCause::MaxLoss, "최대 손실 도달", false, 50),
        (Resting, TeamCause::OwnerRestart, "내가 확인했다", true, 60),
    ];
    let mut from = Resting;
    let mut expected = Vec::new();
    for (i, (to, cause, detail, owner_approved, at)) in steps.into_iter().enumerate() {
        let rec = store
            .change_team_state(TeamChange {
                to,
                at: t(at),
                cause,
                detail: detail.to_string(),
                owner_approved,
            })
            .unwrap();
        let want = TeamStateRecord {
            seq: i as u64 + 1,
            at: t(at),
            from,
            to,
            cause,
            detail: detail.to_string(),
            owner_approved,
        };
        assert_eq!(rec, want);
        expected.push(want);
        from = to;
    }
    assert_eq!(store.team_history().unwrap(), expected);
}

// 검증: M1
#[test]
fn 거부된_시도는_기록을_남기지_않고_앞선_기록을_바꾸지_않는다() {
    let (_dir, _path, store) = temp_store();
    store
        .change_team_state(change(Operating, TeamCause::TradingStart, false, 1))
        .unwrap();
    let before = store.team_history().unwrap();
    assert_eq!(before.len(), 1);

    // 운용 중 → 운용 중, 그리고 전체 정지로 간 뒤 승인 없는 다시 시작.
    assert!(
        store
            .change_team_state(change(Operating, TeamCause::TradingStart, false, 2))
            .is_err()
    );
    assert_eq!(store.team_history().unwrap(), before);

    store
        .change_team_state(change(FullStop, TeamCause::MaxLoss, false, 3))
        .unwrap();
    assert!(
        store
            .change_team_state(change(Resting, TeamCause::OwnerRestart, false, 4))
            .is_err()
    );
    let after = store.team_history().unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1].seq, 2);
}

// ───────────── 2차 ─────────────

// 검증: M1
#[test]
fn 전체_정지로_가는_사유는_어디서든_최대_손실뿐이다() {
    for from in [Operating, DailyLimitHalt, AnomalyHalt] {
        for cause in TeamCause::ALL {
            // 20차: 내 정지(OwnerStop)도 전체 정지로 간다 — 그 사유는 round20.rs에서 본다.
            if cause == TeamCause::MaxLoss || cause == TeamCause::OwnerStop {
                continue;
            }
            for owner_approved in [false, true] {
                let (_dir, store) = store_in(from);
                let before = store.team_history().unwrap();
                let result = store.change_team_state(change(FullStop, cause, owner_approved, 100));
                match result {
                    Err(StoreError::TransitionDenied { transition, reason }) => {
                        assert_eq!(transition, Transition::Team { from, to: FullStop });
                        assert_eq!(
                            reason,
                            DenyReason::MaxLossOnly,
                            "{from:?} → 전체 정지, 사유 {cause:?}"
                        );
                    }
                    other => panic!(
                        "{from:?}에서 {cause:?} 사유로는 전체 정지가 되지 않아야 한다: {other:?}"
                    ),
                }
                // 거부는 상태도 기록도 바꾸지 않는다.
                assert_eq!(store.team_state().unwrap(), from);
                assert_eq!(store.team_history().unwrap(), before);
            }
        }

        // 최대 손실 사유면 된다. 내 승인 표시는 필요 없다.
        let (_dir, store) = store_in(from);
        let rec = store
            .change_team_state(change(FullStop, TeamCause::MaxLoss, false, 100))
            .unwrap_or_else(|e| panic!("{from:?} → 전체 정지(최대 손실)는 되어야 한다: {e}"));
        assert_eq!(
            (rec.from, rec.to, rec.cause),
            (from, FullStop, TeamCause::MaxLoss)
        );
        assert_eq!(store.team_state().unwrap(), FullStop);
    }
}

// 검증: M1
#[test]
fn 멈춘_동안_최대_손실에_닿으면_전체_정지가_되고_기록에_남는다() {
    for (from, steps_before) in [(DailyLimitHalt, 2), (AnomalyHalt, 1)] {
        let (_dir, store) = store_in(from);
        let rec = store
            .change_team_state(TeamChange {
                to: FullStop,
                at: t(500),
                cause: TeamCause::MaxLoss,
                detail: "멈춘 동안 최대 손실 도달".to_string(),
                owner_approved: false,
            })
            .unwrap_or_else(|e| panic!("{from:?} → 전체 정지는 되어야 한다: {e}"));
        let want = TeamStateRecord {
            seq: steps_before + 1,
            at: t(500),
            from,
            to: FullStop,
            cause: TeamCause::MaxLoss,
            detail: "멈춘 동안 최대 손실 도달".to_string(),
            owner_approved: false,
        };
        assert_eq!(rec, want);
        assert_eq!(store.team_history().unwrap().pop().unwrap(), want);
        assert_eq!(store.team_state().unwrap(), FullStop);
    }
}

// 검증: M1
#[test]
fn 멈춤에서_간_전체_정지도_내_승인이_있어야만_나간다() {
    for from in [DailyLimitHalt, AnomalyHalt] {
        let (_dir, store) = store_in(from);
        store
            .change_team_state(change(FullStop, TeamCause::MaxLoss, false, 100))
            .unwrap_or_else(|e| panic!("{from:?} → 전체 정지는 되어야 한다: {e}"));
        let len = store.team_history().unwrap().len();

        // 하루 한도 멈춤을 푸는 사유(다음 거래일 시작)로도, 다른 어떤 사유로도 승인 없이는 못 나간다.
        for cause in TeamCause::ALL {
            let result = store.change_team_state(change(Resting, cause, false, 200));
            match result {
                Err(StoreError::TransitionDenied { reason, .. }) => {
                    assert_eq!(reason, DenyReason::OwnerApprovalRequired);
                }
                other => panic!("승인 없이는 전체 정지에서 나가지 못한다: {other:?}"),
            }
        }
        assert_eq!(store.team_state().unwrap(), FullStop);
        assert_eq!(store.team_history().unwrap().len(), len);

        // 멈춤으로 되돌아가지도 못한다.
        assert_denied(
            store.change_team_state(change(from, TeamCause::OwnerRestart, true, 300)),
            FullStop,
            from,
        );

        let rec = store
            .change_team_state(change(Resting, TeamCause::OwnerRestart, true, 400))
            .unwrap();
        assert!(rec.owner_approved);
        assert_eq!(store.team_state().unwrap(), Resting);
    }
}

// 검증: M1
#[test]
fn 하루_한도_멈춤은_최대_손실로_전체_정지가_되는_것_말고는_다음_거래일까지_그대로다() {
    // 2차로 전체 정지 길이 생겼어도, 쉬는 중으로 푸는 규칙은 느슨해지지 않는다.
    let (_dir, store) = store_in(DailyLimitHalt);
    match store.change_team_state(change(Resting, TeamCause::MaxLoss, true, 100)) {
        Err(StoreError::TransitionDenied { reason, .. }) => {
            assert_eq!(reason, DenyReason::NextTradingDayOnly);
        }
        other => panic!("최대 손실 사유로 쉬는 중이 되지는 않는다: {other:?}"),
    }
    for to in [Operating, AnomalyHalt] {
        assert_denied(
            store.change_team_state(change(to, TeamCause::MaxLoss, true, 100)),
            DailyLimitHalt,
            to,
        );
    }
    assert_eq!(store.team_state().unwrap(), DailyLimitHalt);
    assert_eq!(store.team_history().unwrap().len(), 2);

    store
        .change_team_state(change(FullStop, TeamCause::MaxLoss, false, 200))
        .unwrap();
    assert_eq!(store.team_state().unwrap(), FullStop);
}
