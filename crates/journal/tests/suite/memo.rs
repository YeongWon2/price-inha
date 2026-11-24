//! 6차: 결정 메모 정산과 교훈("어제 메모"의 재료). 종가 · 거래일 · AI 턴 · 가리기는 가짜 포트다.

use std::collections::HashMap;
use std::sync::Mutex;

use pi_journal::{
    ClosePrices, Error, LessonInput, LessonWriter, OTHER_SYMBOL_LESSONS, Redactor,
    SAME_SYMBOL_MEMOS, SETTLE_AFTER_TRADING_DAYS, SettlePorts, TradingCalendar,
    UNSETTLEABLE_AFTER_TRADING_DAYS,
};
use pi_store::{
    DecisionMemo, DecisionMemoId, DecisionMemoState, MemoSettlement, StoreError, TradeId,
    TradeSide, TradeState,
};
use rust_decimal::Decimal;

use crate::common::{Env, MIN, T0, t};

/// 결정 날(금요일).
const DECIDED: &str = "2027-01-15";
/// 결정 날에서 4번째 다음 거래일(19일은 쉬는 날).
const FOURTH: &str = "2027-01-22";
/// 결정 날에서 5번째 다음 거래일 = 정산 날.
const SETTLE: &str = "2027-01-25";
/// 정산 날 다음 거래일.
const SETTLE_NEXT: &str = "2027-01-26";
/// 정산 날에서 4번째 다음 거래일.
const SETTLE_FOURTH: &str = "2027-01-29";
/// 정산 날에서 5번째 다음 거래일 = 정산 못 함으로 닫는 날.
const SETTLE_FIFTH: &str = "2027-02-01";

/// 거래일(19일은 쉬는 날이고 주말은 없다).
const TRADING_DAYS: [&str; 15] = [
    "2027-01-14",
    "2027-01-15",
    "2027-01-18",
    "2027-01-20",
    "2027-01-21",
    "2027-01-22",
    "2027-01-25",
    "2027-01-26",
    "2027-01-27",
    "2027-01-28",
    "2027-01-29",
    "2027-02-01",
    "2027-02-02",
    "2027-02-03",
    "2027-02-04",
];

/// 시험용 비밀 같은 글자(진짜 키 모양이 아니다).
const FAKE_SECRET: &str = "s3cr3t-가짜값";
/// 가린 자리.
const MASK: &str = "[가림]";

fn dec(units: i64, scale: u32) -> Decimal {
    Decimal::new(units, scale)
}

/// 정해 준 종가 표.
#[derive(Default)]
struct Prices {
    closes: HashMap<(String, String), Decimal>,
    benchmark: HashMap<(String, String), Decimal>,
}

impl Prices {
    fn close(mut self, symbol: &str, date: &str, value: Decimal) -> Prices {
        self.closes
            .insert((symbol.to_string(), date.to_string()), value);
        self
    }

    fn bench(mut self, symbol: &str, date: &str, value: Decimal) -> Prices {
        self.benchmark
            .insert((symbol.to_string(), date.to_string()), value);
        self
    }
}

impl ClosePrices for Prices {
    fn close(&self, symbol: &str, date: &str) -> Option<Decimal> {
        self.closes
            .get(&(symbol.to_string(), date.to_string()))
            .copied()
    }

    fn benchmark_close(&self, symbol: &str, date: &str) -> Option<Decimal> {
        self.benchmark
            .get(&(symbol.to_string(), date.to_string()))
            .copied()
    }
}

/// 정해 준 거래일 달력.
struct Calendar;

impl TradingCalendar for Calendar {
    fn next_trading_day(&self, _symbol: &str, date: &str) -> Option<String> {
        TRADING_DAYS
            .iter()
            .find(|day| **day > date)
            .map(|day| day.to_string())
    }
}

/// 정해 준 AI 답. 앞에서부터 하나씩 쓰고, 다 쓰면 `fallback`을 준다. 받은 것을 남긴다.
struct Lessons {
    script: Mutex<Vec<Option<String>>>,
    fallback: Option<String>,
    seen: Mutex<Vec<LessonInput>>,
}

impl Lessons {
    fn always(text: &str) -> Lessons {
        Lessons::script(Vec::new(), Some(text))
    }

