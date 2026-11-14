#![cfg(test)]
//! 매매 (M2): 허용 표 전체, 근거 빈 칸, 자료 시각, 손절값 좁히기, 계좌 · 점수 · 승인, 금액 정밀도.

mod common;

use common::{t, temp_store};
use pi_store::{
    AccountKind, Amount, Approval, DenyReason, Judgment, NewTrade, Rationale, RationaleField,
    RoleScore, SourceRef, Store, StoreError, Trade, TradeId, TradeState, Transition,
};

use TradeState::{AwaitingApproval, Blocked, Discussing, Holding, Judged, Ordered, Reviewed, Sold};

/// 허용되는 전이. `spec.md`의 흐름(F2 · F3 · F4 · F5)에 글로 적힌 것만 넣었다.
/// 이 밖의 56쌍(같은 상태 8쌍 포함)은 모두 거부다.
const ALLOWED: [(TradeState, TradeState); 8] = [
    (Discussing, Judged),
    (Judged, AwaitingApproval),
    (Judged, Blocked),
    (Judged, Ordered),
    (AwaitingApproval, Ordered),
    (Ordered, Holding),
    (Holding, Sold),
    (Sold, Reviewed),
];

const STATES: [TradeState; 8] = [
    Discussing,
    Judged,
    AwaitingApproval,
    Blocked,
    Ordered,
    Holding,
    Sold,
    Reviewed,
];

fn full_rationale() -> Rationale {
    Rationale {
        hypothesis: Some("실적 발표 뒤 눌림에서 되돌린다".to_string()),
        trigger: Some("거래량이 평균의 두 배를 넘었다".to_string()),
        entry: Some(Amount(71_500)),
        stop: Some(Amount(69_000)),
        target: Some(Amount(78_000)),
        expected_value: Some(Amount(1_200)),
    }
}

fn judgment(rationale: Rationale) -> Judgment {
    Judgment {
        decided_at: t(1_000),
        rationale,
        sources: vec![
            SourceRef {
                name: "일봉 시세".to_string(),
                known_at: t(900),
            },
            SourceRef {
                name: "분기 실적 공시".to_string(),
                known_at: t(1_000),
            },
        ],
        scores: vec![
            RoleScore {
                role: "시세 담당".to_string(),
                score: 72,
                reason: "추세가 살아 있다".to_string(),
            },
            RoleScore {
                role: "위험 담당".to_string(),
                score: 55,
                reason: "손절 폭이 넓다".to_string(),
            },
        ],
        consensus_score: 64,
    }
}

fn new_trade(store: &Store) -> Trade {
    store
        .create_trade(NewTrade {
            symbol: "005930".to_string(),
            account: AccountKind::Paper,
            at: t(0),
        })
        .unwrap()
}

/// 허용된 길로만 `state`까지 간 매매를 만든다. 근거는 `rationale`로 판단한다.
fn trade_in_with(store: &Store, state: TradeState, rationale: Rationale) -> TradeId {
    let id = new_trade(store).id;
    if state == Discussing {
        return id;
    }
    store.judge_trade(id, judgment(rationale)).unwrap();
    let path: &[TradeState] = match state {
        Discussing | Judged => &[],
        AwaitingApproval => &[AwaitingApproval],
        Blocked => &[Blocked],
        Ordered => &[Ordered],
        Holding => &[Ordered, Holding],
        Sold => &[Ordered, Holding, Sold],
        Reviewed => &[Ordered, Holding, Sold, Reviewed],
    };
    for (i, to) in path.iter().enumerate() {
        store
            .transition_trade(id, *to, t(2_000 + i as i64))
            .unwrap();
    }
    assert_eq!(store.get_trade(id).unwrap().state, state);
    id
}

fn trade_in(store: &Store, state: TradeState) -> TradeId {
    trade_in_with(store, state, full_rationale())
}

/// `to`로 가 본다. 판단됨은 판단으로, 나머지는 전이로.
fn attempt(store: &Store, id: TradeId, to: TradeState) -> Result<Trade, StoreError> {
    if to == Judged {
        store.judge_trade(id, judgment(full_rationale()))
    } else {
        store.transition_trade(id, to, t(5_000))
    }
}

fn assert_denied(result: Result<Trade, StoreError>, from: TradeState, to: TradeState) {
    match result {
        Err(StoreError::TransitionDenied { transition, .. }) => {
            assert_eq!(transition, Transition::Trade { from, to });
        }
        other => panic!("{from:?} → {to:?} 는 전이 거부여야 한다: {other:?}"),
    }
}

