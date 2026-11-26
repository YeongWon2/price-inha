//! 기능 L(결정 30): 2층 팀 지침의 순수 계산 — 승격 문턱 · 1층 내용 거르기 · 성적 · 내림 · 합침 · 막힘 풀기 · 상한 고르기.
//! 번호(V97~V103)는 이 차수에서 설계서에 더할 제안 번호다(조정자가 dev.md에 적는다).

use pi_journal::{
    Candidate, ClosedTrade, GUIDELINE_BLOCK_MAX_CHARS, GUIDELINE_BLOCK_MAX_ITEMS,
    GUIDELINE_PROMOTE_REPEATS, GUIDELINE_RETIRE_MIN_USES, GUIDELINE_RETIRE_WIN_PCT,
    GUIDELINE_STALE_DAYS, GUIDELINE_TEXT_MAX_CHARS, GUIDELINE_UNBLOCK_DAYS, Guideline,
    GuidelineAction, GuidelineBook, GuidelineKind, GuidelineRules, GuidelineState, Mistake,
    PersonRetire, ReviewRecord, guideline_candidates, similarity_pct, touches_layer_one,
};
use pi_store::{DecisionMemo, DecisionMemoId, DecisionMemoState, TradeId, TradeSide, UnixMillis};
use rust_decimal::Decimal;

const RULES: GuidelineRules = GuidelineRules::DEFAULT;
const NOW: i64 = 1_800_000_000_000;

fn review(trade: i64, mistakes: Vec<Mistake>) -> ReviewRecord {
    ReviewRecord {
        trade: TradeId(trade),
        mistakes,
        note: "복기".into(),
        at: UnixMillis(NOW),
    }
}

fn memo(id: i64, symbol: &str, return_bp: i64, lesson: &str) -> DecisionMemo {
    DecisionMemo {
        id: DecisionMemoId(id),
        trade: TradeId(100 + id),
        date: "2026-11-20".into(),
        symbol: symbol.into(),
        side: TradeSide::Buy,
        state: DecisionMemoState::Settled,
        return_bp: Some(return_bp),
        excess_bp: None,
        holding_days: Some(5),
        lesson: Some(lesson.into()),
        created_at: UnixMillis(NOW),
        settled_at: Some(UnixMillis(NOW)),
    }
}

fn closed(trade: i64, trigger: &str, pnl: i64) -> ClosedTrade {
    ClosedTrade {
        trade,
        symbol: "005930".into(),
        trigger: Some(trigger.into()),
        pnl: Decimal::from(pnl),
    }
}

fn guideline(id: &str, kind: GuidelineKind, text: &str, when: &str) -> Guideline {
    Guideline {
        id: id.into(),
        key: format!("test:{id}"),
        kind,
        text: text.into(),
        when: when.into(),
        symbol: None,
        basis: vec![format!("review:{id}")],
        uses: 0,
        wins: 0,
        losses: 0,
        blocks: 0,
        created_at: NOW,
        updated_at: NOW,
        last_used_day: 0,
        state: GuidelineState::Active,
        retired_why: None,
    }
}

fn book_of(list: Vec<Guideline>) -> GuidelineBook {
    GuidelineBook {
        guidelines: list,
        ..GuidelineBook::default()
    }
}

fn find<'a>(book: &'a GuidelineBook, id: &str) -> &'a Guideline {
    book.guidelines.iter().find(|g| g.id == id).unwrap()
}

fn chased(n: i64) -> Vec<ReviewRecord> {
    (1..=n)
        .map(|trade| review(trade, vec![Mistake::ChasedEntry]))
        .collect()
}

// ───────────── 승격 ─────────────