    fn script(answers: Vec<Option<&str>>, fallback: Option<&str>) -> Lessons {
        let mut answers: Vec<Option<String>> =
            answers.into_iter().map(|a| a.map(str::to_string)).collect();
        answers.reverse();
        Lessons {
            script: Mutex::new(answers),
            fallback: fallback.map(str::to_string),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn seen(&self) -> Vec<LessonInput> {
        self.seen.lock().unwrap().clone()
    }
}

impl LessonWriter for Lessons {
    fn lesson(&self, input: &LessonInput) -> Option<String> {
        self.seen.lock().unwrap().push(input.clone());
        match self.script.lock().unwrap().pop() {
            Some(answer) => answer,
            None => self.fallback.clone(),
        }
    }
}

/// 시험용 비밀 글자를 가린다.
struct Masker;

impl Redactor for Masker {
    fn redact(&self, text: &str) -> String {
        text.replace(FAKE_SECRET, MASK)
    }
}

const LESSON: &str = "실적 기대가 맞았다. 거래량 돌파가 이어졌다. 다음에도 손절을 지킨다.";

fn ports<'a>(prices: &'a Prices, lessons: &'a Lessons) -> SettlePorts<'a> {
    SettlePorts {
        prices,
        calendar: &Calendar,
        lessons,
        redactor: &Masker,
    }
}

/// 판단된 매매와 그 결정 메모.
fn decided(env: &Env, symbol: &str, side: TradeSide) -> (TradeId, DecisionMemo) {
    let trade = env.trade_in(symbol, TradeState::Judged, T0);
    let memo = env
        .journal
        .record_decision(trade, DECIDED, side, t(T0 + MIN))
        .unwrap();
    (trade, memo)
}

fn all_memos(env: &Env) -> Vec<DecisionMemo> {
    env.store.decision_memos_page(None, 100).unwrap()
}

fn memo(env: &Env, id: DecisionMemoId) -> DecisionMemo {
    env.store.get_decision_memo(id).unwrap()
}

/// 결정 날 10,000 · 정산 날 `settled`인 가격(그 밖 날에도 값을 둔다 — 앞을 채우면 틀린 값이 나온다).
fn prices_for(symbol: &str, settled: Decimal) -> Prices {
    Prices::default()
        .close(symbol, DECIDED, dec(10_000, 0))
        .close(symbol, FOURTH, dec(10_100, 0))
        .close(symbol, SETTLE, settled)
        .close(symbol, SETTLE_NEXT, dec(11_111, 0))
        .close(symbol, "2027-01-27", dec(12_222, 0))
}

// 검증: N20 E54 (정산 5거래일 · 정산 못 함 5거래일 더 · 같은 종목 5건 · 다른 종목 3건)
#[test]
fn 정산_날수와_고르는_건수는_n20을_따른다() {
    assert_eq!(SETTLE_AFTER_TRADING_DAYS, 5);
    assert_eq!(UNSETTLEABLE_AFTER_TRADING_DAYS, 5);
    assert_eq!(SAME_SYMBOL_MEMOS, 5);
    assert_eq!(OTHER_SYMBOL_LESSONS, 3);
}

// 검증: M11 R7-4 (판단된 매매 → 결정 메모 정산 대기 하나, 두 번 불러도 하나 · 있던 것 그대로)
#[test]
fn 판단된_매매는_정산_대기_메모_하나가_되고_두_번_불러도_하나다() {
    let env = Env::new();
    let (trade, first) = decided(&env, "005930", TradeSide::Buy);
    assert_eq!(first.trade, trade);
    assert_eq!(first.date, DECIDED);
    assert_eq!(first.symbol, "005930");
    assert_eq!(first.side, TradeSide::Buy);
    assert_eq!(first.state, DecisionMemoState::Pending);
    assert_eq!(first.return_bp, None);
    assert_eq!(first.lesson, None);

    // 다른 날짜 · 판단으로 다시 불러도 새로 만들지 않고 있던 메모 그대로다.
    let again = env
        .journal
        .record_decision(trade, "2027-01-18", TradeSide::Sell, t(T0 + 2 * MIN))
        .unwrap();
    assert_eq!(again, first);
    assert_eq!(all_memos(&env), vec![first.clone()]);

    // 다시 열어도 하나다.
    let env = env.reopen();
    let after = env
        .journal
        .record_decision(trade, DECIDED, TradeSide::Buy, t(T0 + 3 * MIN))
        .unwrap();
    assert_eq!(after, first);
    assert_eq!(all_memos(&env).len(), 1);
}

// 검증: M11 (판단 전 매매 · 없는 매매는 메모를 만들지 않는다)
#[test]
fn 판단_전_매매와_없는_매매는_메모를_만들지_않는다() {
    let env = Env::new();
    let discussing = env.trade_in("005930", TradeState::Discussing, T0);
    let err = env
        .journal
        .record_decision(discussing, DECIDED, TradeSide::Buy, t(T0))
        .unwrap_err();
    assert!(
        matches!(err, Error::Store(StoreError::TradeNotJudged { .. })),
        "{err:?}"
    );
    let err = env
        .journal
        .record_decision(TradeId(9_999), DECIDED, TradeSide::Buy, t(T0))
        .unwrap_err();
    assert!(
        matches!(err, Error::Store(StoreError::TradeNotFound(_))),
        "{err:?}"
    );
    assert!(all_memos(&env).is_empty());
}

// 검증: N20 R7-4 (5거래일 전에는 시세가 있어도 정산하지 않는다)
#[test]
fn 다섯_거래일_전에는_정산하지_않는다() {
    let env = Env::new();
    let (_, created) = decided(&env, "005930", TradeSide::Buy);
    let prices = prices_for("005930", dec(10_320, 0));
    let lessons = Lessons::always(LESSON);
    for today in [DECIDED, "2027-01-19", FOURTH] {
        let closed = env
            .journal
            .settle_memos(today, &ports(&prices, &lessons), t(T0 + 10 * MIN))
            .unwrap();
        assert!(closed.is_empty(), "{today}");
    }
    assert_eq!(memo(&env, created.id), created);
    assert!(lessons.seen().is_empty());
}

// 검증: N20 M11 C25 (사기: 정산 날 당일에 정산, 수익률 · 초과수익 · 보유일 5)
#[test]
fn 사기는_정산_날에_수익률과_초과수익과_보유일로_정산된다() {
    let env = Env::new();
    let (_, created) = decided(&env, "005930", TradeSide::Buy);
    let prices = prices_for("005930", dec(10_320, 0))
        .bench("005930", DECIDED, dec(2_000, 0))
        .bench("005930", SETTLE, dec(2_040, 0));
    let lessons = Lessons::always(LESSON);
    let at = t(T0 + 20 * MIN);
    let closed = env
        .journal
        .settle_memos(SETTLE, &ports(&prices, &lessons), at)
        .unwrap();
    let stored = memo(&env, created.id);
    assert_eq!(closed, vec![stored.clone()]);
    assert_eq!(stored.state, DecisionMemoState::Settled);
    assert_eq!(stored.return_bp, Some(320));
    assert_eq!(stored.excess_bp, Some(120));
    assert_eq!(stored.holding_days, Some(5));
    assert_eq!(stored.lesson.as_deref(), Some(LESSON));
    assert_eq!(stored.settled_at, Some(at));
}

// 검증: N20 M11 E54 (팔기 뒤 내렸으면 옳은 결정 — 부호를 뒤집는다 · 정산 날 뒤 시세는 쓰지 않는다 · 지수 없으면 빈 값)
#[test]
fn 팔기_뒤_내렸으면_수익률은_양수다() {
    let env = Env::new();
    let (_, created) = decided(&env, "000660", TradeSide::Sell);
    let prices = prices_for("000660", dec(9_600, 0));
    let lessons = Lessons::always(LESSON);
    // 정산 날보다 늦게 돌려도 정산 날 종가로 셈한다.
    env.journal
        .settle_memos("2027-01-27", &ports(&prices, &lessons), t(T0 + 30 * MIN))
        .unwrap();
    let stored = memo(&env, created.id);
    assert_eq!(stored.state, DecisionMemoState::Settled);
    assert_eq!(stored.return_bp, Some(400));
    assert_eq!(stored.excess_bp, None);
    assert_eq!(stored.holding_days, Some(5));
}

// 검증: M11 N20 (기준 지수의 두 날 종가 가운데 하나라도 없으면 초과수익은 비운다)
#[test]
fn 기준_지수_시세가_모자라면_초과수익을_비운다() {
    let env = Env::new();
    let (_, created) = decided(&env, "005930", TradeSide::Buy);
    // 지수는 결정 날만 있고 정산 날은 없다(다음 날 값으로 채우지 않는다).
    let prices = prices_for("005930", dec(10_320, 0))
        .bench("005930", DECIDED, dec(2_000, 0))
        .bench("005930", SETTLE_NEXT, dec(2_100, 0));
    let lessons = Lessons::always(LESSON);
    env.journal
        .settle_memos(SETTLE, &ports(&prices, &lessons), t(T0 + 20 * MIN))
        .unwrap();
    let stored = memo(&env, created.id);
    assert_eq!(stored.state, DecisionMemoState::Settled);
    assert_eq!(stored.return_bp, Some(320));
    assert_eq!(stored.excess_bp, None);
}

// 검증: N20 (만분율 반올림은 0에서 멀어지는 쪽: +0.5bp → 1, −0.5bp → −1, 팔기는 뒤집은 값)
#[test]
fn 반올림은_0에서_멀어지는_쪽이다() {
    let env = Env::new();
    let (_, up) = decided(&env, "005930", TradeSide::Buy);
    let (_, down) = decided(&env, "000660", TradeSide::Buy);
    let (_, sell_up) = decided(&env, "035420", TradeSide::Sell);
    let prices = Prices::default()
        .close("005930", DECIDED, dec(30_000, 0))
        .close("005930", SETTLE, dec(300_015, 1))
        .close("000660", DECIDED, dec(30_000, 0))
        .close("000660", SETTLE, dec(299_985, 1))
        .close("035420", DECIDED, dec(30_000, 0))
        .close("035420", SETTLE, dec(300_015, 1));
    let lessons = Lessons::always(LESSON);
    env.journal
        .settle_memos(SETTLE, &ports(&prices, &lessons), t(T0 + 20 * MIN))
        .unwrap();
    assert_eq!(memo(&env, up.id).return_bp, Some(1));
    assert_eq!(memo(&env, down.id).return_bp, Some(-1));
    assert_eq!(memo(&env, sell_up.id).return_bp, Some(-1));
}

// 검증: E54 N20 M11 (정산 날 종가 없음 → 대기, 뒤 시세로 채우지 않음, 정산 날에서 5거래일 더 지나면 정산 못 함)
#[test]
fn 정산_날_종가가_없으면_대기하다가_다섯_거래일_뒤_정산_못_함이다() {
    let env = Env::new();
    let (_, created) = decided(&env, "005930", TradeSide::Buy);
    // 정산 날만 비었고 그 뒤 날들은 다 있다.
    let mut prices = Prices::default().close("005930", DECIDED, dec(10_000, 0));
    for day in TRADING_DAYS.iter().filter(|day| **day > SETTLE) {
        prices = prices.close("005930", day, dec(10_500, 0));
    }
    let lessons = Lessons::always(LESSON);
    for today in [SETTLE, SETTLE_NEXT, SETTLE_FOURTH] {
        let closed = env
            .journal
            .settle_memos(today, &ports(&prices, &lessons), t(T0 + 40 * MIN))
            .unwrap();
        assert!(closed.is_empty(), "{today}");
        assert_eq!(memo(&env, created.id), created, "{today}");
    }
    let at = t(T0 + 50 * MIN);
    let closed = env
        .journal
        .settle_memos(SETTLE_FIFTH, &ports(&prices, &lessons), at)
        .unwrap();
    let stored = memo(&env, created.id);
    assert_eq!(closed, vec![stored.clone()]);
    assert_eq!(stored.state, DecisionMemoState::Unsettleable);
    assert_eq!(stored.return_bp, None);
    assert_eq!(stored.excess_bp, None);
    assert_eq!(stored.holding_days, None);
    assert_eq!(stored.lesson, None);
    assert_eq!(stored.settled_at, Some(at));
    assert!(lessons.seen().is_empty());
}

// 검증: M11 R7-4 C25 (교훈은 결정 근거 · 수익률 · 초과수익 · 보유일을 넘겨 받고, 가린 뒤 저장한다)
#[test]
fn 교훈은_근거와_결과를_넘겨_받고_가린_뒤_저장된다() {
    let env = Env::new();
    let (trade, created) = decided(&env, "005930", TradeSide::Buy);
    let prices = prices_for("005930", dec(10_320, 0))
        .bench("005930", DECIDED, dec(2_000, 0))
        .bench("005930", SETTLE, dec(2_040, 0));
    let answer =
        format!("실적 기대가 맞았다. 접속 글자 {FAKE_SECRET}는 남기지 않는다. 손절을 지켰다.");
    let lessons = Lessons::always(&answer);
    env.journal
        .settle_memos(SETTLE, &ports(&prices, &lessons), t(T0 + 20 * MIN))
        .unwrap();

    let seen = lessons.seen();
    assert_eq!(seen.len(), 1);
    let input = &seen[0];
    assert_eq!(input.trade, trade);
    assert_eq!(input.symbol, "005930");
    assert_eq!(input.date, DECIDED);
    assert_eq!(input.side, TradeSide::Buy);
    assert_eq!(
        input.hypothesis.as_deref(),
        Some("실적 발표 뒤 눌림에서 반등한다")
    );
    assert_eq!(input.trigger.as_deref(), Some("거래량 두 배 돌파"));
    assert_eq!(input.return_bp, 320);
    assert_eq!(input.excess_bp, Some(120));
    assert_eq!(input.holding_days, 5);

    let lesson = memo(&env, created.id).lesson.unwrap();
    assert!(!lesson.contains(FAKE_SECRET), "{lesson}");
    assert_eq!(lesson, answer.replace(FAKE_SECRET, MASK));
}

// 검증: M11 N20 (AI가 답하지 못하거나 빈 글이면 그 메모만 대기로 남고, 다음에 같은 값으로 정산된다)
#[test]
fn 교훈을_받지_못한_메모는_대기로_남고_다음에_정산된다() {
    let env = Env::new();
    let (_, first) = decided(&env, "005930", TradeSide::Buy);
    let (_, second) = decided(&env, "000660", TradeSide::Buy);
    let (_, third) = decided(&env, "035420", TradeSide::Buy);
    let prices = prices_for("005930", dec(10_320, 0))
        .close("000660", DECIDED, dec(10_000, 0))
        .close("000660", SETTLE, dec(10_100, 0))
        .close("035420", DECIDED, dec(10_000, 0))
        .close("035420", SETTLE, dec(9_900, 0));
    // 첫째는 답 없음, 둘째는 공백뿐, 셋째는 교훈.
    let failing = Lessons::script(vec![None, Some("   "), Some(LESSON)], None);
    let closed = env
        .journal
        .settle_memos(SETTLE, &ports(&prices, &failing), t(T0 + 20 * MIN))
        .unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].id, third.id);
    assert_eq!(memo(&env, first.id), first);
    assert_eq!(memo(&env, second.id), second);
    assert_eq!(memo(&env, third.id).state, DecisionMemoState::Settled);
    // 셈은 AI를 부르기 전에 끝났다(대기로 남은 메모도 같은 값을 넘겼다).
    assert_eq!(failing.seen()[0].return_bp, 320);
    assert_eq!(failing.seen()[1].return_bp, 100);

