//! 백로그 T(결정 30): 1층으로 가는 지침 후보를 개선안 후보로 읽기 · 기록된 거래일로 날 수 세기.
//!
//! - 1층 후보 글에서 RiskParams 칸 하나와 바꿀 쪽(목표 값 또는 늘림 · 줄임)이 확실히 읽히면 지금의 개선안 길
//!   (`Improvement` · 근거 매매 · 위험 표시)로 낸다. 확실히 읽히지 않으면 아무것도 만들지 않는다.
//! - 하루 정리를 돌지 않은 거래일(서버가 남긴 하루 시작 · 마감 기록)도 날 수에 넣고, 사자 판단은 그 날짜에 센다.

use pi_journal::{
    Candidate, GUIDELINE_RULE_STEP_PCT, GUIDELINE_STALE_DAYS, Guideline, GuidelineBook,
    GuidelineKind, GuidelineRules, GuidelineState, Improvement, Mistake, candidate_trades,
    rule_change_of,
};
use pi_store::{
    DecisionMemo, DecisionMemoId, DecisionMemoState, ProposalState, TradeId, TradeSide, UnixMillis,
};
use pi_template::{Change, PctRange, RiskParams};
use rust_decimal::Decimal;

use crate::common::{Env, T0, base_params, t};

const RULES: GuidelineRules = GuidelineRules::DEFAULT;

fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

fn candidate(key: &str, text: &str, basis: &[&str]) -> Candidate {
    Candidate {
        key: key.into(),
        kind: GuidelineKind::Avoid,
        text: text.into(),
        when: "같은 상황이 다시 보일 때".into(),
        symbol: None,
        basis: basis.iter().map(|b| (*b).to_string()).collect(),
    }
}

fn memo(id: i64, trade: i64) -> DecisionMemo {
    DecisionMemo {
        id: DecisionMemoId(id),
        trade: TradeId(trade),
        date: "2026-11-20".into(),
        symbol: "000660".into(),
        side: TradeSide::Buy,
        state: DecisionMemoState::Settled,
        return_bp: Some(-300),
        excess_bp: None,
        holding_days: Some(5),
        lesson: Some("교훈".into()),
        created_at: UnixMillis(T0),
        settled_at: Some(UnixMillis(T0)),
    }
}

/// 단타 기본값: 일일 손실 한도 1.5% · 고정 손절 1.5% · 종목 비중 20%.
fn params() -> RiskParams {
    base_params()
}

// ───────────── 글에서 규칙 바뀜 읽기 ─────────────

// 검증: V98 R7-2 R8 (목표 값이 적힌 1층 글: 그 칸을 그 값으로)
#[test]
fn 칸과_목표_값이_적히면_그_칸을_그_값으로_바꾸는_안이다() {
    let current = params();
    assert_eq!(
        rule_change_of("일일 손실 한도를 1%로 줄인다", &current),
        Some(Change::DailyLossLimitPct(d("1")))
    );
    assert_eq!(
        rule_change_of("종목 비중 상한을 15% 로 낮춘다", &current),
        Some(Change::SymbolWeightPct(d("15")))
    );
    let stop = rule_change_of("손절 한도를 1.2%로 좁힌다", &current);
    assert_eq!(
        stop.map(|change| match change {
            Change::Stop(rule) => rule.fixed_loss_pct,
            other => panic!("손절 칸이어야 한다: {other:?}"),
        }),
        Some(Some(PctRange {
            low: d("1.2"),
            high: d("1.2")
        }))
    );
}

// 검증: V98 R7-2 R8 (값 없이 늘림 · 줄임만 적힌 글: 지금 값의 정한 몫만큼)
#[test]
fn 값_없이_넓힌다_줄인다만_있으면_지금_값의_정한_몫만큼_움직인다() {
    assert_eq!(GUIDELINE_RULE_STEP_PCT, 10);
    let current = params();
    // 1.5%의 10% = 0.15 → 0.2(소수 한 자리, 반올림).
    let wider = rule_change_of("손절 폭을 넓힌다", &current);
    assert_eq!(
        wider.map(|change| match change {
            Change::Stop(rule) => rule.fixed_loss_pct,
            other => panic!("손절 칸이어야 한다: {other:?}"),
        }),
        Some(Some(PctRange {
            low: d("1.7"),
            high: d("1.7")
        }))
    );
    assert_eq!(
        rule_change_of("일일 손실 한도를 줄인다", &current),
        Some(Change::DailyLossLimitPct(d("1.3")))
    );
    // 20%의 10% = 2.
    assert_eq!(
        rule_change_of("종목 비중 한도를 늘린다", &current),
        Some(Change::SymbolWeightPct(d("22")))
    );
}

// 검증: V98 R7-2 R8 (확실히 읽히지 않는 글은 개선안이 되지 못한다)
#[test]
fn 칸이_둘이거나_없거나_쪽이_어긋나거나_바꿀_쪽이_없으면_읽지_않는다() {
    let current = params();
    for text in [
        "일일 손실 한도와 종목 비중을 줄인다",
        "금지 종목을 늘린다",
        "승인 없이 산다",
        "일일 손실 한도를 3%로 줄인다",
        "일일 손실 한도를 살핀다",
        "일일 손실 한도를 1%에서 2%로 바꾼다",
        "일일 손실 한도를 1.5%로 줄인다",
    ] {
        assert_eq!(rule_change_of(text, &current), None, "{text}");
    }
}