// 검증: V97 R7-1 R8 (제안 번호 · 결정 30: 같은 실수가 문턱 이상이면 피하기 지침 후보, 문턱 아래면 없다)
#[test]
fn 같은_실수_유형이_문턱만큼_되풀이되면_피하기_지침_후보가_된다() {
    assert_eq!(GUIDELINE_PROMOTE_REPEATS, 3);
    let below = guideline_candidates(&chased(2), &[], &[], &RULES);
    assert!(below.is_empty(), "두 번은 아직 아니다: {below:?}");
    let list = guideline_candidates(&chased(3), &[], &[], &RULES);
    assert_eq!(list.len(), 1, "{list:?}");
    let one = &list[0];
    assert_eq!(one.kind, GuidelineKind::Avoid);
    assert_eq!(one.key, "mistake:chased_entry");
    assert_eq!(one.basis, vec!["review:1", "review:2", "review:3"]);
    assert!(!one.text.is_empty() && !one.when.is_empty());
    assert!(one.text.chars().count() <= GUIDELINE_TEXT_MAX_CHARS);
    assert!(
        !touches_layer_one(&one.text),
        "틀 글은 1층을 건드리지 않는다"
    );
}

// 검증: V97 R8 (올리면 활성 지침 · 같은 후보는 다시 올리지 않고 근거가 늘면 고친다)
#[test]
fn 후보를_올리면_활성_지침이_되고_근거가_늘면_고친다() {
    let mut book = GuidelineBook::default();
    let first = book.promote(
        guideline_candidates(&chased(3), &[], &[], &RULES),
        NOW,
        &RULES,
    );
    assert_eq!(first.changes.len(), 1);
    assert_eq!(first.changes[0].action, GuidelineAction::Promoted);
    assert_eq!(first.changes[0].why, "mistake");
    assert_eq!(book.guidelines.len(), 1);
    let id = book.guidelines[0].id.clone();
    assert!(book.guidelines[0].is_active());
    assert_eq!(book.guidelines[0].created_at, NOW);

    let again = book.promote(
        guideline_candidates(&chased(3), &[], &[], &RULES),
        NOW + 1,
        &RULES,
    );
    assert!(again.changes.is_empty(), "같은 후보는 바뀐 것이 없다");
    assert_eq!(book.guidelines.len(), 1);

    let more = book.promote(
        guideline_candidates(&chased(4), &[], &[], &RULES),
        NOW + 2,
        &RULES,
    );
    assert_eq!(more.changes.len(), 1);
    assert_eq!(more.changes[0].action, GuidelineAction::Revised);
    assert_eq!(more.changes[0].id, id);
    assert_eq!(find(&book, &id).basis.len(), 4);
    assert_eq!(find(&book, &id).updated_at, NOW + 2);
}

// 검증: V97 R8 (내린 지침의 열쇠는 다시 올리지 않는다)
#[test]
fn 내린_지침은_같은_후보로_다시_올라오지_않는다() {
    let mut book = GuidelineBook::default();
    book.promote(
        guideline_candidates(&chased(3), &[], &[], &RULES),
        NOW,
        &RULES,
    );
    let id = book.guidelines[0].id.clone();
    assert!(matches!(
        book.retire_by_person(&id, NOW + 1),
        PersonRetire::Retired(_)
    ));
    let again = book.promote(
        guideline_candidates(&chased(5), &[], &[], &RULES),
        NOW + 2,
        &RULES,
    );
    assert!(again.changes.is_empty());
    assert_eq!(book.guidelines.len(), 1);
    assert_eq!(book.guidelines[0].state, GuidelineState::Retired);
}

// 검증: V97 R7-4 R8 (비슷한 교훈이 문턱 이상이면 지침, 모두 같은 종목이면 그 종목에만, 대개 손실이면 피하기)
#[test]
fn 비슷한_교훈이_세_번이면_지침이_되고_같은_종목이면_그_종목에만_든다() {
    let memos = vec![
        memo(
            1,
            "000660",
            -300,
            "실적 발표 직전에 사면 변동이 커서 손해를 봤다. 다음엔 기다린다.",
        ),
        memo(
            2,
            "000660",
            -150,
            "실적 발표 직전에 사면 변동이 커서 손해를 봤다.",
        ),
        memo(
            3,
            "000660",
            -80,
            "실적 발표 직전에 사면 변동이 커서 손해를 보았다. 기다리자.",
        ),
        memo(
            4,
            "000660",
            500,
            "외국인 수급이 좋아 오래 들고 간 것이 맞았다.",
        ),
    ];
    let list = guideline_candidates(&[], &memos, &[], &RULES);
    assert_eq!(list.len(), 1, "{list:?}");
    let one = &list[0];
    assert_eq!(one.key, "lesson:1");
    assert_eq!(one.kind, GuidelineKind::Avoid);
    assert_eq!(one.symbol.as_deref(), Some("000660"));
    assert_eq!(one.basis, vec!["memo:1", "memo:2", "memo:3"]);
    assert!(one.text.contains("실적 발표 직전"), "{}", one.text);
    assert!(!one.text.contains("다음엔"), "첫 문장만 쓴다: {}", one.text);
}

