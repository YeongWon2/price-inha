#![cfg(test)]
//! 후보 일괄 처리(스키마 버전 8)의 공개 API.
//!
//! - 매매 방향: 판단과 한 묶음으로 남고, 다시 열어도 팔기 매매를 알아본다.
//! - 매매 수량: 승인이 붙기 전에만 두거나 바꾼다(실행이 승인 내용과 대조한다).
//! - 매매 상태 바뀜 이력: 만들기와 모든 전이가 남고, 시각 범위로 읽는다(하루 정리).
//! - 개선안 위험 표시: 제안됨일 때 올리기만 한다.
//! - 실전 승격 확인판: 전체 정지와 판을 승격과 같은 묶음에서 본다.
//! - 지식: 조각 번호 조회 · 메타 키 지우기 · 조각과 메타를 한 묶음으로 올리기.

mod common;

use common::{t, temp_store};
use pi_store::{
    AccountKind, Amount, Approval, ApprovalId, ApprovalStatus, ApprovalUse, BlockKind, BlockReason,
    ChunkId, DenyReason, Judgment, KnowledgeKind, KnowledgeSource, NewApprovalRequest,
    NewKnowledgeRecord, NewProposal, NewTemplate, NewTrade, ProposalState, Rationale, RoleScore,
    SourceRef, Store, StoreError, TeamCause, TeamChange, TeamState, TemplateState, TradeId,
    TradeSide, TradeState, TradeStateChange, Transition,
};
use serde_json::json;

// ───────────── 도우미 ─────────────

fn judgment(at: i64) -> Judgment {
    Judgment {
        decided_at: t(at),
        rationale: Rationale {
            hypothesis: Some("반등".to_string()),
            trigger: Some("거래량".to_string()),
            entry: Some(Amount(70_000)),
            stop: Some(Amount(68_000)),
            target: Some(Amount(75_000)),
            expected_value: Some(Amount(1_000)),
        },
        sources: vec![SourceRef {
            name: "시세".to_string(),
            known_at: t(at),
        }],
        scores: vec![RoleScore {
            role: "시세".to_string(),
            score: 3,
            reason: "좋다".to_string(),
        }],
        consensus_score: 3,
    }
}

fn new_trade(store: &Store, at: i64) -> TradeId {
    store
        .create_trade(NewTrade {
            symbol: "005930".to_string(),
            account: AccountKind::Paper,
            at: t(at),
        })
        .unwrap()
        .id
}

fn approval(number: i64) -> Approval {
    Approval {
        number,
        content_fingerprint: format!("sha256:order-{number}"),
    }
}