// ───────────── 개선안 내기 ─────────────

// 검증: V98 V20 R7-2 R8 (읽힌 1층 후보: 근거 매매 · 바꾸기 전 · 후 값으로 제안됨 개선안)
#[test]
fn 읽힌_실수_후보는_근거_매매와_전후_값으로_제안됨_개선안이_된다() {
    let env = Env::new();
    let trades: Vec<TradeId> = (0..3).map(|n| env.sold("005930", T0 + n)).collect();
    let basis: Vec<String> = trades.iter().map(|t| format!("review:{}", t.0)).collect();
    let basis: Vec<&str> = basis.iter().map(String::as_str).collect();
    let found = candidate(
        "mistake:일일 손실 한도를 1%로 줄인다",
        "복기에서 되풀이된 실수를 피한다: 일일 손실 한도를 1%로 줄인다",
        &basis,
    );
    let current = params();
    let proposal = env
        .journal
        .propose_from_candidate(&found, &[], &current, t(T0 + 10))
        .unwrap()
        .expect("개선안이 생긴다");
    assert_eq!(proposal.state, ProposalState::Proposed);
    assert_eq!(proposal.evidence, trades);
    assert!(!proposal.increases_risk, "좁히는 변경");
    let improvement = Improvement::from_proposal(&proposal).unwrap();
    assert_eq!(
        improvement.mistake,
        Mistake::Other("일일 손실 한도를 1%로 줄인다".into())
    );
    assert_eq!(improvement.before, current);
    assert_eq!(improvement.after.daily_loss_limit_pct, d("1"));
}

// 검증: V98 R7-2 R8 (교훈 후보: 메모의 매매가 근거 · 넓히면 위험 표시)
#[test]
fn 교훈_후보는_메모의_매매가_근거이고_넓히면_위험을_키운다고_표시된다() {
    let env = Env::new();
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0 + 1);
    let memos = vec![memo(7, a.0), memo(8, b.0)];
    assert_eq!(
        candidate_trades(
            &candidate("lesson:8", "", &["memo:7", "memo:8", "memo:9"]),
            &memos
        ),
        vec![a, b],
        "없는 메모는 빼고 메모의 매매로"
    );
    let found = candidate("lesson:8", "손절 폭을 2%로 넓힌다", &["memo:7", "memo:8"]);
    let proposal = env
        .journal
        .propose_from_candidate(&found, &memos, &params(), t(T0 + 10))
        .unwrap()
        .expect("개선안이 생긴다");
    assert_eq!(proposal.evidence, vec![a, b]);
    assert!(
        proposal.increases_risk,
        "손절을 넓히는 변경은 냉각을 거친다"
    );
    let improvement = Improvement::from_proposal(&proposal).unwrap();
    assert_eq!(
        improvement.mistake,
        Mistake::Other("손절 폭을 2%로 넓힌다".into())
    );
}

// 검증: V98 R7-2 (읽히지 않거나 근거가 모자라면 아무것도 만들지 않는다)
#[test]
fn 읽히지_않거나_근거_매매가_둘이_안_되면_개선안을_만들지_않는다() {
    let env = Env::new();
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0 + 1);
    let basis = [format!("review:{}", a.0), format!("review:{}", b.0)];
    let basis: Vec<&str> = basis.iter().map(String::as_str).collect();
    let vague = candidate("mistake:한도를 살핀다", "일일 손실 한도를 살핀다", &basis);
    assert!(
        env.journal
            .propose_from_candidate(&vague, &[], &params(), t(T0))
            .unwrap()
            .is_none()
    );
    let lone = candidate(
        "mistake:x",
        "일일 손실 한도를 1%로 줄인다",
        &[&format!("review:{}", a.0)],
    );
    assert!(
        env.journal
            .propose_from_candidate(&lone, &[], &params(), t(T0))
            .unwrap()
            .is_none()
    );
    assert!(
        env.store
            .proposals_in_state(ProposalState::Proposed)
            .unwrap()
            .is_empty()
    );
}

// 검증: V98 M4 E28 (절대 상한을 넘는 값은 개선안이 되지 못한다)
#[test]
fn 절대_상한을_넘는_값이면_개선안을_만들지_않고_거부된다() {
    let env = Env::new();
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0 + 1);
    let basis = [format!("review:{}", a.0), format!("review:{}", b.0)];
    let basis: Vec<&str> = basis.iter().map(String::as_str).collect();
    let over = candidate("mistake:x", "일일 손실 한도를 40%로 늘린다", &basis);
    assert!(
        env.journal
            .propose_from_candidate(&over, &[], &params(), t(T0))
            .is_err()
    );
    assert!(
        env.store
            .proposals_in_state(ProposalState::Proposed)
            .unwrap()
            .is_empty()
    );
}