// 검증: V97 R8 (익절로 끝난 매매들의 비슷한 계기가 문턱 이상이면 들어가기 지침, 손절이 더 많으면 아니다)
#[test]
fn 익절로_끝난_매매의_같은_계기가_세_번이면_들어가기_지침이_된다() {
    let wins = vec![
        closed(1, "새 공시: 자사주 매입 결정", 5_000),
        closed(2, "새 공시: 자사주 매입 결정", 3_000),
        closed(3, "새 공시: 자사주 매입 결정", 1_000),
        closed(4, "새 공시: 유상증자 결정", -2_000),
    ];
    let list = guideline_candidates(&[], &[], &wins, &RULES);
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(list[0].kind, GuidelineKind::Enter);
    assert_eq!(list[0].key, "enter:1");
    assert_eq!(list[0].basis, vec!["trade:1", "trade:2", "trade:3"]);
    assert!(list[0].when.contains("자사주 매입"), "{}", list[0].when);

    let mut mixed = wins.clone();
    for trade in 10..14 {
        mixed.push(closed(trade, "새 공시: 자사주 매입 결정", -1_000));
    }
    let none = guideline_candidates(&[], &[], &mixed, &RULES);
    assert!(none.is_empty(), "손절이 더 많은 계기는 아니다: {none:?}");
}

// ───────────── 1층 거르기 ─────────────

// 검증: V98 R7-2 R7-3 R14-1 (한도 · 금지 · 승인 길을 바꾸는 글을 알아본다)
#[test]
fn 숫자_한도_종목_지정_승인_우회는_1층을_건드리는_글이다() {
    for text in [
        "한 종목 비중을 30% 넘기지 않는다",
        "하루 손실 한도를 늘린다",
        "005930은 사지 않는다",
        "급할 때는 승인 없이 산다",
        "이 종목은 매수 금지",
        "손절 상한을 바꾼다",
        "100만원 이상은 사지 않는다",
    ] {
        assert!(touches_layer_one(text), "{text}");
    }
    for text in [
        "계기 직후 이미 크게 오른 종목을 뒤따라 사지 않는다",
        "새 공시: 2026-11-21 자사주 매입 결정",
        "가설과 계기가 분명하지 않으면 들어가지 않는다",
    ] {
        assert!(!touches_layer_one(text), "{text}");
    }
}

// 검증: V98 R7-2 R8 (1층을 건드리는 후보는 지침이 되지 못하고 1층 개선안 길로 간다)
#[test]
fn 한도를_바꾸라는_되풀이는_지침이_되지_못하고_1층_길로_간다() {
    let reviews: Vec<ReviewRecord> = (1..=3)
        .map(|trade| {
            review(
                trade,
                vec![Mistake::Other("손절 한도를 3%로 줄인다".into())],
            )
        })
        .collect();
    let list = guideline_candidates(&reviews, &[], &[], &RULES);
    assert_eq!(list.len(), 1, "후보는 생긴다: {list:?}");
    let mut book = GuidelineBook::default();
    let promotion = book.promote(list, NOW, &RULES);
    assert!(promotion.changes.is_empty());
    assert_eq!(promotion.to_rules.len(), 1);
    assert!(book.guidelines.is_empty(), "장부에 오르지 않는다");
}

// ───────────── 성적 ─────────────

