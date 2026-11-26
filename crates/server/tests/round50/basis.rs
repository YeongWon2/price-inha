//! C23 · E53 · V68 · V69: 근거 카드 읽기. `GET /api/trades/{id}/basis`의 옛 칸(`id` · `summary`)은 그대로이고
//! 담당별 의견 · 팀장 요약 · 뒤집힐 조건 · 찬반 수 · 자료 이름이 더해진다. 세기 · 정렬은 서버가 한다.

use pi_store::{TradeId, TradeSide};
use price_inha::{TRADE_BASIS_PATH, TRADE_PATH};
use serde_json::{Value, json};

use crate::common::{
    Bench, HYNIX, LEADER_REVERSAL, LEADER_SUMMARY, NAVER, REVIEW_REASON, RISK_REASON, SCOUT_REASON,
    SOURCE_NAME, STRATEGY_REASON, get, kst, monday,
};

async fn start() -> Bench {
    Bench::start(kst(monday(), 10, 0)).await
}

/// 근거 경로를 읽는다(200이어야 한다).
async fn basis(bench: &Bench, trade: TradeId) -> Value {
    let path = TRADE_BASIS_PATH.replace("{id}", &trade.0.to_string());
    let reply = get(bench.addr(), &path).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    reply.body
}

/// 의견 목록에서 그 담당의 줄.
fn line<'a>(body: &'a Value, agent: &str) -> &'a Value {
    body["opinions"]
        .as_array()
        .unwrap_or_else(|| panic!("opinions 목록이 있다: {body}"))
        .iter()
        .find(|o| o["agent"] == agent)
        .unwrap_or_else(|| panic!("{agent} 줄이 있다: {body}"))
}

// 검증: V68 C23 R6-4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 근거_경로에_담당마다_방향_확신도_이유_보는_기간이_팀_차례로_실린다() {
    let bench = start().await;
    let trade = bench.judged_with_opinions(HYNIX, TradeSide::Buy);
    let body = basis(&bench, trade).await;
    // 옛 칸은 그대로다.
    assert_eq!(body["id"], trade.0, "{body}");
    assert!(
        body["summary"].as_str().is_some_and(|s| !s.is_empty()),
        "{body}"
    );
    // 준 차례(리스크 · 뉴스 · 복기 · 전략)가 아니라 팀 차례(뉴스 → 전략 → 리스크 → 복기)로 서버가 늘어놓는다.
    let agents: Vec<&str> = body["opinions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["agent"].as_str().unwrap())
        .collect();
    assert_eq!(agents, ["scout", "strategy", "risk", "review"], "{body}");

    let strategy = line(&body, "strategy");
    assert_eq!(strategy["direction"], "bull", "{strategy}");
    assert_eq!(strategy["confidence"], "high", "{strategy}");
    assert_eq!(strategy["reason"], STRATEGY_REASON);
    assert_eq!(strategy["horizon_days"], 20);
    assert_eq!(strategy["abstained"], false);
    assert_eq!(strategy["score"], 80);
    assert_eq!(strategy["name"], "전략 담당");

    let risk = line(&body, "risk");
    assert_eq!(
        (
            &risk["direction"],
            &risk["confidence"],
            &risk["horizon_days"]
        ),
        (&json!("neutral"), &json!("mid"), &json!(5)),
        "{risk}"
    );
    assert_eq!(risk["reason"], RISK_REASON);

    let review = line(&body, "review");
    assert_eq!(review["direction"], "bear", "{review}");
    assert_eq!(review["confidence"], "low", "{review}");
    assert_eq!(
        review["horizon_days"],
        Value::Null,
        "보는 기간이 없으면 null: {review}"
    );
    assert_eq!(review["reason"], REVIEW_REASON);
    bench.close().await;
}