// ───────────── 기록된 거래일로 날 수 ─────────────

fn dates(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

fn closed_once(date: &str) -> GuidelineBook {
    let mut book = GuidelineBook::default();
    assert!(book.close_days(date, &[]));
    book
}

// 검증: V100 R8 (하루 정리를 돌지 않은 기록된 거래일도 날 수에 든다)
#[test]
fn 정리를_돌지_않은_기록된_거래일도_날_수와_하루_기록에_든다() {
    let mut book = closed_once("2026-11-02");
    assert_eq!(book.day, 1);
    let recorded = dates(&[
        "2026-11-01",
        "2026-11-02",
        "2026-11-03",
        "2026-11-04",
        "2026-11-06",
    ]);
    assert!(book.close_days("2026-11-05", &recorded));
    assert_eq!(
        book.day, 4,
        "11-03 · 11-04와 오늘(11-05) — 앞 정리 날 · 그 앞 · 오늘 뒤는 빼고"
    );
    let seen: Vec<&str> = book.days.iter().map(|d| d.date.as_str()).collect();
    assert_eq!(
        seen,
        ["2026-11-02", "2026-11-03", "2026-11-04", "2026-11-05"]
    );
    assert_eq!(book.last_close.as_deref(), Some("2026-11-05"));
    assert!(
        !book.close_days("2026-11-05", &recorded),
        "같은 날은 한 번만"
    );
    assert_eq!(book.day, 4);
}

// 검증: V100 R8 (처음 정리는 오늘만 센다 — 지침이 생기기 전의 날은 세지 않는다)
#[test]
fn 처음_정리는_앞의_기록된_날을_세지_않는다() {
    let mut book = GuidelineBook::default();
    assert!(book.close_days("2026-11-05", &dates(&["2026-11-03", "2026-11-04"])));
    assert_eq!(book.day, 1);
    assert_eq!(book.days.len(), 1);
}

// 검증: V101 R8 (사자 판단은 그 날짜의 하루 기록에 든다)
#[test]
fn 사자_판단은_판단한_날짜의_하루_기록에_든다() {
    let mut book = closed_once("2026-11-02");
    book.note_decision_on(1, true, "2026-11-03");
    book.note_decision_on(2, true, "2026-11-03");
    book.note_decision_on(3, true, "2026-11-05");
    book.note_decision_on(4, false, "2026-11-05");
    assert_eq!(book.today_buys, 3, "지금까지의 셈도 그대로 는다");
    assert!(book.close_days("2026-11-05", &dates(&["2026-11-03", "2026-11-04"])));
    let tally: Vec<(&str, u32)> = book
        .days
        .iter()
        .map(|d| (d.date.as_str(), d.buys))
        .collect();
    assert_eq!(
        tally,
        [
            ("2026-11-02", 0),
            ("2026-11-03", 2),
            ("2026-11-04", 0),
            ("2026-11-05", 1)
        ]
    );
    assert_eq!(book.today_buys, 0);
    assert!(book.open_buys.is_empty());
}

// 검증: V101 R8 (날짜 없이 센 사자 판단(옛 장부)은 오늘에 든다)
#[test]
fn 날짜_없이_센_사자_판단은_오늘_기록에_든다() {
    let mut book = closed_once("2026-11-02");
    book.note_decision(1, true);
    book.note_decision(2, true);
    assert!(book.close_days("2026-11-04", &dates(&["2026-11-03"])));
    let tally: Vec<(&str, u32)> = book
        .days
        .iter()
        .map(|d| (d.date.as_str(), d.buys))
        .collect();
    assert_eq!(
        tally,
        [("2026-11-02", 0), ("2026-11-03", 0), ("2026-11-04", 2)]
    );
}

fn active(id: &str, last_used_day: u32) -> Guideline {
    Guideline {
        id: id.into(),
        key: format!("mistake:{id}"),
        kind: GuidelineKind::Avoid,
        text: format!("지침 {id}"),
        when: "때".into(),
        symbol: None,
        basis: vec!["review:1".into()],
        uses: 0,
        wins: 0,
        losses: 0,
        blocks: 0,
        created_at: T0,
        updated_at: T0,
        last_used_day,
        state: GuidelineState::Active,
        retired_why: None,
    }
}

// 검증: V100 R8 (30일 미사용은 기록된 거래일로 센다)
#[test]
fn 정리를_돌지_않은_날이_많아도_기록된_거래일로_오래_안_쓴_지침을_내린다() {
    let mut book = closed_once("2026-10-01");
    book.guidelines.push(active("g1", book.day));
    let recorded: Vec<String> = (2..=31).map(|n| format!("2026-10-{n:02}")).collect();
    assert!(book.close_days("2026-11-01", &recorded));
    assert_eq!(book.day, 1 + 30 + 1);
    let retro = book.retrospect(T0, &RULES);
    assert_eq!(retro.len(), 1, "{retro:?}");
    assert_eq!(book.guidelines[0].retired_why.as_deref(), Some("unused"));
    assert_eq!(
        book.day - book.guidelines[0].last_used_day,
        GUIDELINE_STALE_DAYS + 1
    );
}