// 검증: V99 R8 (판단 글에 넣은 매매가 끝나면 쓰임 · 익절 · 손절을 센다)
#[test]
fn 넣은_매매가_끝나면_쓰임_익절_손절을_센다() {
    let mut book = book_of(vec![
        guideline("g1", GuidelineKind::Enter, "들어간다", "언제나"),
        guideline("g2", GuidelineKind::Avoid, "피한다", "언제나"),
    ]);
    let both = vec!["g1".to_string(), "g2".to_string()];
    for trade in [1, 2, 3] {
        book.note_injected(trade, &both);
        book.note_decision(trade, true);
    }
    assert_eq!(book.today_buys, 3);
    let done = book.settle(
        &[
            (1, Decimal::from(500)),
            (2, Decimal::from(-300)),
            (3, Decimal::ZERO),
            (9, Decimal::from(10)),
        ],
        NOW + 5,
    );
    assert_eq!(done, vec![1, 2, 3]);
    assert!(book.pending.is_empty());
    let g1 = find(&book, "g1");
    assert_eq!((g1.uses, g1.wins, g1.losses), (3, 1, 1));
    assert_eq!(g1.win_rate_pct(), Some(33));
    assert_eq!(g1.updated_at, NOW + 5);
    assert_eq!(find(&book, "g2").uses, 3);
}

// 검증: V99 V102 R8 (사지 않고 끝난 논의는 쓰임에 들지 않고, 넣었던 피하기 지침의 막음만 는다)
#[test]
fn 사지_않은_논의는_쓰임이_아니고_피하기_지침의_막음만_는다() {
    let mut book = book_of(vec![
        guideline("g1", GuidelineKind::Enter, "들어간다", "언제나"),
        guideline("g2", GuidelineKind::Avoid, "피한다", "언제나"),
    ]);
    book.note_injected(7, &["g1".to_string(), "g2".to_string()]);
    book.note_decision(7, false);
    assert!(book.pending.is_empty());
    assert_eq!(book.today_buys, 0);
    assert_eq!(find(&book, "g2").blocks, 1);
    assert_eq!(find(&book, "g1").blocks, 0);
    assert!(book.settle(&[(7, Decimal::from(100))], NOW).is_empty());
    assert_eq!(find(&book, "g1").uses, 0);
}

// ───────────── 회고 ─────────────

// 검증: V100 R8 (쓰임이 충분한데 익절 비율이 낮으면 내린다)
#[test]
fn 쓰임이_충분한데_익절_비율이_낮으면_내린다() {
    assert_eq!(
        (GUIDELINE_RETIRE_MIN_USES, GUIDELINE_RETIRE_WIN_PCT),
        (5, 40)
    );
    let mut poor = guideline("g1", GuidelineKind::Avoid, "가 지침", "가");
    (poor.uses, poor.wins, poor.losses) = (5, 1, 4);
    let mut fair = guideline("g2", GuidelineKind::Avoid, "나 지침", "나");
    (fair.uses, fair.wins, fair.losses) = (5, 2, 3);
    let mut young = guideline("g3", GuidelineKind::Avoid, "다 지침", "다");
    (young.uses, young.wins, young.losses) = (4, 0, 4);
    let mut book = book_of(vec![poor, fair, young]);
    let changes = book.retrospect(NOW + 9, &RULES);
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(changes[0].action, GuidelineAction::Retired);
    assert_eq!(changes[0].id, "g1");
    assert_eq!(changes[0].why, "poor_win_rate");
    let g1 = find(&book, "g1");
    assert_eq!(g1.state, GuidelineState::Retired);
    assert_eq!(g1.retired_why.as_deref(), Some("poor_win_rate"));
    assert_eq!(g1.updated_at, NOW + 9);
    assert!(find(&book, "g2").is_active() && find(&book, "g3").is_active());
}

