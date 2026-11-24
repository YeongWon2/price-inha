//! 29차 1번: 매매 목록 · 매매 하나(C30 · V83 · R6-3 · F7-7).

use pi_store::{FactValue, HISTORY_PAGE_MAX, NewFact, TradeId, TradeState, UnixMillis};
use price_inha::{
    APPROVAL_LOG_PATH, DAILY_SUMMARY_PATH, HISTORY_DEFAULT_MAX, TRADE_BASIS_PATH, TRADES_PATH,
};
use serde_json::{Value, json};

use crate::common::{
    Bench, ENTRY, HYNIX, HYPOTHESIS, NAVER, SCORE_REASON, SCORE_ROLE, SOURCE_NAME, TRIGGER, d, get,
    get_anonymous, get_with, ids, kst, monday, next_of, num, trade_path,
};

/// 근거에 붙일 자료 이름.
const FACT_LABEL: &str = "주요사항보고서(자기주식취득결정)";

async fn start() -> Bench {
    Bench::start(kst(monday(), 10, 0)).await
}

/// 매매 목록을 읽는다(200이어야 한다).
async fn page(bench: &Bench, query: &str) -> Value {
    let reply = get(bench.addr(), &format!("{TRADES_PATH}{query}")).await;
    assert_eq!(reply.status, 200, "{query}: {}", reply.body);
    reply.body
}

/// 논의중 매매 `count`개(1밀리초씩 늦게). 만든 차례의 번호.
fn many(bench: &Bench, count: i64) -> Vec<TradeId> {
    (0..count)
        .map(|i| bench.discussing(if i % 2 == 0 { HYNIX } else { NAVER }, i))
        .collect()
}

fn desc(list: &[TradeId]) -> Vec<i64> {
    let mut out: Vec<i64> = list.iter().map(|id| id.0).collect();
    out.sort_unstable_by(|a, b| b.cmp(a));
    out
}

// 검증: V83 C30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 빈_저장소면_매매_목록이_비고_다음_쪽이_없다() {
    let bench = start().await;
    let body = page(&bench, "").await;
    assert_eq!(body["trades"], json!([]), "{body}");
    assert_eq!(body["next"], Value::Null, "{body}");
    bench.close().await;
}

// 검증: V83 C30 R6-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 매매_목록은_최근_것부터이고_max와_next로_겹침_빠짐_없이_넘긴다() {
    let bench = start().await;
    let made = many(&bench, 5);
    let all = desc(&made);

    let first = page(&bench, "?max=2").await;
    assert_eq!(ids(&first, "trades"), all[0..2].to_vec(), "{first}");
    assert_eq!(next_of(&first), Some(all[1]), "{first}");
    let second = page(&bench, &format!("?max=2&before={}", all[1])).await;
    assert_eq!(ids(&second, "trades"), all[2..4].to_vec(), "{second}");
    assert_eq!(next_of(&second), Some(all[3]), "{second}");
    let third = page(&bench, &format!("?max=2&before={}", all[3])).await;
    assert_eq!(ids(&third, "trades"), all[4..5].to_vec(), "{third}");
    assert_eq!(next_of(&third), None, "마지막 쪽: {third}");

    // 쪽이 꽉 찼어도 더 없으면 next는 null이다.
    let whole = page(&bench, "?max=5").await;
    assert_eq!(ids(&whole, "trades"), all, "{whole}");
    assert_eq!(next_of(&whole), None, "더 없다: {whole}");
    let four = page(&bench, "?max=4").await;
    assert_eq!(next_of(&four), Some(all[3]), "{four}");
    let rest = page(&bench, &format!("?max=4&before={}", all[3])).await;
    assert_eq!(ids(&rest, "trades"), vec![all[4]], "{rest}");
    assert_eq!(next_of(&rest), None, "{rest}");

    // 줄의 모양: 번호 · 종목 · 계좌 · 상태 · 만든 · 바뀐 · 판단 시각.
    let oldest = made[0];
    let saved = bench.store().get_trade(oldest).unwrap();
    let row = third["trades"][0].clone();
    assert_eq!(row["id"], json!(oldest.0), "{row}");
    assert_eq!(row["symbol"], json!(HYNIX), "{row}");
    assert_eq!(row["account"], json!("paper"), "{row}");
    assert_eq!(row["state"], json!("discussing"), "{row}");
    assert_eq!(
        row["created_at"],
        json!(saved.created_at.0),
        "밀리초: {row}"
    );
    assert_eq!(
        row["updated_at"],
        json!(saved.updated_at.0),
        "밀리초: {row}"
    );
    assert_eq!(row["decided_at"], Value::Null, "판단 전: {row}");
    bench.close().await;
}