// 검증: M2
#[test]
fn 상태는_여덟_개다() {
    assert_eq!(TradeState::ALL.len(), 8);
    for s in STATES {
        assert!(TradeState::ALL.contains(&s));
    }
}

// 검증: M2
#[test]
fn 새_매매는_논의중이고_근거와_점수와_승인이_비어_있다() {
    let (_dir, _path, store) = temp_store();
    let trade = store
        .create_trade(NewTrade {
            symbol: "AAPL".to_string(),
            account: AccountKind::Live,
            at: t(7),
        })
        .unwrap();
    assert_eq!(trade.state, Discussing);
    assert_eq!(trade.symbol, "AAPL");
    assert_eq!(trade.account, AccountKind::Live);
    assert_eq!(trade.created_at, t(7));
    assert_eq!(trade.updated_at, t(7));
    assert_eq!(trade.rationale, Rationale::default());
    assert_eq!(trade.decided_at, None);
    assert!(trade.sources.is_empty());
    assert!(trade.scores.is_empty());
    assert_eq!(trade.consensus_score, None);
    assert_eq!(trade.approval, None);
    assert_eq!(store.get_trade(trade.id).unwrap(), trade);
}

// 검증: M2
#[test]
fn 매매_번호는_서로_다르고_없는_번호는_찾을_수_없다() {
    let (_dir, _path, store) = temp_store();
    let a = new_trade(&store).id;
    let b = new_trade(&store).id;
    assert_ne!(a, b);
    let missing = TradeId(a.0.max(b.0) + 1_000);
    assert!(matches!(
        store.get_trade(missing),
        Err(StoreError::TradeNotFound(id)) if id == missing
    ));
    assert!(matches!(
        store.transition_trade(missing, Ordered, t(1)),
        Err(StoreError::TradeNotFound(_))
    ));
}

// 검증: M2
#[test]
fn 허용_표에_있는_전이는_모두_된다() {
    let (_dir, _path, store) = temp_store();
    for (from, to) in ALLOWED {
        let id = trade_in(&store, from);
        let trade = attempt(&store, id, to)
            .unwrap_or_else(|e| panic!("{from:?} → {to:?} 는 되어야 한다: {e}"));
        assert_eq!(trade.state, to);
        assert_eq!(store.get_trade(id).unwrap().state, to);
    }
}

// 검증: M2
#[test]
fn 허용_표에_없는_전이는_모두_거부되고_매매는_그대로다() {
    let (_dir, _path, store) = temp_store();
    let mut checked = 0;
    for from in STATES {
        for to in STATES {
            if ALLOWED.contains(&(from, to)) {
                continue;
            }
            let id = trade_in(&store, from);
            let before = store.get_trade(id).unwrap();
            assert_denied(attempt(&store, id, to), from, to);
            assert_eq!(store.get_trade(id).unwrap(), before);
            checked += 1;
        }
    }
    assert_eq!(checked, 64 - ALLOWED.len());
}

// 검증: M2
#[test]
fn 판단됨은_판단으로만_가고_전이_호출로는_가지_못한다() {
    let (_dir, _path, store) = temp_store();
    for from in STATES {
        let id = trade_in(&store, from);
        let before = store.get_trade(id).unwrap();
        let result = store.transition_trade(id, Judged, t(5_000));
        match result {
            Err(StoreError::TransitionDenied { reason, .. }) => {
                assert_eq!(reason, DenyReason::NotAllowed);
            }
            other => panic!("{from:?}에서 전이 호출로 판단됨은 거부여야 한다: {other:?}"),
        }
        assert_eq!(store.get_trade(id).unwrap(), before);
    }
}

// 검증: M2
#[test]
fn 판단됨이_될_때_근거와_자료_시각과_담당_점수와_합의_점수가_함께_남는다() {
    let (_dir, _path, store) = temp_store();
    let id = new_trade(&store).id;
    let j = judgment(full_rationale());
    let trade = store.judge_trade(id, j.clone()).unwrap();
    assert_eq!(trade.state, Judged);
    assert_eq!(trade.rationale, j.rationale);
    assert_eq!(trade.decided_at, Some(j.decided_at));
    assert_eq!(trade.updated_at, j.decided_at);
    assert_eq!(trade.sources, j.sources);
    assert_eq!(trade.scores, j.scores);
    assert_eq!(trade.consensus_score, Some(64));
    assert_eq!(store.get_trade(id).unwrap(), trade);
}