    // 다음 날 다시 돌리면 남은 둘이 같은 값으로 정산된다(정산 날 종가 기준).
    let working = Lessons::always(LESSON);
    let at = t(T0 + 24 * 60 * MIN);
    let closed = env
        .journal
        .settle_memos(SETTLE_NEXT, &ports(&prices, &working), at)
        .unwrap();
    let ids: Vec<DecisionMemoId> = closed.iter().map(|m| m.id).collect();
    assert_eq!(ids, vec![first.id, second.id]);
    let first_now = memo(&env, first.id);
    assert_eq!(first_now.state, DecisionMemoState::Settled);
    assert_eq!(first_now.return_bp, Some(320));
    assert_eq!(first_now.settled_at, Some(at));
    assert_eq!(memo(&env, second.id).return_bp, Some(100));
    assert_eq!(working.seen().len(), 2);
}

// 검증: N20 M11 (다시 돌려도 · 다시 열어도 닫힌 메모는 그대로이고 AI를 다시 부르지 않는다)
#[test]
fn 다시_돌려도_닫힌_메모는_그대로다() {
    let env = Env::new();
    let (_, created) = decided(&env, "005930", TradeSide::Buy);
    let prices = prices_for("005930", dec(10_320, 0));
    let lessons = Lessons::always(LESSON);
    env.journal
        .settle_memos(SETTLE, &ports(&prices, &lessons), t(T0 + 20 * MIN))
        .unwrap();
    let settled = memo(&env, created.id);
    assert_eq!(settled.state, DecisionMemoState::Settled);

    let again = env
        .journal
        .settle_memos(SETTLE_NEXT, &ports(&prices, &lessons), t(T0 + 30 * MIN))
        .unwrap();
    assert!(again.is_empty());
    let env = env.reopen();
    let again = env
        .journal
        .settle_memos(SETTLE_FIFTH, &ports(&prices, &lessons), t(T0 + 40 * MIN))
        .unwrap();
    assert!(again.is_empty());
    assert_eq!(memo(&env, created.id), settled);
    assert_eq!(lessons.seen().len(), 1);
}