// 검증: V100 R8 (오래 쓰이지 않은 지침은 내린다 · 하루 정리는 날마다 한 번만 날 수를 늘린다)
#[test]
fn 오래_쓰이지_않은_지침은_내린다() {
    assert_eq!(GUIDELINE_STALE_DAYS, 30);
    let mut book = book_of(vec![
        guideline("g1", GuidelineKind::Manage, "오래된 지침", "가"),
        guideline("g2", GuidelineKind::Manage, "자주 쓰는 지침", "나"),
    ]);
    assert!(book.close_day("2026-01-01"));
    assert!(!book.close_day("2026-01-01"), "같은 날은 한 번만");
    assert_eq!(book.day, 1);
    for n in 2..=30 {
        assert!(book.close_day(&format!("2026-02-{n:02}")));
        book.note_injected(n, &["g2".to_string()]);
        book.note_decision(n, false);
    }
    assert_eq!(book.day, 30);
    assert_eq!(find(&book, "g2").last_used_day, 30);
    let changes = book.retrospect(NOW, &RULES);
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(
        (changes[0].id.as_str(), changes[0].why.as_str()),
        ("g1", "unused")
    );
    assert!(find(&book, "g2").is_active());
}

// 검증: V101 R8 (같은 종류 · 같은 조건 · 글이 거의 같은 지침은 먼저 만든 쪽으로 합친다)
#[test]
fn 겹치는_지침은_먼저_만든_쪽으로_합친다() {
    let mut first = guideline(
        "g1",
        GuidelineKind::Avoid,
        "급등 뒤 추격 매수를 하지 않는다",
        "급등 뒤",
    );
    (first.uses, first.wins) = (2, 1);
    let mut second = guideline(
        "g2",
        GuidelineKind::Avoid,
        "급등 뒤 추격 매수를 하지 않는다.",
        "급등 뒤",
    );
    (second.uses, second.wins, second.losses, second.blocks) = (2, 2, 0, 3);
    second.basis = vec!["memo:9".into()];
    let other_kind = guideline(
        "g3",
        GuidelineKind::Manage,
        "급등 뒤 추격 매수를 하지 않는다",
        "급등 뒤",
    );
    let other_when = guideline(
        "g4",
        GuidelineKind::Avoid,
        "급등 뒤 추격 매수를 하지 않는다",
        "장 막판",
    );
    let mut book = book_of(vec![first, second, other_kind, other_when]);
    assert!(
        similarity_pct(
            "급등 뒤 추격 매수를 하지 않는다",
            "급등 뒤 추격 매수를 하지 않는다."
        ) >= 80
    );
    let changes = book.retrospect(NOW + 3, &RULES);
    let merged: Vec<_> = changes
        .iter()
        .filter(|c| c.action == GuidelineAction::Merged)
        .collect();
    assert_eq!(merged.len(), 1, "{changes:?}");
    assert_eq!(merged[0].id, "g2");
    assert_eq!(merged[0].into.as_deref(), Some("g1"));
    let g1 = find(&book, "g1");
    assert_eq!((g1.uses, g1.wins, g1.losses, g1.blocks), (4, 3, 0, 3));
    assert!(
        g1.basis.contains(&"memo:9".to_string()) && g1.basis.contains(&"review:g1".to_string())
    );
    assert_eq!(find(&book, "g2").retired_why.as_deref(), Some("merged"));
    assert!(find(&book, "g3").is_active() && find(&book, "g4").is_active());
}

/// 하루 정리를 `days`번(날마다 `buys`번 사기) 한다.
fn days_with(book: &mut GuidelineBook, start: u32, buys: &[u32]) {
    for (offset, count) in buys.iter().enumerate() {
        for _ in 0..*count {
            book.today_buys += 1;
        }
        let day = start + offset as u32;
        assert!(book.close_day(&format!("2026-03-{day:02}")));
    }
}

