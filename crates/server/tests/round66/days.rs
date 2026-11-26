//! "날 수": 하루 정리를 돌지 않은 거래일도 서버가 남긴 하루 시작 · 마감 기록으로 세어 30일 미사용 · 5일 막힘을 본다.

use chrono::{Duration, NaiveDate};
use pi_journal::{DayTally, Guideline, GuidelineBook, GuidelineKind, GuidelineState};
use price_inha::KIND_GUIDELINE;
use serde_json::{Value, json};

use crate::common::{Bench, HYNIX, decision_day, events};

/// 그 날부터 `n`날(하루씩)의 `YYYY-MM-DD`.
fn run_of(first: NaiveDate, n: i64) -> Vec<String> {
    (0..n)
        .map(|k| (first + Duration::days(k)).format("%Y-%m-%d").to_string())
        .collect()
}

fn ymd(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn guideline(id: &str, kind: GuidelineKind, last_used_day: u32, blocks: u32) -> Guideline {
    Guideline {
        id: id.into(),
        key: format!("mistake:{id}"),
        kind,
        text: format!("시험 지침 {id}"),
        when: "시험 조건".into(),
        symbol: None,
        basis: vec!["review:1".into()],
        uses: 0,
        wins: 0,
        losses: 0,
        blocks,
        created_at: 0,
        updated_at: 0,
        last_used_day,
        state: GuidelineState::Active,
        retired_why: None,
    }
}

fn retired(bench: &Bench) -> Vec<Value> {
    events(&bench.store(), KIND_GUIDELINE)
        .into_iter()
        .filter(|event| event["action"] == json!("retired"))
        .collect()
}

/// 마지막 하루 정리가 결정 날보다 `gap`날 앞이고(날 수 1), 지침 하나가 그날 쓰인 장부.
fn stale_book(gap: i64) -> GuidelineBook {
    GuidelineBook {
        guidelines: vec![guideline("g1", GuidelineKind::Manage, 1, 0)],
        day: 1,
        last_close: Some(ymd(decision_day() - Duration::days(gap))),
        next_id: 1,
        ..GuidelineBook::default()
    }
}

// 검증: V100 R8 (30일 미사용은 하루 정리를 돌지 않은 기록된 거래일도 센다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 하루_정리를_돌지_않은_기록된_거래일도_세어_오래_안_쓴_지침을_내린다() {
    let bench = Bench::start().await;
    bench.write_ledger(&stale_book(40));
    // 마지막 정리 다음 날부터 30거래일이 기록돼 있고, 그동안 하루 정리는 돌지 않았다.
    bench.mark_days(&run_of(decision_day() - Duration::days(39), 30));
    bench.close_day().await;

    let found = retired(&bench);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["id"], json!("g1"));
    assert_eq!(found[0]["why"], json!("unused"));
    assert_eq!(
        bench.ledger()["day"],
        json!(1 + 30 + 1),
        "기록된 30날 + 오늘"
    );
    bench.close().await;
}

// 검증: V100 R8 (기록이 없는 날 · 적은 날은 세지 않는다 — 서버가 꺼져 있던 날)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 기록된_거래일이_적으면_오래_안_쓴_것으로_보지_않는다() {
    let bench = Bench::start().await;
    bench.write_ledger(&stale_book(40));
    bench.mark_days(&run_of(decision_day() - Duration::days(10), 5));
    bench.close_day().await;

    assert!(retired(&bench).is_empty(), "{:?}", retired(&bench));
    assert_eq!(bench.ledger()["day"], json!(1 + 5 + 1));
    bench.close().await;
}

// 검증: V101 R8 (5일 막힘도 기록된 거래일로 센다: 정리를 돌지 않은 날의 사자 판단 0이 최근 구간에 든다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 하루_정리를_돌지_않은_날도_세어_사자_판단이_줄면_가장_많이_막은_지침을_내린다() {
    let bench = Bench::start().await;
    let first = decision_day() - Duration::days(10);
    let earlier = run_of(first, 5);
    let book = GuidelineBook {
        guidelines: vec![guideline("g1", GuidelineKind::Avoid, 5, 3)],
        days: earlier
            .iter()
            .map(|date| DayTally {
                date: date.clone(),
                buys: 2,
            })
            .collect(),
        day: 5,
        last_close: earlier.last().cloned(),
        next_id: 1,
        ..GuidelineBook::default()
    };
    bench.write_ledger(&book);
    // 그 뒤 4거래일은 기록만 있고(사자 판단 없음) 오늘이 다섯째 날이다.
    bench.mark_days(&run_of(first + Duration::days(5), 4));
    bench.close_day().await;

    let found = retired(&bench);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["id"], json!("g1"));
    assert_eq!(found[0]["why"], json!("blocking_trades"));
    bench.close().await;
}

// 검증: V101 R8 (사자 판단은 판단한 날짜로 장부에 남는다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 사자_판단은_그_날짜로_장부에_남는다() {
    let bench = Bench::start().await;
    let discussed = bench.discuss(HYNIX).await.expect("논의가 끝나야 한다");
    assert_eq!(discussed.outcome.decision, pi_agent::Decision::Buy);
    let ledger = bench.ledger();
    assert_eq!(
        ledger["open_buys"],
        json!([{ "date": ymd(decision_day()), "buys": 1 }]),
        "{ledger}"
    );
    assert_eq!(ledger["today_buys"], json!(1), "{ledger}");
    bench.close().await;
}