fn approved(store: &Store, fingerprint: &str) -> ApprovalUse {
    let id = store
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

fn change(
    trade: TradeId,
    from: Option<TradeState>,
    to: TradeState,
    at: i64,
) -> (TradeId, Option<TradeState>, TradeState, i64) {
    let _ = TradeStateChange {
        seq: 0,
        trade,
        from,
        to,
        at: t(at),
    };
    (trade, from, to, at)
}

fn shape(changes: &[TradeStateChange]) -> Vec<(TradeId, Option<TradeState>, TradeState, i64)> {
    changes
        .iter()
        .map(|c| (c.trade, c.from, c.to, c.at.0 - t(0).0))
        .collect()
}

fn pending_template(store: &Store) -> pi_store::Template {
    let made = store
        .create_template(NewTemplate {
            value: json!({ "R%": "1" }),
            at: t(0),
        })
        .unwrap();
    store
        .revise_template(made.id, &json!({ "R%": "0.5" }), t(5))
        .unwrap();
    store
        .submit_template_for_promotion(made.id, &json!({ "매매": 31 }), t(10))
        .unwrap()
}

fn full_stop(store: &Store, at: i64) {
    store
        .change_team_state(TeamChange {
            to: TeamState::Operating,
            at: t(at - 1),
            cause: TeamCause::TradingStart,
            detail: "시험".to_string(),
            owner_approved: false,
        })
        .unwrap();
    store
        .change_team_state(TeamChange {
            to: TeamState::FullStop,
            at: t(at),
            cause: TeamCause::MaxLoss,
            detail: "시험".to_string(),
            owner_approved: false,
        })
        .unwrap();
}

fn record(record_ref: &str, chunks: &[&str], at: i64) -> NewKnowledgeRecord {
    NewKnowledgeRecord {
        kind: KnowledgeKind::Trade,
        source: KnowledgeSource::Mine,
        record_ref: record_ref.to_string(),
        chunks: chunks.iter().map(|c| (*c).to_string()).collect(),
        at: t(at),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_string()).collect()
}

// ───────────── 매매 방향 ─────────────

// 검증: M2
#[test]
fn 방향과_함께_판단하면_판단과_방향이_함께_남고_다시_열어도_그대로다() {
    let (_dir, path, store) = temp_store();
    let sell = new_trade(&store, 1);
    let buy = new_trade(&store, 2);
    let plain = new_trade(&store, 3);
    let judged = store
        .judge_trade_with_side(sell, judgment(10), TradeSide::Sell)
        .unwrap();
    assert_eq!(judged.state, TradeState::Judged);
    assert_eq!(judged.decided_at, Some(t(10)));
    store
        .judge_trade_with_side(buy, judgment(11), TradeSide::Buy)
        .unwrap();
    store.judge_trade(plain, judgment(12)).unwrap();
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.trade_side(sell).unwrap(), Some(TradeSide::Sell));
    assert_eq!(store.trade_side(buy).unwrap(), Some(TradeSide::Buy));
    assert_eq!(store.trade_side(plain).unwrap(), None);
    assert_eq!(store.get_trade(sell).unwrap(), judged);
}

// 검증: M2
#[test]
fn 거부된_판단은_방향도_남기지_않고_한_번_정한_방향은_바뀌지_않는다() {
    let (_dir, _path, store) = temp_store();
    let trade = new_trade(&store, 1);
    store
        .judge_trade_with_side(trade, judgment(10), TradeSide::Buy)
        .unwrap();
    // 이미 판단됨: 다시 판단하면 거부되고 방향도 그대로다.
    let err = store
        .judge_trade_with_side(trade, judgment(20), TradeSide::Sell)
        .unwrap_err();
    assert!(
        matches!(err, StoreError::TransitionDenied { .. }),
        "{err:?}"
    );
    assert_eq!(store.trade_side(trade).unwrap(), Some(TradeSide::Buy));

    // 판단 시각보다 늦은 자료로 판단이 거부되면 방향도 없다.
    let late = new_trade(&store, 2);
    let mut bad = judgment(30);
    bad.sources[0].known_at = t(31);
    store
        .judge_trade_with_side(late, bad, TradeSide::Sell)
        .unwrap_err();
    assert_eq!(store.trade_side(late).unwrap(), None);
    assert_eq!(store.get_trade(late).unwrap().state, TradeState::Discussing);
}

// 검증: M2
#[test]
fn 없는_매매의_방향_수량_이력은_매매_없음_오류다() {
    let (_dir, _path, store) = temp_store();
    let missing = TradeId(99);
    assert!(matches!(
        store.trade_side(missing),
        Err(StoreError::TradeNotFound(TradeId(99)))
    ));
    assert!(matches!(
        store.trade_quantity(missing),
        Err(StoreError::TradeNotFound(TradeId(99)))
    ));
    assert!(matches!(
        store.set_trade_quantity(missing, 3),
        Err(StoreError::TradeNotFound(TradeId(99)))
    ));
    assert!(matches!(
        store.trade_state_changes(missing),
        Err(StoreError::TradeNotFound(TradeId(99)))
    ));
    assert!(matches!(
        store.judge_trade_with_side(missing, judgment(1), TradeSide::Buy),
        Err(StoreError::TradeNotFound(TradeId(99)))
    ));
}