// 검증: V102 R8 (매매가 막히지 않게: 사자 판단이 크게 줄면 가장 많이 막은 피하기 지침부터 하나 내린다)
#[test]
fn 사자_판단이_크게_줄면_가장_많이_막은_피하기_지침부터_하나_내린다() {
    assert_eq!(GUIDELINE_UNBLOCK_DAYS, 5);
    let mut few = guideline("g1", GuidelineKind::Avoid, "가 피하기", "가");
    few.blocks = 2;
    let mut most = guideline("g2", GuidelineKind::Avoid, "나 피하기", "나");
    most.blocks = 6;
    let mut enter = guideline("g3", GuidelineKind::Enter, "다 들어가기", "다");
    enter.blocks = 9;
    let mut book = book_of(vec![few, most, enter]);
    days_with(&mut book, 1, &[2, 2, 2, 2, 2, 1, 0, 0, 1, 0]);
    let changes = book.retrospect(NOW, &RULES);
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(
        (changes[0].id.as_str(), changes[0].why.as_str()),
        ("g2", "blocking_trades")
    );
    assert!(find(&book, "g1").is_active() && find(&book, "g3").is_active());
}

// 검증: V102 R8 (줄지 않았거나 앞 구간이 너무 적으면 막힘 풀기를 하지 않는다)
#[test]
fn 사자_판단이_줄지_않았으면_막힘_풀기를_하지_않는다() {
    let mut avoid = guideline("g1", GuidelineKind::Avoid, "가 피하기", "가");
    avoid.blocks = 4;
    let mut steady = book_of(vec![avoid.clone()]);
    days_with(&mut steady, 1, &[2, 2, 2, 2, 2, 1, 1, 1, 1, 1]);
    assert!(
        steady.retrospect(NOW, &RULES).is_empty(),
        "절반이면 아직이다"
    );
    let mut thin = book_of(vec![avoid]);
    days_with(&mut thin, 1, &[1, 0, 1, 0, 1, 0, 0, 0, 0, 0]);
    assert!(
        thin.retrospect(NOW, &RULES).is_empty(),
        "앞 구간이 적으면 견주지 않는다"
    );
}

// ───────────── 고르기 ─────────────

// 검증: V103 R8 (판단 글에 넣는 지침은 활성 · 종목이 맞는 것을 성적 순으로, 개수 상한 안에서만)
#[test]
fn 판단_글에_넣는_지침은_성적_순으로_개수_상한_안에서만_고른다() {
    assert_eq!(GUIDELINE_BLOCK_MAX_ITEMS, 7);
    let mut list = Vec::new();
    for n in 0..10u32 {
        let mut g = guideline(
            &format!("g{n}"),
            GuidelineKind::Manage,
            &format!("지침 {n}"),
            "언제나",
        );
        (g.uses, g.wins) = (10, n);
        list.push(g);
    }
    let mut elsewhere = guideline("gx", GuidelineKind::Manage, "다른 종목 지침", "언제나");
    elsewhere.symbol = Some("000660".into());
    (elsewhere.uses, elsewhere.wins) = (10, 10);
    let mut off = guideline("gr", GuidelineKind::Manage, "내린 지침", "언제나");
    off.state = GuidelineState::Retired;
    (off.uses, off.wins) = (10, 10);
    list.push(elsewhere);
    list.push(off);
    let book = book_of(list);
    let picked: Vec<&str> = book
        .select("005930", &RULES)
        .iter()
        .map(|g| g.id.as_str())
        .collect();
    assert_eq!(picked, vec!["g9", "g8", "g7", "g6", "g5", "g4", "g3"]);
    let here: Vec<&str> = book
        .select("000660", &RULES)
        .iter()
        .map(|g| g.id.as_str())
        .collect();
    assert_eq!(here[0], "gx", "그 종목 지침도 든다");
    assert!(!here.contains(&"gr"));
}