// 검증: M2
#[test]
fn 자료의_알게_된_시각이_판단_시각보다_늦으면_저장을_거부한다() {
    let (_dir, _path, store) = temp_store();
    let id = new_trade(&store).id;
    let before = store.get_trade(id).unwrap();

    let mut j = judgment(full_rationale());
    // 앞의 자료는 괜찮고 마지막 하나만 1밀리초 늦다.
    j.sources.push(SourceRef {
        name: "장 마감 뒤 뉴스".to_string(),
        known_at: t(1_001),
    });
    match store.judge_trade(id, j) {
        Err(StoreError::SourceAfterDecision {
            source_name,
            known_at,
            decided_at,
        }) => {
            assert_eq!(source_name, "장 마감 뒤 뉴스");
            assert_eq!(known_at, t(1_001));
            assert_eq!(decided_at, t(1_000));
        }
        other => panic!("늦은 자료는 거부여야 한다: {other:?}"),
    }
    // 아무것도 저장되지 않았다: 논의중이고 근거 · 자료 · 점수가 비어 있다.
    assert_eq!(store.get_trade(id).unwrap(), before);
    assert!(store.trades_in_state(Judged).unwrap().is_empty());
}

// 검증: M2
#[test]
fn 자료의_알게_된_시각이_판단_시각과_같으면_저장된다() {
    let (_dir, _path, store) = temp_store();
    let id = new_trade(&store).id;
    let mut j = judgment(full_rationale());
    j.sources = vec![SourceRef {
        name: "방금 받은 시세".to_string(),
        known_at: j.decided_at,
    }];
    let trade = store.judge_trade(id, j.clone()).unwrap();
    assert_eq!(trade.state, Judged);
    assert_eq!(trade.sources, j.sources);
}

/// 근거 한 칸만 비운 것들. (빈 칸, 근거)
fn rationales_with_one_gap() -> Vec<(RationaleField, Rationale)> {
    let full = full_rationale;
    vec![
        (
            RationaleField::Hypothesis,
            Rationale {
                hypothesis: None,
                ..full()
            },
        ),
        (
            RationaleField::Hypothesis,
            Rationale {
                hypothesis: Some(String::new()),
                ..full()
            },
        ),
        (
            RationaleField::Hypothesis,
            Rationale {
                hypothesis: Some("  \n\t ".to_string()),
                ..full()
            },
        ),
        (
            RationaleField::Trigger,
            Rationale {
                trigger: None,
                ..full()
            },
        ),
        (
            RationaleField::Trigger,
            Rationale {
                trigger: Some(" ".to_string()),
                ..full()
            },
        ),
        (
            RationaleField::Entry,
            Rationale {
                entry: None,
                ..full()
            },
        ),
        (
            RationaleField::Stop,
            Rationale {
                stop: None,
                ..full()
            },
        ),
        (
            RationaleField::Target,
            Rationale {
                target: None,
                ..full()
            },
        ),
        (
            RationaleField::ExpectedValue,
            Rationale {
                expected_value: None,
                ..full()
            },
        ),
    ]
}

// 검증: M2
#[test]
fn 근거가_하나라도_비면_판단됨에서_주문됨으로_갈_수_없다() {
    let (_dir, _path, store) = temp_store();
    for (field, rationale) in rationales_with_one_gap() {
        let id = trade_in_with(&store, Judged, rationale);
        let before = store.get_trade(id).unwrap();
        match store.transition_trade(id, Ordered, t(3_000)) {
            Err(StoreError::TransitionDenied { transition, reason }) => {
                assert_eq!(
                    transition,
                    Transition::Trade {
                        from: Judged,
                        to: Ordered
                    }
                );
                assert_eq!(
                    reason,
                    DenyReason::RationaleIncomplete {
                        missing: vec![field]
                    }
                );
            }
            other => panic!("{field:?}가 빈 매매는 주문됨이 될 수 없다: {other:?}"),
        }
        assert_eq!(store.get_trade(id).unwrap(), before);
    }
}

// 검증: M2
#[test]
fn 근거가_하나라도_비면_승인대기에서도_주문됨으로_갈_수_없다() {
    let (_dir, _path, store) = temp_store();
    for (field, rationale) in rationales_with_one_gap() {
        let id = trade_in_with(&store, AwaitingApproval, rationale);
        match store.transition_trade(id, Ordered, t(3_000)) {
            Err(StoreError::TransitionDenied {
                reason: DenyReason::RationaleIncomplete { missing },
                ..
            }) => assert_eq!(missing, vec![field]),
            other => panic!("{field:?}가 빈 매매는 주문됨이 될 수 없다: {other:?}"),
        }
        assert_eq!(store.get_trade(id).unwrap().state, AwaitingApproval);
    }
}

