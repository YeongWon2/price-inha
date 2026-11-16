//! 실전 승격에서 "템플릿을 읽은 뒤 승격 사이"의 틈(저장소 `promote_template_live_checked`).
//!
//! 순차 호출로는 그 틈을 만들 수 없어서, 승격 시각을 얻으려고 시계를 두 번째 부를 때 저장소를 한 번 고치는
//! 시계를 쓴다. 승격 시각은 템플릿을 읽은 **뒤**(승격 바로 앞)에 시계로 얻으므로, 두 번째 시계 호출은 읽은 뒤다.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::common::{T0, ask, full_stop, pending_template};
use pi_approval::{
    Answer, ApprovalNumber, Approvals, Clock, Decision, Error, RejectReason, Subject, WaitTimes,
};
use pi_store::{Store, TemplateId, TemplateState, UnixMillis};

type Meddle = Box<dyn Fn(&Store) + Send + Sync>;

/// 켜 둔 동안 두 번째로 불릴 때 저장소를 한 번 고치는 시계.
struct GapClock {
    store: Store,
    armed: AtomicBool,
    calls: AtomicU32,
    meddle: Mutex<Option<Meddle>>,
}

impl Clock for GapClock {
    fn now(&self) -> UnixMillis {
        if self.armed.load(Ordering::SeqCst) && self.calls.fetch_add(1, Ordering::SeqCst) + 1 == 2 {
            self.armed.store(false, Ordering::SeqCst);
            if let Some(meddle) = self.meddle.lock().unwrap().take() {
                meddle(&self.store);
            }
        }
        UnixMillis(T0)
    }
}

struct Gap {
    _dir: tempfile::TempDir,
    store: Store,
    clock: Arc<GapClock>,
    approvals: Approvals,
}

impl Gap {
    fn new() -> Gap {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("pi.sqlite")).unwrap();
        let clock = Arc::new(GapClock {
            store: store.clone(),
            armed: AtomicBool::new(false),
            calls: AtomicU32::new(0),
            meddle: Mutex::new(None),
        });
        let approvals = Approvals::new(store.clone(), clock.clone(), WaitTimes::default());
        Gap {
            _dir: dir,
            store,
            clock,
            approvals,
        }
    }

    /// 승인받은 승격 번호를 만들고, 승격 중 시계의 두 번째 호출에서 `meddle`이 돌게 켠다.
    fn approved_and_armed(&self, template: TemplateId, meddle: Meddle) -> ApprovalNumber {
        let subject = Subject::LivePromotion {
            template,
            revision: 1,
        };
        let r = self.approvals.request(ask(subject)).unwrap();
        let a = self
            .approvals
            .answer(&r.number, r.fingerprint.as_str(), Decision::Approve)
            .unwrap();
        assert_eq!(a, Answer::Approved);
        *self.clock.meddle.lock().unwrap() = Some(meddle);
        self.clock.calls.store(0, Ordering::SeqCst);
        self.clock.armed.store(true, Ordering::SeqCst);
        r.number
    }
}

// 검증: E7 V44 M5
#[test]
fn template_revised_between_read_and_promotion_is_voided_and_not_promoted() {
    let g = Gap::new();
    let t = pending_template(&g.store);
    let n = g.approved_and_armed(
        t,
        Box::new(move |store| {
            store
                .reject_template_promotion(t, UnixMillis(T0 - 80))
                .unwrap();
            store
                .revise_template(t, &serde_json::json!({ "한도": 500 }), UnixMillis(T0 - 70))
                .unwrap();
            store
                .submit_template_for_promotion(
                    t,
                    &serde_json::json!({ "기준": "통과" }),
                    UnixMillis(T0 - 60),
                )
                .unwrap();
        }),
    );
    match g.approvals.promote_live(&n, t) {
        Err(Error::Denied(RejectReason::FingerprintMismatch)) => {}
        other => panic!("판이 바뀐 승격은 FingerprintMismatch여야 한다: {other:?}"),
    }
    let stored = g.store.get_template(t).unwrap();
    assert_eq!(stored.state, TemplateState::PromotionPending);
    assert_eq!(stored.revision, 2);
    assert_eq!(stored.approval, None);
    // 승인은 무효가 됐다: 다시 시도해도 쓸 수 없다.
    assert!(matches!(
        g.approvals.promote_live(&n, t),
        Err(Error::Denied(_))
    ));
    assert_eq!(g.store.get_template(t).unwrap().approval, None);
}

// 검증: P20 V44
#[test]
fn full_stop_between_read_and_promotion_denies_and_keeps_approval() {
    let g = Gap::new();
    let t = pending_template(&g.store);
    let n = g.approved_and_armed(t, Box::new(full_stop));
    match g.approvals.promote_live(&n, t) {
        Err(Error::Denied(RejectReason::FullStop)) => {}
        other => panic!("전체 정지는 FullStop이어야 한다: {other:?}"),
    }
    let stored = g.store.get_template(t).unwrap();
    assert_eq!(stored.state, TemplateState::PromotionPending);
    assert_eq!(stored.approval, None);
    assert!(!g.approvals.get(&n).unwrap().unwrap().consumed);
}
