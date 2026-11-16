//! 템플릿 상태 이름과 허용 전이 표(M3 · V29 · V30 · E36).

use pi_template::{ALLOWED_TRANSITIONS, TemplateState, TransitionReason};

const ALL: [TemplateState; 4] = [
    TemplateState::Paper,
    TemplateState::PendingPromotion,
    TemplateState::Live,
    TemplateState::Stopped,
];

// 검증: M3
#[test]
fn state_labels_are_the_spec_names() {
    assert_eq!(TemplateState::Paper.label(), "모의중");
    assert_eq!(TemplateState::PendingPromotion.label(), "승격대기");
    assert_eq!(TemplateState::Live.label(), "실전중");
    assert_eq!(TemplateState::Stopped.label(), "멈춤");
}

// 검증: M3, V29
#[test]
fn new_template_starts_in_paper() {
    assert_eq!(TemplateState::INITIAL, TemplateState::Paper);
}

// 검증: M3, V30
#[test]
fn transition_table_has_exactly_the_spec_edges() {
    let edges: Vec<_> = ALLOWED_TRANSITIONS
        .iter()
        .map(|t| (t.from, t.to, t.reason))
        .collect();
    assert_eq!(
        edges,
        vec![
            (
                TemplateState::Paper,
                TemplateState::PendingPromotion,
                TransitionReason::CriteriaPassed
            ),
            (
                TemplateState::PendingPromotion,
                TemplateState::Live,
                TransitionReason::Approved
            ),
            (
                TemplateState::PendingPromotion,
                TemplateState::Paper,
                TransitionReason::Rejected
            ),
            (
                TemplateState::Live,
                TemplateState::Stopped,
                TransitionReason::UserStopped
            ),
            (
                TemplateState::Live,
                TemplateState::Stopped,
                TransitionReason::GlobalHalt
            ),
        ]
    );
}

// 검증: M3, V30
#[test]
fn live_is_reachable_only_from_pending_by_approval() {
    for from in ALL {
        let reasons = from.reasons_to(TemplateState::Live);
        if from == TemplateState::PendingPromotion {
            assert_eq!(reasons, vec![TransitionReason::Approved]);
        } else {
            assert!(reasons.is_empty(), "{from:?} -> Live");
        }
    }
}

// 검증: M3
#[test]
fn reasons_follow_the_table() {
    assert_eq!(
        TemplateState::Paper.reasons_to(TemplateState::PendingPromotion),
        vec![TransitionReason::CriteriaPassed]
    );
    assert_eq!(
        TemplateState::PendingPromotion.reasons_to(TemplateState::Paper),
        vec![TransitionReason::Rejected]
    );
    assert_eq!(
        TemplateState::Live.reasons_to(TemplateState::Stopped),
        vec![TransitionReason::UserStopped, TransitionReason::GlobalHalt]
    );
}

// 검증: M3, V30
#[test]
fn edges_outside_the_table_have_no_reason() {
    let allowed: Vec<_> = ALLOWED_TRANSITIONS.iter().map(|t| (t.from, t.to)).collect();
    for from in ALL {
        for to in ALL {
            let reasons = from.reasons_to(to);
            assert_eq!(
                reasons.is_empty(),
                !allowed.contains(&(from, to)),
                "{from:?} -> {to:?}"
            );
        }
    }
    assert!(
        TemplateState::Stopped
            .reasons_to(TemplateState::Live)
            .is_empty()
    );
    assert!(
        TemplateState::Paper
            .reasons_to(TemplateState::Paper)
            .is_empty()
    );
}