// 검증: M2
#[test]
fn 근거가_모두_비면_빈_칸_여섯_개가_모두_알려진다() {
    let (_dir, _path, store) = temp_store();
    let id = trade_in_with(&store, Judged, Rationale::default());
    match store.transition_trade(id, Ordered, t(3_000)) {
        Err(StoreError::TransitionDenied {
            reason: DenyReason::RationaleIncomplete { missing },
            ..
        }) => {
            assert_eq!(missing.len(), 6);
            for field in [
                RationaleField::Hypothesis,
                RationaleField::Trigger,
                RationaleField::Entry,
                RationaleField::Stop,
                RationaleField::Target,
                RationaleField::ExpectedValue,
            ] {
                assert!(missing.contains(&field), "{field:?}가 빠졌다");
            }
        }
        other => panic!("근거 없는 매매는 주문됨이 될 수 없다: {other:?}"),
    }
}

// 검증: M2
#[test]
fn 근거가_빈_매매는_막힘으로_간다() {
    let (_dir, _path, store) = temp_store();
    let id = trade_in_with(&store, Judged, Rationale::default());
    let trade = store.transition_trade(id, Blocked, t(3_000)).unwrap();
    assert_eq!(trade.state, Blocked);
    assert_eq!(trade.updated_at, t(3_000));
}

// 검증: M2
#[test]
fn 보유중_손절값은_더_높게만_바꿀_수_있다() {
    let (_dir, _path, store) = temp_store();
    let id = trade_in(&store, Holding);
    assert_eq!(
        store.get_trade(id).unwrap().rationale.stop,
        Some(Amount(69_000))
    );

    // 넓히기(더 낮게): 거부.
    for lower in [Amount(68_999), Amount(1), Amount(0), Amount(-5)] {
        match store.update_stop(id, lower, t(4_000)) {
            Err(StoreError::StopNotTightened { current, requested }) => {
                assert_eq!(current, Amount(69_000));
                assert_eq!(requested, lower);
            }
            other => panic!("손절값 넓히기는 거부여야 한다: {other:?}"),
        }
        assert_eq!(
            store.get_trade(id).unwrap().rationale.stop,
            Some(Amount(69_000))
        );
    }

    // 좁히기(더 높게): 된다. 최소 단위 하나만 높아도 된다.
    let trade = store.update_stop(id, Amount(69_001), t(4_100)).unwrap();
    assert_eq!(trade.rationale.stop, Some(Amount(69_001)));
    assert_eq!(trade.updated_at, t(4_100));
    assert_eq!(trade.state, Holding);

    let trade = store.update_stop(id, Amount(70_500), t(4_200)).unwrap();
    assert_eq!(trade.rationale.stop, Some(Amount(70_500)));

    // 한 번 좁힌 뒤에는 처음 값으로도 돌아가지 못한다.
    assert!(matches!(
        store.update_stop(id, Amount(69_000), t(4_300)),
        Err(StoreError::StopNotTightened { .. })
    ));
    let after = store.get_trade(id).unwrap();
    assert_eq!(after.rationale.stop, Some(Amount(70_500)));
    // 다른 근거는 손대지 않는다.
    assert_eq!(
        Rationale {
            stop: Some(Amount(69_000)),
            ..after.rationale
        },
        full_rationale()
    );
}

// 검증: M2
// 모호: 지시서는 "더 높게"라고만 한다. 같은 값은 좁히기가 아니라고 보고 거부로 썼다.
#[test]
fn 보유중_손절값을_같은_값으로_바꾸는_것은_거부된다() {
    let (_dir, _path, store) = temp_store();
    let id = trade_in(&store, Holding);
    assert!(matches!(
        store.update_stop(id, Amount(69_000), t(4_000)),
        Err(StoreError::StopNotTightened { current, requested })
            if current == Amount(69_000) && requested == Amount(69_000)
    ));
}

// 검증: M2
// 모호: 보유중이 아닐 때 손절값을 바꿀 수 있는지는 스펙에 없다. 보수적으로 거부로 썼다.
#[test]
fn 보유중이_아닌_매매의_손절값은_바꿀_수_없다() {
    let (_dir, _path, store) = temp_store();
    for state in [Judged, AwaitingApproval, Blocked, Ordered, Sold, Reviewed] {
        let id = trade_in(&store, state);
        match store.update_stop(id, Amount(70_000), t(4_000)) {
            Err(StoreError::StopNotAdjustable { state: s }) => assert_eq!(s, state),
            other => panic!("{state:?}에서는 손절값을 바꿀 수 없다: {other:?}"),
        }
        assert_eq!(
            store.get_trade(id).unwrap().rationale.stop,
            Some(Amount(69_000))
        );
    }
}