/// `date`에 결정하고 `settled_at`에 정산된 메모(교훈은 `lesson`). 저장소 API로 바로 닫는다.
fn settled_memo(
    env: &Env,
    symbol: &str,
    date: &str,
    lesson: &str,
    settled_at: i64,
) -> DecisionMemo {
    let trade = env.trade_in(symbol, TradeState::Judged, T0);
    let created = env
        .journal
        .record_decision(trade, date, TradeSide::Buy, t(T0))
        .unwrap();
    env.store
        .settle_decision_memo(
            created.id,
            MemoSettlement::Settled {
                return_bp: 100,
                excess_bp: None,
                holding_days: 5,
                lesson: lesson.to_string(),
            },
            t(settled_at),
        )
        .unwrap()
}

// 검증: N20 C25 (같은 종목 최근 5건(결정 + 교훈) · 다른 종목 최근 3건(교훈만) · 최근 것부터 · 정산된 것만)
#[test]
fn 다음_판단에는_같은_종목_다섯_건과_다른_종목_교훈_세_건만_고른다() {
    let env = Env::new();
    let mut same = Vec::new();
    for day in 1..=7 {
        let date = format!("2027-01-{day:02}");
        same.push(settled_memo(
            &env,
            "005930",
            &date,
            &format!("같은 종목 교훈 {day}"),
            T0 + day * MIN,
        ));
    }
    for day in 1..=5 {
        let date = format!("2027-01-{:02}", day + 10);
        settled_memo(
            &env,
            "000660",
            &date,
            &format!("다른 종목 교훈 {day}"),
            T0 + (day + 10) * MIN,
        );
    }
    // 더 최근이지만 정산 대기 · 정산 못 함인 메모는 빠진다.
    let pending = env.trade_in("005930", TradeState::Judged, T0);
    env.journal
        .record_decision(pending, "2027-01-20", TradeSide::Buy, t(T0))
        .unwrap();
    let closed = env.trade_in("000660", TradeState::Judged, T0);
    let unsettleable = env
        .journal
        .record_decision(closed, "2027-01-21", TradeSide::Sell, t(T0))
        .unwrap();
    env.store
        .settle_decision_memo(
            unsettleable.id,
            MemoSettlement::Unsettleable,
            t(T0 + 30 * MIN),
        )
        .unwrap();

    let brief = env.journal.memo_brief("005930", t(T0 + 60 * MIN)).unwrap();
    let expected: Vec<DecisionMemo> = same.iter().rev().take(5).cloned().collect();
    assert_eq!(brief.same_symbol, expected);
    assert_eq!(
        brief.other_lessons,
        vec![
            "다른 종목 교훈 5".to_string(),
            "다른 종목 교훈 4".to_string(),
            "다른 종목 교훈 3".to_string(),
        ]
    );
}