// 검증: V83 C30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn max가_없으면_기본_줄_수이고_100보다_크면_100줄로_줄인다() {
    let bench = start().await;
    let made = many(&bench, 105);
    let all = desc(&made);

    let plain = page(&bench, "").await;
    assert_eq!(HISTORY_DEFAULT_MAX, 20);
    assert_eq!(ids(&plain, "trades"), all[0..HISTORY_DEFAULT_MAX].to_vec());
    assert_eq!(next_of(&plain), Some(all[HISTORY_DEFAULT_MAX - 1]));

    let big = page(&bench, "?max=500").await;
    assert_eq!(HISTORY_PAGE_MAX, 100);
    assert_eq!(ids(&big, "trades"), all[0..HISTORY_PAGE_MAX].to_vec());
    let cursor = next_of(&big).expect("더 있다");
    assert_eq!(cursor, all[HISTORY_PAGE_MAX - 1]);
    let tail = page(&bench, &format!("?max=500&before={cursor}")).await;
    assert_eq!(ids(&tail, "trades"), all[HISTORY_PAGE_MAX..].to_vec());
    assert_eq!(next_of(&tail), None);
    bench.close().await;
}

// 검증: V83 C30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상태로_거르면_그_상태인_매매만_쪽을_넘겨_나온다() {
    let bench = start().await;
    let made = many(&bench, 4);
    let decided = UnixMillis(bench.now().timestamp_millis() + 1_000);
    bench.judge(made[1], decided);
    bench.judge(made[2], decided);
    bench.judge(made[3], decided);
    let waiting = UnixMillis(decided.0 + 1_000);
    bench
        .store()
        .transition_trade(made[3], TradeState::AwaitingApproval, waiting)
        .unwrap();

    let judged = page(&bench, "?state=judged").await;
    assert_eq!(
        ids(&judged, "trades"),
        vec![made[2].0, made[1].0],
        "{judged}"
    );
    for row in judged["trades"].as_array().unwrap() {
        assert_eq!(row["state"], json!("judged"), "{row}");
        assert_eq!(row["decided_at"], json!(decided.0), "판단 시각: {row}");
    }
    let one = page(&bench, "?state=judged&max=1").await;
    assert_eq!(ids(&one, "trades"), vec![made[2].0]);
    assert_eq!(next_of(&one), Some(made[2].0));
    let two = page(&bench, &format!("?state=judged&max=1&before={}", made[2].0)).await;
    assert_eq!(ids(&two, "trades"), vec![made[1].0]);
    assert_eq!(next_of(&two), None, "거른 뒤로 더 없다: {two}");

    let awaiting = page(&bench, "?state=awaiting_approval").await;
    assert_eq!(ids(&awaiting, "trades"), vec![made[3].0], "{awaiting}");
    assert_eq!(awaiting["trades"][0]["updated_at"], json!(waiting.0));
    let discussing = page(&bench, "?state=discussing").await;
    assert_eq!(ids(&discussing, "trades"), vec![made[0].0], "{discussing}");
    let sold = page(&bench, "?state=sold").await;
    assert_eq!(sold["trades"], json!([]), "{sold}");
    assert_eq!(next_of(&sold), None);
    bench.close().await;
}