// 검증: V68 E53 C23
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 의견을_못_낸_담당은_0점이_아니라_기권이고_찬반_수에_따로_센다() {
    let bench = start().await;
    let trade = bench.judged_with_opinions(HYNIX, TradeSide::Buy);
    let body = basis(&bench, trade).await;
    let scout = line(&body, "scout");
    assert_eq!(scout["abstained"], true, "{scout}");
    assert_eq!(
        scout["direction"],
        Value::Null,
        "기권은 방향이 없다: {scout}"
    );
    assert_eq!(scout["confidence"], Value::Null, "{scout}");
    assert_eq!(scout["reason"], SCOUT_REASON);
    assert_eq!(
        body["votes"],
        json!({"bull": 1, "bear": 1, "neutral": 1, "abstain": 1}),
        "찬반 수는 서버가 센다(기권은 중립과 따로): {body}"
    );
    bench.close().await;
}

// 검증: V69 C23 R6-5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 근거_경로에_팀장_결정과_요약과_뒤집힐_조건과_자료_이름이_실린다() {
    let bench = start().await;
    let bought = bench.judged_with_opinions(HYNIX, TradeSide::Buy);
    let sold = bench.judged_with_opinions(NAVER, TradeSide::Sell);
    let body = basis(&bench, bought).await;
    assert_eq!(
        body["leader"],
        json!({"decision": "buy", "summary": LEADER_SUMMARY, "flip_condition": LEADER_REVERSAL}),
        "{body}"
    );
    let names: Vec<&str> = body["sources"]
        .as_array()
        .unwrap_or_else(|| panic!("sources 목록이 있다: {body}"))
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, [SOURCE_NAME], "{body}");
    let body = basis(&bench, sold).await;
    assert_eq!(body["leader"]["decision"], "sell", "{body}");
    bench.close().await;
}

// 검증: V69 E53 C23
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 의견_칸이_생기기_전_매매는_빈_목록이고_팀장_요약이_없다() {
    let bench = start().await;
    let trade = bench.judged_plain(HYNIX);
    let body = basis(&bench, trade).await;
    assert_eq!(body["id"], trade.0, "{body}");
    assert!(
        body["summary"].as_str().is_some_and(|s| !s.is_empty()),
        "{body}"
    );
    assert_eq!(
        body["opinions"],
        json!([]),
        "빈 칸을 추정해 채우지 않는다: {body}"
    );
    assert_eq!(body["leader"], Value::Null, "{body}");
    assert_eq!(
        body["votes"],
        json!({"bull": 0, "bear": 0, "neutral": 0, "abstain": 0}),
        "{body}"
    );
    bench.close().await;
}

// 검증: E3 C23
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 없는_매매의_근거는_404다() {
    let bench = start().await;
    let reply = get(bench.addr(), &TRADE_BASIS_PATH.replace("{id}", "987654")).await;
    assert_eq!(reply.status, 404, "{}", reply.body);
    assert_eq!(reply.body["error"], "not_found");
    bench.close().await;
}

// 검증: V68 V69 V83 C23
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 히스토리의_매매_하나에도_근거에_같은_의견_칸이_실린다() {
    let bench = start().await;
    let with = bench.judged_with_opinions(HYNIX, TradeSide::Buy);
    let plain = bench.judged_plain(NAVER);
    for trade in [with, plain] {
        let card = basis(&bench, trade).await;
        let reply = get(
            bench.addr(),
            &TRADE_PATH.replace("{id}", &trade.0.to_string()),
        )
        .await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        let detail = &reply.body["basis"];
        assert!(detail["opinions"].is_array(), "{detail}");
        assert!(detail["votes"].is_object(), "{detail}");
        for key in ["opinions", "leader", "votes"] {
            assert_eq!(detail[key], card[key], "{key}: {detail}");
        }
        // 옛 칸(담당 점수 · 자료)은 그대로다.
        assert!(detail["scores"].is_array(), "{detail}");
        assert!(detail["sources"].is_array(), "{detail}");
    }
    bench.close().await;
}
