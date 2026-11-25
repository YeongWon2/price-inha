//! 보낸 사람 판정 · 페어링(V88). 수치는 원본(OpenClaw · Hermes), 허용 1명 · DM만은 결정 24.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::TimeDelta;
use pi_channel::{
    ApproveError, BookError, CODE_ALPHABET, CODE_LEN, Error, Gatekeeper, IgnoreReason, Judgement,
    MAX_ALLOWED, MAX_FAILED_APPROVALS, MAX_PENDING, PAIRING_LOCKOUT_SECS, PAIRING_RATE_LIMIT_SECS,
    PAIRING_TTL_SECS, PairingBook, PairingCode, PairingState,
};

use crate::common::{TestBook, at_min, dm, group, sender, t0};

fn keeper() -> (Arc<TestBook>, Gatekeeper) {
    let book = TestBook::new();
    let gk = Gatekeeper::new(book.clone());
    (book, gk)
}

/// 새 코드를 받아야 한다.
fn issued(j: Judgement) -> PairingCode {
    match j {
        Judgement::PairingIssued { code } => code,
        other => panic!("코드를 받아야 한다: {other:?}"),
    }
}

/// 8자리 · 글자표 안.
fn assert_code_shape(code: &PairingCode) {
    assert_eq!(code.as_str().chars().count(), CODE_LEN);
    assert!(
        code.as_str().chars().all(|c| CODE_ALPHABET.contains(c)),
        "글자표 밖 글자"
    );
}

fn ignored(reason: IgnoreReason) -> Judgement {
    Judgement::Ignored { reason }
}

// 검증: V88
#[test]
fn 수치는_원본과_같다() {
    assert_eq!(CODE_LEN, 8);
    assert_eq!(CODE_ALPHABET, "ABCDEFGHJKLMNPQRSTUVWXYZ23456789");
    assert_eq!(PAIRING_TTL_SECS, 3600);
    assert_eq!(MAX_PENDING, 3);
    assert_eq!(PAIRING_RATE_LIMIT_SECS, 600);
    assert_eq!(MAX_FAILED_APPROVALS, 5);
    assert_eq!(PAIRING_LOCKOUT_SECS, 3600);
    assert_eq!(MAX_ALLOWED, 1);
}

// 검증: V88
#[test]
fn 허용된_사람의_1대1_글은_받는다() {
    let (book, gk) = keeper();
    book.set(PairingState {
        allowed: vec![sender("5001")],
        ..PairingState::default()
    });
    assert_eq!(gk.judge(&dm("5001"), t0()), Ok(Judgement::Allowed));
    assert_eq!(gk.allowed(), Ok(Some(sender("5001"))));
    assert!(book.state().pending.is_empty());
}

// 검증: V88
#[test]
fn 처음_보는_1대1은_코드를_받고_요청이_남는다() {
    let (book, gk) = keeper();
    let code = issued(gk.judge(&dm("5001"), t0()).unwrap());
    assert_code_shape(&code);
    let state = book.state();
    assert_eq!(state.pending.len(), 1);
    assert_eq!(state.pending[0].code, code);
    assert_eq!(state.pending[0].sender, sender("5001"));
    assert_eq!(state.pending[0].created_at, t0());
    assert!(
        state.allowed.is_empty(),
        "코드를 주는 것만으로 허용하지 않는다"
    );
}

// 검증: V88
#[test]
fn 같은_사람이_다시_말하면_코드를_다시_주지_않는다() {
    let (book, gk) = keeper();
    let code = issued(gk.judge(&dm("5001"), t0()).unwrap());
    for m in [1, 9, 30, 59] {
        assert_eq!(
            gk.judge(&dm("5001"), at_min(m)),
            Ok(Judgement::PairingPending)
        );
    }
    let state = book.state();
    assert_eq!(state.pending.len(), 1);
    assert_eq!(state.pending[0].code, code);
}