// 검증: C30 E57
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 틀린_상태_글자나_쪽_값은_400이다() {
    let bench = start().await;
    many(&bench, 2);
    for query in [
        "?state=bogus",
        "?state=Judged",
        "?state=%ED%8C%90%EB%8B%A8%EB%90%A8",
        "?max=0",
        "?max=-1",
        "?max=abc",
        "?before=abc",
        "?before=-3",
    ] {
        let reply = get(bench.addr(), &format!("{TRADES_PATH}{query}")).await;
        assert_eq!(reply.status, 400, "{query}: {}", reply.body);
        assert_eq!(reply.body["error"], json!("invalid_input"), "{query}");
    }
    bench.close().await;
}

// 검증: V83 C30 R6-3 F7
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 매매_하나는_상태_흐름_방향_수량_자료_이름과_근거를_싣는다() {
    let bench = start().await;
    let store = bench.store();
    let trade = bench.discussing(HYNIX, 0);
    let created = store.get_trade(trade).unwrap().created_at;
    let fact = store
        .save_facts(&[NewFact {
            kind: "disclosure".to_string(),
            subject: HYNIX.to_string(),
            label: FACT_LABEL.to_string(),
            period: None,
            value: FactValue::Text(FACT_LABEL.to_string()),
            unit: None,
            reference: Some("20261116000123".to_string()),
            source_name: "DART".to_string(),
            source_url: "local-fake".to_string(),
            published_on: Some("2026-11-16".to_string()),
            known_at: created,
        }])
        .unwrap();
    let decided = UnixMillis(created.0 + 1_000);
    bench.judge(trade, decided);
    store.set_trade_quantity(trade, 7).unwrap();
    store.attach_trade_facts(trade, &[fact[0].id]).unwrap();
    let waiting = UnixMillis(created.0 + 2_000);
    store
        .transition_trade(trade, TradeState::AwaitingApproval, waiting)
        .unwrap();

    let reply = get(bench.addr(), &trade_path(&trade.0.to_string())).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let body = reply.body;
    assert_eq!(body["id"], json!(trade.0));
    assert_eq!(body["symbol"], json!(HYNIX));
    assert_eq!(body["account"], json!("paper"));
    assert_eq!(body["state"], json!("awaiting_approval"));
    assert_eq!(body["created_at"], json!(created.0));
    assert_eq!(body["updated_at"], json!(waiting.0));
    assert_eq!(body["decided_at"], json!(decided.0));
    assert_eq!(
        body["states"],
        json!([
            {"state": "discussing", "from": null, "at": created.0},
            {"state": "judged", "from": "discussing", "at": decided.0},
            {"state": "awaiting_approval", "from": "judged", "at": waiting.0},
        ]),
        "시각 순서의 상태 흐름: {body}"
    );
    assert_eq!(body["side"], json!("buy"));
    assert_eq!(body["quantity"], json!(7));
    assert_eq!(body["facts"], json!([FACT_LABEL]));
    assert_eq!(body["currency"], json!("KRW"), "한국 시장: {body}");

    let basis = &body["basis"];
    assert_eq!(basis["hypothesis"], json!(HYPOTHESIS));
    assert_eq!(basis["trigger"], json!(TRIGGER));
    assert!(basis["entry"].is_string(), "금액은 십진수 글자: {basis}");
    assert_eq!(num(&basis["entry"]), d(&ENTRY.to_string()));
    assert_eq!(num(&basis["stop"]), d("95000"));
    assert_eq!(num(&basis["target"]), d("110000"));
    assert_eq!(num(&basis["expected_value"]), d("5000"));
    assert_eq!(
        basis["scores"],
        json!([{"role": SCORE_ROLE, "score": 70, "reason": SCORE_REASON}])
    );
    assert_eq!(basis["consensus_score"], json!(70));
    assert_eq!(
        basis["sources"],
        json!([{"name": SOURCE_NAME, "known_at": decided.0}])
    );

    // 근거 요약은 지금 근거 경로의 글과 같다.
    let plain = get(
        bench.addr(),
        &TRADE_BASIS_PATH.replace("{id}", &trade.0.to_string()),
    )
    .await;
    assert_eq!(plain.status, 200, "근거 경로는 그대로다: {}", plain.body);
    assert_eq!(body["summary"], plain.body["summary"]);
    assert!(
        body["summary"]
            .as_str()
            .is_some_and(|s| s.contains(HYPOTHESIS))
    );
    bench.close().await;
}