// ───────────── 매매 수량 ─────────────

// 검증: M2
#[test]
fn 수량은_승인이_붙기_전까지_두고_바꿀_수_있고_다시_열어도_그대로다() {
    let (_dir, path, store) = temp_store();
    let trade = new_trade(&store, 1);
    assert_eq!(store.trade_quantity(trade).unwrap(), None);
    store.set_trade_quantity(trade, 10).unwrap();
    assert_eq!(store.trade_quantity(trade).unwrap(), Some(10));
    store
        .judge_trade_with_side(trade, judgment(10), TradeSide::Buy)
        .unwrap();
    store.set_trade_quantity(trade, 12).unwrap();
    store
        .transition_trade(trade, TradeState::AwaitingApproval, t(20))
        .unwrap();
    store.set_trade_quantity(trade, 7).unwrap();
    let before = store.get_trade(trade).unwrap();
    drop(store);

    let store = Store::open(&path).unwrap();
    assert_eq!(store.trade_quantity(trade).unwrap(), Some(7));
    assert_eq!(store.trade_side(trade).unwrap(), Some(TradeSide::Buy));
    // 수량은 매매의 다른 칸을 바꾸지 않는다.
    assert_eq!(store.get_trade(trade).unwrap(), before);
}

// 검증: M2
#[test]
fn 영이나_너무_큰_수량은_거부되고_그대로다() {
    let (_dir, _path, store) = temp_store();
    let trade = new_trade(&store, 1);
    store.set_trade_quantity(trade, 5).unwrap();
    for bad in [0, u64::MAX, i64::MAX as u64 + 1] {
        let err = store.set_trade_quantity(trade, bad).unwrap_err();
        assert!(matches!(err, StoreError::InvalidQuantity), "{bad}: {err:?}");
    }
    assert_eq!(store.trade_quantity(trade).unwrap(), Some(5));
    store.set_trade_quantity(trade, i64::MAX as u64).unwrap();
    assert_eq!(store.trade_quantity(trade).unwrap(), Some(i64::MAX as u64));
}

// 검증: M2
#[test]
fn 승인이_붙거나_주문된_매매의_수량은_잠기고_그대로다() {
    let (_dir, _path, store) = temp_store();
    let trade = new_trade(&store, 1);
    store.judge_trade(trade, judgment(10)).unwrap();
    store.set_trade_quantity(trade, 3).unwrap();
    store
        .transition_trade(trade, TradeState::AwaitingApproval, t(20))
        .unwrap();
    store.set_trade_approval(trade, approval(1), t(21)).unwrap();
    let err = store.set_trade_quantity(trade, 4).unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::QuantityLocked {
                state: TradeState::AwaitingApproval
            }
        ),
        "{err:?}"
    );
    store
        .transition_trade(trade, TradeState::Ordered, t(30))
        .unwrap();
    let err = store.set_trade_quantity(trade, 4).unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::QuantityLocked {
                state: TradeState::Ordered
            }
        ),
        "{err:?}"
    );
    assert_eq!(store.trade_quantity(trade).unwrap(), Some(3));

    // 수량 없이 막힌 매매도 처음 두기가 거부된다.
    let blocked = new_trade(&store, 2);
    store.judge_trade(blocked, judgment(40)).unwrap();
    store
        .block_trade(
            blocked,
            BlockReason {
                kind: BlockKind::PrincipleOrRationale,
                detail: "근거 부족".to_string(),
            },
            t(41),
        )
        .unwrap();
    let err = store.set_trade_quantity(blocked, 1).unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::QuantityLocked {
                state: TradeState::Blocked
            }
        ),
        "{err:?}"
    );
    assert_eq!(store.trade_quantity(blocked).unwrap(), None);
}

// ───────────── 매매 상태 바뀜 이력 ─────────────