// 검증: V88
#[test]
fn 십_분_안에는_요청이_없어도_새_코드를_주지_않는다() {
    let (book, gk) = keeper();
    issued(gk.judge(&dm("5001"), t0()).unwrap());
    let mut state = book.state();
    state.pending.clear();
    book.set(state);
    assert_eq!(
        gk.judge(&dm("5001"), at_min(5)),
        Ok(ignored(IgnoreReason::RateLimited))
    );
    assert!(book.state().pending.is_empty());
    issued(gk.judge(&dm("5001"), at_min(11)).unwrap());
}

// 검증: V88
#[test]
fn 요청은_한_시간_뒤_만료되고_옛_코드는_맞지_않는다() {
    let (book, gk) = keeper();
    let old = issued(gk.judge(&dm("5001"), t0()).unwrap());
    let new = issued(gk.judge(&dm("5001"), at_min(61)).unwrap());
    assert_ne!(old, new);
    assert_eq!(book.state().pending.len(), 1);
    assert_eq!(
        gk.approve(old.as_str(), at_min(62)),
        Err(ApproveError::WrongCode)
    );
    assert_eq!(gk.approve(new.as_str(), at_min(63)), Ok(sender("5001")));
}

// 검증: V88
#[test]
fn 기다리는_요청이_3건이면_더_만들지_않는다() {
    let (book, gk) = keeper();
    for id in ["5001", "5002", "5003"] {
        issued(gk.judge(&dm(id), t0()).unwrap());
    }
    assert_eq!(
        gk.judge(&dm("5004"), at_min(1)),
        Ok(ignored(IgnoreReason::TooManyPending))
    );
    assert_eq!(book.state().pending.len(), 3);
    // 앞 요청들이 만료되면 다시 받는다.
    issued(gk.judge(&dm("5004"), at_min(61)).unwrap());
    assert_eq!(book.state().pending.len(), 1);
}

// 검증: V88
#[test]
fn 그룹_글은_허용된_사람이어도_받지_않는다() {
    let (book, gk) = keeper();
    assert_eq!(
        gk.judge(&group("5001"), t0()),
        Ok(ignored(IgnoreReason::NotDirect))
    );
    assert!(
        book.state().pending.is_empty(),
        "그룹 글로 요청을 만들지 않는다"
    );
    book.set(PairingState {
        allowed: vec![sender("5001")],
        ..PairingState::default()
    });
    assert_eq!(
        gk.judge(&group("5001"), t0()),
        Ok(ignored(IgnoreReason::NotDirect))
    );
}

// 검증: V88
#[test]
fn 맞는_코드로_승인하면_허용하고_요청을_지운다() {
    let (book, gk) = keeper();
    let code = issued(gk.judge(&dm("5001"), t0()).unwrap());
    assert_eq!(gk.approve(code.as_str(), at_min(1)), Ok(sender("5001")));
    let state = book.state();
    assert_eq!(state.allowed, vec![sender("5001")]);
    assert!(state.pending.is_empty());
    assert_eq!(gk.allowed(), Ok(Some(sender("5001"))));
    assert_eq!(gk.judge(&dm("5001"), at_min(2)), Ok(Judgement::Allowed));
    assert_eq!(
        gk.approve(code.as_str(), at_min(3)),
        Err(ApproveError::OwnerExists),
        "이미 1명이 있다"
    );
}

// 검증: V88
#[test]
fn 이미_한_명이_있으면_승인을_거절하고_요청은_둔다() {
    let (book, gk) = keeper();
    book.set(PairingState {
        allowed: vec![sender("9001")],
        ..PairingState::default()
    });
    let code = issued(gk.judge(&dm("5001"), t0()).unwrap());
    assert_eq!(
        gk.approve(code.as_str(), at_min(1)),
        Err(ApproveError::OwnerExists)
    );
    let state = book.state();
    assert_eq!(state.allowed, vec![sender("9001")]);
    assert_eq!(state.pending.len(), 1);
}