// 검증: N20 (as_of 뒤에 정산된 메모는 고르지 않는다 — 판단 시각 뒤 자료를 넣지 않는다)
#[test]
fn 판단_시각_뒤에_정산된_메모는_고르지_않는다() {
    let env = Env::new();
    let early = settled_memo(&env, "005930", "2027-01-04", "앞서 정산된 교훈", T0 + MIN);
    settled_memo(
        &env,
        "005930",
        "2027-01-05",
        "뒤에 정산된 교훈",
        T0 + 10 * MIN,
    );
    settled_memo(
        &env,
        "000660",
        "2027-01-04",
        "다른 종목 앞선 교훈",
        T0 + 2 * MIN,
    );
    settled_memo(
        &env,
        "000660",
        "2027-01-05",
        "다른 종목 뒤 교훈",
        T0 + 10 * MIN,
    );

    // 같은 시각은 넣는다.
    let brief = env.journal.memo_brief("005930", t(T0 + 2 * MIN)).unwrap();
    assert_eq!(brief.same_symbol, vec![early]);
    assert_eq!(brief.other_lessons, vec!["다른 종목 앞선 교훈".to_string()]);
}

// 검증: N20 (메모가 없으면 빈 묶음)
#[test]
fn 메모가_없으면_빈_묶음이다() {
    let env = Env::new();
    let brief = env.journal.memo_brief("005930", t(T0)).unwrap();
    assert!(brief.same_symbol.is_empty());
    assert!(brief.other_lessons.is_empty());
}