// 검증: V103 R8 (지침 줄의 글자 합은 상한을 넘지 않는다: 넘는 지침은 건너뛴다)
#[test]
fn 판단_글에_넣는_지침_줄의_글자_합은_상한을_넘지_않는다() {
    assert_eq!(GUIDELINE_BLOCK_MAX_CHARS, 1_200);
    let long = "가".repeat(GUIDELINE_TEXT_MAX_CHARS);
    let mut list = Vec::new();
    for n in 0..7u32 {
        let mut g = guideline(
            &format!("g{n}"),
            GuidelineKind::Avoid,
            &long,
            &"나".repeat(100),
        );
        (g.uses, g.wins) = (10, 10 - n);
        list.push(g);
    }
    let mut short = guideline("gs", GuidelineKind::Enter, "짧은 지침", "언제나");
    (short.uses, short.wins) = (10, 0);
    list.push(short);
    let book = book_of(list);
    let picked = book.select("005930", &RULES);
    let total: usize = picked.iter().map(|g| g.line().chars().count()).sum();
    assert!(total <= GUIDELINE_BLOCK_MAX_CHARS, "{total}");
    assert!(picked.len() < 7, "긴 지침은 다 들지 못한다");
    assert_eq!(
        picked.last().unwrap().id,
        "gs",
        "넘는 것은 건너뛰고 짧은 것은 든다"
    );
}

// 검증: V103 R8 (지침 한 줄의 모양과 익절 비율)
#[test]
fn 지침_한_줄은_종류_글_조건이다() {
    let mut g = guideline(
        "g1",
        GuidelineKind::Avoid,
        "추격 매수를 하지 않는다",
        "급등 뒤",
    );
    assert_eq!(g.line(), "[피하기] 추격 매수를 하지 않는다 (언제: 급등 뒤)");
    assert_eq!(g.win_rate_pct(), None);
    (g.uses, g.wins) = (4, 3);
    assert_eq!(g.win_rate_pct(), Some(75));
}

// ───────────── 사람 · 장부 ─────────────

// 검증: V104 R8 (사람이 끄면 내리고, 없는 번호 · 이미 내린 것은 바뀐 것이 없다)
#[test]
fn 사람이_끄면_내리고_다시_끄면_바뀐_것이_없다() {
    let mut book = book_of(vec![guideline("g1", GuidelineKind::Avoid, "가", "가")]);
    match book.retire_by_person("g1", NOW + 7) {
        PersonRetire::Retired(change) => {
            assert_eq!(change.action, GuidelineAction::Retired);
            assert_eq!(change.why, "person");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(find(&book, "g1").retired_why.as_deref(), Some("person"));
    assert_eq!(
        book.retire_by_person("g1", NOW + 8),
        PersonRetire::AlreadyRetired
    );
    assert_eq!(book.retire_by_person("g9", NOW + 8), PersonRetire::NotFound);
}

// 검증: V104 (장부는 JSON으로 오가도 같고, 종류 · 상태는 소문자 글자다)
#[test]
fn 장부는_json으로_오가도_같다() {
    let mut book = GuidelineBook::default();
    book.promote(
        guideline_candidates(&chased(3), &[], &[], &RULES),
        NOW,
        &RULES,
    );
    book.note_injected(3, &[book.guidelines[0].id.clone()]);
    let text = serde_json::to_string(&book).unwrap();
    assert!(
        text.contains("\"kind\":\"avoid\"") && text.contains("\"state\":\"active\""),
        "{text}"
    );
    let back: GuidelineBook = serde_json::from_str(&text).unwrap();
    assert_eq!(back, book);
    let empty: GuidelineBook = serde_json::from_str("{}").unwrap();
    assert_eq!(empty, GuidelineBook::default());
}

// 검증: V97 (고정 틀의 후보를 손으로 만들어도 같은 길로 오른다)
#[test]
fn 손으로_만든_후보도_같은_길로_오른다() {
    let mut book = GuidelineBook::default();
    let candidate = Candidate {
        key: "lesson:5".into(),
        kind: GuidelineKind::Manage,
        text: "실적 발표 직전에는 비중을 작게 시작한다".into(),
        when: "사기 판단을 할 때".into(),
        symbol: None,
        basis: vec!["memo:5".into()],
    };
    let promotion = book.promote(vec![candidate], NOW, &RULES);
    assert_eq!(promotion.changes.len(), 1);
    assert_eq!(promotion.changes[0].why, "lesson");
    let ids: Vec<&str> = book.guidelines.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(ids, vec!["g1"]);
}