// 검증: V88
#[test]
fn 틀린_코드_4번은_잠그지_않는다() {
    let (_book, gk) = keeper();
    let code = issued(gk.judge(&dm("5001"), t0()).unwrap());
    for i in 0..4 {
        assert_eq!(
            gk.approve("ZZZZZZZZ", at_min(1 + i)),
            Err(ApproveError::WrongCode)
        );
    }
    assert_eq!(gk.approve(code.as_str(), at_min(6)), Ok(sender("5001")));
}

// 검증: V88
#[test]
fn 틀린_코드_5번이면_한_시간_동안_승인을_받지_않는다() {
    let (_book, gk) = keeper();
    let code = issued(gk.judge(&dm("5001"), t0()).unwrap());
    for i in 1..=4 {
        assert_eq!(
            gk.approve("ZZZZZZZZ", at_min(i)),
            Err(ApproveError::WrongCode)
        );
    }
    let fifth = at_min(5);
    assert!(gk.approve("ZZZZZZZZ", fifth).is_err());
    let until = fifth + TimeDelta::seconds(PAIRING_LOCKOUT_SECS);
    assert_eq!(
        gk.approve(code.as_str(), at_min(6)),
        Err(ApproveError::Locked { until }),
        "잠금 중에는 맞는 코드도 받지 않는다"
    );
    assert_eq!(gk.allowed(), Ok(None));
    // 잠금이 풀린 뒤(옛 요청은 만료) 새 코드로 승인된다.
    let again = issued(gk.judge(&dm("5001"), at_min(66)).unwrap());
    assert_eq!(gk.approve(again.as_str(), at_min(67)), Ok(sender("5001")));
}

// 검증: V88
#[test]
fn 뺀_사람은_다시_처음_보는_사람이다() {
    let (book, gk) = keeper();
    let code = issued(gk.judge(&dm("5001"), t0()).unwrap());
    gk.approve(code.as_str(), at_min(1)).unwrap();
    assert_eq!(gk.revoke(&sender("5001")), Ok(true));
    assert_eq!(gk.allowed(), Ok(None));
    assert!(book.state().allowed.is_empty());
    assert_eq!(
        gk.revoke(&sender("5001")),
        Ok(false),
        "없는 사람을 빼면 거짓"
    );
    let again = issued(gk.judge(&dm("5001"), at_min(20)).unwrap());
    assert_ne!(again, code);
}

// 검증: V88
#[test]
fn 코드는_같은_시계_같은_사람이어도_매번_다르다() {
    let mut seen = HashSet::new();
    for _ in 0..64 {
        let (_book, gk) = keeper();
        let code = issued(gk.judge(&dm("5001"), t0()).unwrap());
        assert_code_shape(&code);
        seen.insert(code.as_str().to_string());
    }
    assert_eq!(seen.len(), 64, "겹친 코드가 있다");
    let (_book, gk) = keeper();
    let a = issued(gk.judge(&dm("5001"), t0()).unwrap());
    let b = issued(gk.judge(&dm("5002"), t0()).unwrap());
    assert_ne!(a, b);
}

/// 늘 실패하는 장부.
struct BrokenBook;

impl PairingBook for BrokenBook {
    fn load(&self) -> Result<PairingState, BookError> {
        Err(BookError("디스크 오류".to_string()))
    }

    fn save(&self, _state: &PairingState) -> Result<(), BookError> {
        Err(BookError("디스크 오류".to_string()))
    }
}

// 검증: V88
#[test]
fn 장부를_읽지_못하면_종류_있는_오류다() {
    let gk = Gatekeeper::new(Arc::new(BrokenBook));
    assert!(matches!(gk.judge(&dm("5001"), t0()), Err(Error::Book(_))));
    assert!(matches!(
        gk.approve("ABCDEFGH", t0()),
        Err(ApproveError::Book(_))
    ));
    assert!(matches!(gk.revoke(&sender("5001")), Err(Error::Book(_))));
    assert!(matches!(gk.allowed(), Err(Error::Book(_))));
}