// 검증: M2
#[test]
fn 계좌_종류는_모의와_실전이_섞이지_않고_남는다() {
    let (_dir, _path, store) = temp_store();
    let paper = store
        .create_trade(NewTrade {
            symbol: "005930".to_string(),
            account: AccountKind::Paper,
            at: t(1),
        })
        .unwrap();
    let live = store
        .create_trade(NewTrade {
            symbol: "005930".to_string(),
            account: AccountKind::Live,
            at: t(2),
        })
        .unwrap();
    store
        .judge_trade(paper.id, judgment(full_rationale()))
        .unwrap();
    store
        .judge_trade(live.id, judgment(full_rationale()))
        .unwrap();
    store.transition_trade(live.id, Ordered, t(3_000)).unwrap();
    assert_eq!(
        store.get_trade(paper.id).unwrap().account,
        AccountKind::Paper
    );
    assert_eq!(store.get_trade(live.id).unwrap().account, AccountKind::Live);
}

// 검증: M2
#[test]
fn 승인_번호와_내용_지문이_매매에_남는다() {
    let (_dir, _path, store) = temp_store();
    let id = trade_in(&store, AwaitingApproval);
    let approval = Approval {
        number: 17,
        content_fingerprint: "sha256:3f1c9a0d".to_string(),
    };
    let trade = store
        .set_trade_approval(id, approval.clone(), t(2_500))
        .unwrap();
    assert_eq!(trade.approval, Some(approval.clone()));
    assert_eq!(trade.state, AwaitingApproval);

    // 주문됨으로 간 뒤에도 그대로 남는다.
    let trade = store.transition_trade(id, Ordered, t(2_600)).unwrap();
    assert_eq!(trade.approval, Some(approval.clone()));
    assert_eq!(store.get_trade(id).unwrap().approval, Some(approval));

    assert!(matches!(
        store.set_trade_approval(
            TradeId(id.0 + 1_000),
            Approval {
                number: 18,
                content_fingerprint: "sha256:00".to_string(),
            },
            t(2_700),
        ),
        Err(StoreError::TradeNotFound(_))
    ));
}

// 검증: M2
#[test]
fn 상태별_매매_목록은_그_상태의_매매만_번호_순서로_준다() {
    let (_dir, _path, store) = temp_store();
    let h1 = trade_in(&store, Holding);
    let _d = trade_in(&store, Discussing);
    let h2 = trade_in(&store, Holding);
    let s = trade_in(&store, Sold);

    let holding = store.trades_in_state(Holding).unwrap();
    assert_eq!(
        holding.iter().map(|x| x.id).collect::<Vec<_>>(),
        vec![h1, h2]
    );
    assert!(holding.iter().all(|x| x.state == Holding));
    assert_eq!(holding[0], store.get_trade(h1).unwrap());

    let sold = store.trades_in_state(Sold).unwrap();
    assert_eq!(sold.iter().map(|x| x.id).collect::<Vec<_>>(), vec![s]);
    assert!(store.trades_in_state(Reviewed).unwrap().is_empty());
}

// 검증: M2
#[test]
fn 금액과_가격은_부동소수점을_거치지_않고_한_자리도_틀리지_않게_남는다() {
    let (_dir, _path, store) = temp_store();
    // 2^53 + 1 은 f64로 나타낼 수 없다. 부동소수점을 거치면 값이 달라진다.
    let rationale = Rationale {
        entry: Some(Amount(9_007_199_254_740_993)),
        stop: Some(Amount(9_007_199_254_740_991)),
        target: Some(Amount(i64::MAX)),
        expected_value: Some(Amount(-9_007_199_254_740_993)),
        ..full_rationale()
    };
    let id = trade_in_with(&store, Holding, rationale.clone());
    assert_eq!(store.get_trade(id).unwrap().rationale, rationale);

    // 2^53 근처에서도 최소 단위 하나 차이를 가린다.
    let trade = store
        .update_stop(id, Amount(9_007_199_254_740_992), t(4_000))
        .unwrap();
    assert_eq!(trade.rationale.stop, Some(Amount(9_007_199_254_740_992)));
    assert!(matches!(
        store.update_stop(id, Amount(9_007_199_254_740_992), t(4_100)),
        Err(StoreError::StopNotTightened { .. })
    ));
    let trade = store
        .update_stop(id, Amount(9_007_199_254_740_993), t(4_200))
        .unwrap();
    assert_eq!(trade.rationale.stop, Some(Amount(9_007_199_254_740_993)));
}