// 검증: M2
#[test]
fn 만들기와_모든_전이가_이력에_순서대로_남는다() {
    let (_dir, _path, store) = temp_store();
    let trade = new_trade(&store, 1);
    store.judge_trade(trade, judgment(10)).unwrap();
    store
        .transition_trade(trade, TradeState::AwaitingApproval, t(20))
        .unwrap();
    // 승인 붙이기 · 거부된 전이는 상태를 바꾸지 않으므로 이력에 없다.
    store.set_trade_approval(trade, approval(1), t(25)).unwrap();
    store
        .transition_trade(trade, TradeState::Sold, t(26))
        .unwrap_err();
    store
        .block_trade(
            trade,
            BlockReason {
                kind: BlockKind::ApprovalRejected,
                detail: "거절".to_string(),
            },
            t(30),
        )
        .unwrap();
    let changes = store.trade_state_changes(trade).unwrap();
    assert_eq!(
        shape(&changes),
        vec![
            change(trade, None, TradeState::Discussing, 1),
            change(trade, Some(TradeState::Discussing), TradeState::Judged, 10),
            change(
                trade,
                Some(TradeState::Judged),
                TradeState::AwaitingApproval,
                20
            ),
            change(
                trade,
                Some(TradeState::AwaitingApproval),
                TradeState::Blocked,
                30
            ),
        ]
    );
    for pair in changes.windows(2) {
        assert!(pair[0].seq < pair[1].seq);
    }
}

// 검증: M2
#[test]
fn 시각_범위로_모든_매매의_바뀜을_시각_순서로_읽는다() {
    let (_dir, _path, store) = temp_store();
    let a = new_trade(&store, 100);
    let b = new_trade(&store, 150);
    store.judge_trade(a, judgment(200)).unwrap();
    store.judge_trade(b, judgment(200)).unwrap();
    store
        .transition_trade(a, TradeState::AwaitingApproval, t(300))
        .unwrap();

    let day = store.trade_state_changes_between(t(150), t(300)).unwrap();
    assert_eq!(
        shape(&day),
        vec![
            change(b, None, TradeState::Discussing, 150),
            change(a, Some(TradeState::Discussing), TradeState::Judged, 200),
            change(b, Some(TradeState::Discussing), TradeState::Judged, 200),
        ]
    );
    assert_eq!(
        store
            .trade_state_changes_between(t(0), t(1_000))
            .unwrap()
            .len(),
        5
    );
    assert!(
        store
            .trade_state_changes_between(t(300), t(300))
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .trade_state_changes_between(t(400), t(100))
            .unwrap()
            .is_empty()
    );
}

// ───────────── 개선안 위험 표시 ─────────────

fn proposal(store: &Store, risky: bool) -> pi_store::Proposal {
    let a = new_trade(store, 1);
    let b = new_trade(store, 2);
    store
        .create_proposal(NewProposal {
            content: json!({ "손절": "넓힘" }),
            evidence: vec![a, b],
            increases_risk: risky,
            at: t(10),
        })
        .unwrap()
}

// 검증: M4
#[test]
fn 위험_표시를_올리면_승인_뒤_냉각중을_거친다() {
    let (_dir, _path, store) = temp_store();
    let made = proposal(&store, false);
    let marked = store.mark_proposal_risky(made.id, t(20)).unwrap();
    assert!(marked.increases_risk);
    assert_eq!(marked.state, ProposalState::Proposed);
    assert_eq!(marked.updated_at, t(20));
    assert_eq!(marked.content, made.content);
    assert_eq!(marked.evidence, made.evidence);
    // 이미 표시가 있으면 아무것도 바꾸지 않는다.
    assert_eq!(store.mark_proposal_risky(made.id, t(25)).unwrap(), marked);

    let use_ = approved(&store, "sha256:proposal");
    let after = store.approve_proposal(made.id, use_, t(30)).unwrap();
    assert_eq!(after.state, ProposalState::Cooling);
    assert_eq!(after.applied_at, None);
}