// 검증: V83 C30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 판단_전_매매는_근거_칸이_비어_있고_흐름은_만든_줄_하나다() {
    let bench = start().await;
    let trade = bench.discussing(NAVER, 0);
    let created = bench.store().get_trade(trade).unwrap().created_at;
    let reply = get(bench.addr(), &trade_path(&trade.0.to_string())).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let body = reply.body;
    assert_eq!(body["state"], json!("discussing"));
    assert_eq!(
        body["states"],
        json!([{"state": "discussing", "from": null, "at": created.0}])
    );
    assert_eq!(body["side"], Value::Null);
    assert_eq!(body["quantity"], Value::Null);
    assert_eq!(body["facts"], json!([]));
    assert_eq!(body["decided_at"], Value::Null);
    let basis = &body["basis"];
    for key in [
        "hypothesis",
        "trigger",
        "entry",
        "stop",
        "target",
        "expected_value",
        "consensus_score",
    ] {
        assert_eq!(basis[key], Value::Null, "{key}: {basis}");
    }
    assert_eq!(basis["scores"], json!([]));
    assert_eq!(basis["sources"], json!([]));
    assert!(body["summary"].is_string(), "{body}");
    bench.close().await;
}

// 검증: C30 E57
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 없는_매매_번호는_404이고_숫자가_아니면_400이다() {
    let bench = start().await;
    let trade = bench.discussing(HYNIX, 0);
    let missing = get(bench.addr(), &trade_path(&(trade.0 + 100).to_string())).await;
    assert_eq!(missing.status, 404, "{}", missing.body);
    assert_eq!(missing.body["error"], json!("not_found"));
    let bad = get(bench.addr(), &trade_path("abc")).await;
    assert_eq!(bad.status, 400, "{}", bad.body);
    // 있는 번호는 열린다(위 404가 경로가 없어서가 아님을 본다).
    let found = get(bench.addr(), &trade_path(&trade.0.to_string())).await;
    assert_eq!(found.status, 200, "{}", found.body);
    assert_eq!(found.body["id"], json!(trade.0));
    bench.close().await;
}

// 검증: C30 E4 N6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기는_세션_없이_히스토리를_못_읽고_세션이_있으면_읽는다() {
    let bench = start().await;
    let trade = bench.discussing(HYNIX, 0);
    let one = trade_path(&trade.0.to_string());
    let dated = format!("{DAILY_SUMMARY_PATH}?date=2026-11-16");
    bench.go_remote();
    for path in [TRADES_PATH, one.as_str(), APPROVAL_LOG_PATH, dated.as_str()] {
        let reply = get_anonymous(bench.addr(), path).await;
        assert_eq!(reply.status, 401, "{path}: {}", reply.body);
    }
    let cookie = bench.remote_login().await;
    let listed = get_with(bench.addr(), TRADES_PATH, &cookie).await;
    assert_eq!(listed.status, 200, "{}", listed.body);
    assert_eq!(ids(&listed.body, "trades"), vec![trade.0]);
    let single = get_with(bench.addr(), &one, &cookie).await;
    assert_eq!(single.status, 200, "{}", single.body);
    let log = get_with(bench.addr(), APPROVAL_LOG_PATH, &cookie).await;
    assert_eq!(log.status, 200, "{}", log.body);
    assert_eq!(log.body["approvals"], json!([]), "{}", log.body);
    bench.close().await;
}
