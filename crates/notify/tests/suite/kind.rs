//! 알림 종류 여섯 가지(E2 · V10 · V11 · V12).

use std::collections::BTreeSet;

use pi_notify::{NoticeKind, Urgency};
use pi_store::Store;

// 검증: N5 E2 V10 V11 V12
#[test]
fn 종류는_여섯_가지이고_이름이_정해진_대로다() {
    let names: Vec<&str> = NoticeKind::ALL.iter().map(|k| k.name()).collect();
    assert_eq!(
        names,
        [
            "approval_request",
            "stall",
            "anomaly",
            "access_lock",
            "backup_failure",
            "daily_summary"
        ]
    );
    let unique: BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(unique.len(), 6);
}

// 검증: N5
#[test]
fn 이름으로_종류를_다시_찾고_모르는_이름은_없다() {
    for kind in NoticeKind::ALL {
        assert_eq!(NoticeKind::from_name(kind.name()), Some(kind));
    }
    assert_eq!(NoticeKind::from_name("nope"), None);
    assert_eq!(NoticeKind::from_name(""), None);
    assert_eq!(NoticeKind::from_name("Approval_Request"), None);
}

// 검증: N5 E2
#[test]
fn 이벤트_종류는_notice_점_이름이고_저장소가_받아_준다() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("pi.sqlite")).unwrap();
    for kind in NoticeKind::ALL {
        assert_eq!(kind.event_kind(), format!("notice.{}", kind.name()));
        let seq = store
            .append(kind.event_kind(), &serde_json::json!({}))
            .unwrap();
        assert!(seq > 0);
    }
}

// 검증: V10 V11
#[test]
fn 승인_요청과_이상은_조용한_시간을_무시한다() {
    assert!(NoticeKind::ApprovalRequest.ignores_quiet_hours());
    assert!(NoticeKind::Anomaly.ignores_quiet_hours());
    assert!(!NoticeKind::BackupFailure.ignores_quiet_hours());
    assert!(!NoticeKind::DailySummary.ignores_quiet_hours());
}

// 검증: N5 V10 V11 E2
#[test]
fn 급한_알림은_높은_긴급도이고_하루_정리는_아니다() {
    assert_eq!(NoticeKind::ApprovalRequest.urgency(), Urgency::High);
    assert_eq!(NoticeKind::Stall.urgency(), Urgency::High);
    assert_eq!(NoticeKind::Anomaly.urgency(), Urgency::High);
    assert_eq!(NoticeKind::AccessLock.urgency(), Urgency::High);
    assert_ne!(NoticeKind::DailySummary.urgency(), Urgency::High);
}
