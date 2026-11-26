//! 막힌 까닭 글의 길이 상한: 부르는 쪽이 긴 글을 주어도 처리 기록에는 [`BLOCKED_REASON_MAX_CHARS`] 글자까지만
//! 남는다(글자 단위로 자른다 — 한글이 깨지지 않는다). 다시 켜도 같다. 짧은 글은 그대로다.

use pi_approval::{BLOCKED_REASON_MAX_CHARS, DueHandling, Notice};

use crate::common::{MIN, T0, buy, close_due_at, env, reopen, requested};

fn reason_of(handling: Option<DueHandling>) -> String {
    match handling {
        Some(DueHandling::Blocked { reason }) => reason,
        other => panic!("막힘 기록이어야 한다: {other:?}"),
    }
}

// 검증: E22 E38
#[test]
fn 긴_막힘_까닭은_상한_글자까지만_남고_다시_켜도_같다() {
    let e = env();
    let r = requested(&e, buy("long"), Some(Notice::Sent));
    close_due_at(&e, T0 + 5 * MIN);
    let long = "가".repeat(BLOCKED_REASON_MAX_CHARS * 3);
    e.approvals
        .record_due_handling(
            &r.number,
            DueHandling::Blocked {
                reason: long.clone(),
            },
        )
        .unwrap();

    let kept = reason_of(e.approvals.due_handling(&r.number).unwrap());
    assert_eq!(kept.chars().count(), BLOCKED_REASON_MAX_CHARS);
    assert!(long.starts_with(&kept), "앞에서부터 자른다");

    let again = reopen(&e.path, e.clock.clone());
    assert_eq!(reason_of(again.due_handling(&r.number).unwrap()), kept);
    assert!(again.unhandled_due().unwrap().is_empty());
}

// 검증: E22
#[test]
fn 상한_안의_막힘_까닭은_그대로_남는다() {
    let e = env();
    let r = requested(&e, buy("short"), Some(Notice::Sent));
    close_due_at(&e, T0 + 5 * MIN);
    let exact = "나".repeat(BLOCKED_REASON_MAX_CHARS);
    e.approvals
        .record_due_handling(
            &r.number,
            DueHandling::Blocked {
                reason: exact.clone(),
            },
        )
        .unwrap();
    assert_eq!(
        reason_of(e.approvals.due_handling(&r.number).unwrap()),
        exact
    );
}

// 검증: E22
#[test]
fn 막힘_까닭_상한은_200글자다() {
    assert_eq!(BLOCKED_REASON_MAX_CHARS, 200);
}