// 검증: M4
#[test]
fn 제안됨이_아닌_개선안의_위험_표시는_바꿀_수_없다() {
    let (_dir, _path, store) = temp_store();
    let rejected = proposal(&store, false);
    store.reject_proposal(rejected.id, t(20)).unwrap();
    let before = store.get_proposal(rejected.id).unwrap();
    let err = store.mark_proposal_risky(rejected.id, t(30)).unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::ProposalLocked {
                state: ProposalState::Rejected,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(store.get_proposal(rejected.id).unwrap(), before);

    let applied = proposal(&store, false);
    let use_ = approved(&store, "sha256:applied");
    store.approve_proposal(applied.id, use_, t(40)).unwrap();
    let err = store.mark_proposal_risky(applied.id, t(50)).unwrap_err();
    assert!(matches!(err, StoreError::ProposalLocked { .. }), "{err:?}");
    assert!(!store.get_proposal(applied.id).unwrap().increases_risk);
    assert!(matches!(
        store.mark_proposal_risky(pi_store::ProposalId(99), t(1)),
        Err(StoreError::ProposalNotFound(_))
    ));
}

// ───────────── 실전 승격 확인판 ─────────────

// 검증: E36 M3
#[test]
fn 확인판_승격은_판이_같고_전체_정지가_아니면_된다() {
    let (_dir, _path, store) = temp_store();
    let pending = pending_template(&store);
    assert_eq!(pending.revision, 2);
    let use_ = approved(&store, "sha256:live");
    let id = use_.id;
    let live = store
        .promote_template_live_checked(pending.id, 2, use_, t(20))
        .unwrap();
    assert_eq!(live.state, TemplateState::Live);
    assert_eq!(live.approval, Some(id));
    assert!(store.get_approval(id).unwrap().consumed_at.is_some());
}

// 검증: E36 M3
#[test]
fn 전체_정지_중이면_확인판_승격은_거부되고_템플릿도_승인도_그대로다() {
    let (_dir, _path, store) = temp_store();
    let pending = pending_template(&store);
    let use_ = approved(&store, "sha256:live");
    full_stop(&store, 15);
    let err = store
        .promote_template_live_checked(pending.id, 2, use_.clone(), t(20))
        .unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::TransitionDenied {
                transition: Transition::Template {
                    from: TemplateState::PromotionPending,
                    to: TemplateState::Live
                },
                reason: DenyReason::TeamFullStop
            }
        ),
        "{err:?}"
    );
    assert_eq!(store.get_template(pending.id).unwrap(), pending);
    assert_eq!(store.get_approval(use_.id).unwrap().consumed_at, None);
}

// 검증: E36 M3
#[test]
fn 판이_다르면_확인판_승격은_거부되고_템플릿도_승인도_그대로다() {
    let (_dir, _path, store) = temp_store();
    let pending = pending_template(&store);
    let use_ = approved(&store, "sha256:live");
    let err = store
        .promote_template_live_checked(pending.id, 1, use_.clone(), t(20))
        .unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::TemplateRevisionMismatch {
                expected: 1,
                actual: 2,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(store.get_template(pending.id).unwrap(), pending);
    assert_eq!(store.get_approval(use_.id).unwrap().consumed_at, None);
    // 승격대기가 아니면 상태를 먼저 본다.
    let fresh = store
        .create_template(NewTemplate {
            value: json!({}),
            at: t(30),
        })
        .unwrap();
    let err = store
        .promote_template_live_checked(fresh.id, 1, use_, t(31))
        .unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::TransitionDenied {
                reason: DenyReason::NotAllowed,
                ..
            }
        ),
        "{err:?}"
    );
    let _ = ApprovalId(0);
}

// ───────────── 지식: 조각 번호 · 메타 ─────────────

// 검증: D10 M7
#[test]
fn 번호로_조각을_읽고_없는_번호는_없음이다() {
    let (_dir, _path, store) = temp_store();
    let ids = store
        .index_record(record("trade-1", &["첫 조각", "둘째 조각"], 5))
        .unwrap();
    let chunk = store.knowledge_chunk(ids[1]).unwrap().unwrap();
    assert_eq!(chunk.id, ids[1]);
    assert_eq!(chunk.ordinal, 1);
    assert_eq!(chunk.text, "둘째 조각");
    assert_eq!(chunk.record_ref, "trade-1");
    assert_eq!(chunk.updated_at, t(5));
    assert_eq!(store.knowledge_chunk(ChunkId(999)).unwrap(), None);
}

// 검증: D10 M7
#[test]
fn 메타_키를_지우면_없음이_되고_없는_키는_거짓이다() {
    let (_dir, _path, store) = temp_store();
    store.set_knowledge_meta("chunk.1", "1-3").unwrap();
    store.set_knowledge_meta("model", "m").unwrap();
    assert!(store.remove_knowledge_meta("chunk.1").unwrap());
    assert_eq!(store.knowledge_meta("chunk.1").unwrap(), None);
    assert!(!store.remove_knowledge_meta("chunk.1").unwrap());
    assert_eq!(store.knowledge_meta("model").unwrap().as_deref(), Some("m"));
}

// 검증: D10 M7
#[test]
fn 조각과_메타를_한_묶음으로_올리고_바뀌면_옛_조각의_메타를_지운다() {
    let (_dir, _path, store) = temp_store();
    store.set_knowledge_meta("model", "m").unwrap();
    let first = store
        .index_record_with_meta(
            record("trade-1", &["가", "나"], 1),
            "chunk.",
            &strings(&["1-1", "2-4"]),
        )
        .unwrap();
    for (id, value) in first.iter().zip(["1-1", "2-4"]) {
        let key = format!("chunk.{}", id.0);
        assert_eq!(store.knowledge_meta(&key).unwrap().as_deref(), Some(value));
    }
    let second = store
        .index_record_with_meta(record("trade-1", &["다"], 2), "chunk.", &strings(&["1-9"]))
        .unwrap();
    for id in &first {
        assert_eq!(
            store.knowledge_meta(&format!("chunk.{}", id.0)).unwrap(),
            None
        );
    }
    assert_eq!(
        store
            .knowledge_meta(&format!("chunk.{}", second[0].0))
            .unwrap()
            .as_deref(),
        Some("1-9")
    );
    assert_eq!(store.knowledge_meta("model").unwrap().as_deref(), Some("m"));
    assert_eq!(
        store
            .knowledge_chunks(KnowledgeKind::Trade, "trade-1")
            .unwrap()
            .len(),
        1
    );
}

// 검증: D10 M7
#[test]
fn 메타_개수가_다르거나_앞붙이가_비면_아무것도_바꾸지_않는다() {
    let (_dir, _path, store) = temp_store();
    let ids = store
        .index_record_with_meta(record("trade-1", &["가"], 1), "chunk.", &strings(&["1-1"]))
        .unwrap();
    let before = store
        .knowledge_chunks(KnowledgeKind::Trade, "trade-1")
        .unwrap();
    let cases = [
        (
            record("trade-1", &["나", "다"], 2),
            "chunk.",
            strings(&["1-1"]),
        ),
        (record("trade-1", &["나"], 2), "", strings(&["1-1"])),
        (record("trade-1", &["  "], 2), "chunk.", strings(&["1-1"])),
    ];
    for (rec, prefix, values) in cases {
        let err = store
            .index_record_with_meta(rec, prefix, &values)
            .unwrap_err();
        assert!(matches!(err, StoreError::InvalidKnowledge(_)), "{err:?}");
    }
    assert_eq!(
        store
            .knowledge_chunks(KnowledgeKind::Trade, "trade-1")
            .unwrap(),
        before
    );
    assert_eq!(
        store
            .knowledge_meta(&format!("chunk.{}", ids[0].0))
            .unwrap()
            .as_deref(),
        Some("1-1")
    );
}
