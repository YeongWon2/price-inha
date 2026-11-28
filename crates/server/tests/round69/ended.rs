//! 지켜보기 · 오류로 끝난 논의의 표시 칸(M2 상태표는 그대로 — 표시만 더한다).

use pi_store::TradeId;
use price_inha::{TRADE_BASIS_PATH, TRADE_PATH, TRADES_PATH};
use serde_json::{Value, json};

use crate::common::{Bench, get};

/// 매매 목록을 읽는다(200이어야 한다).
async fn page(bench: &Bench, query: &str) -> Value {
    let (status, body) = get(bench.addr(), &format!("{TRADES_PATH}{query}")).await;
    assert_eq!(status, 200, "{query}: {body}");
    body
}

/// 목록에서 그 매매의 줄.
fn row_of(list: &Value, trade: TradeId) -> Value {
    list["trades"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == json!(trade.0))
        .cloned()
        .unwrap_or_else(|| panic!("매매 {}가 목록에 있어야 한다: {list}", trade.0))
}

/// 매매 하나와 그 근거(둘 다 200).
async fn detail_and_basis(bench: &Bench, trade: TradeId) -> (Value, Value) {
    let id = trade.0.to_string();
    let (status, detail) = get(bench.addr(), &TRADE_PATH.replace("{id}", &id)).await;
    assert_eq!(status, 200, "{detail}");
    let (status, basis) = get(bench.addr(), &TRADE_BASIS_PATH.replace("{id}", &id)).await;
    assert_eq!(status, 200, "{basis}");
    (detail, basis)
}

fn ids(list: &Value) -> Vec<i64> {
    list["trades"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_i64().unwrap())
        .collect()
}

/// 칸이 없어야 한다(`null`도 아니고 아예 없다).
fn absent(body: &Value, field: &str) {
    assert!(
        body.get(field).is_none(),
        "{field} 칸이 없어야 한다: {body}"
    );
}

// 검증: V83 C30 M2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 지켜보기로_끝난_논의중_매매에_watched_칸이_붙고_상태는_그대로다() {
    let bench = Bench::start().await;
    let ended = bench.discussing(1_000);
    bench.member_said(ended, 1_100);
    bench.watched(ended, "missing_data", 1_200);
    let plain = bench.discussing(2_000);

    let list = page(&bench, "").await;
    let row = row_of(&list, ended);
    assert_eq!(row["state"], json!("discussing"), "상태는 그대로: {row}");
    assert_eq!(
        row["watched"],
        json!({"reason_code": "missing_data", "at": 1_200}),
        "{row}"
    );
    absent(&row, "failed");
    let quiet = row_of(&list, plain);
    absent(&quiet, "watched");
    absent(&quiet, "failed");

    let (detail, basis) = detail_and_basis(&bench, ended).await;
    assert_eq!(detail["state"], json!("discussing"));
    assert_eq!(
        detail["watched"],
        json!({"reason_code": "missing_data", "at": 1_200})
    );
    assert_eq!(
        basis["watched"],
        json!({"reason_code": "missing_data", "at": 1_200})
    );
    absent(&detail, "failed");
    let (detail, basis) = detail_and_basis(&bench, plain).await;
    absent(&detail, "watched");
    absent(&basis, "watched");
    bench.close().await;
}

// 검증: V83 C30 M2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다시_논의가_열리면_칸이_없고_다시_지켜보면_마지막_결과다() {
    let bench = Bench::start().await;
    let trade = bench.discussing(1_000);
    bench.watched(trade, "missing_data", 1_100);
    // 논의 뒤 서버가 남기는 다른 줄은 새 논의가 아니다.
    bench.server_note(trade, 1_150);
    let row = row_of(&page(&bench, "").await, trade);
    assert_eq!(
        row["watched"],
        json!({"reason_code": "missing_data", "at": 1_100})
    );

    bench.member_said(trade, 1_200);
    let row = row_of(&page(&bench, "").await, trade);
    absent(&row, "watched");
    absent(&row, "failed");
    let (detail, basis) = detail_and_basis(&bench, trade).await;
    absent(&detail, "watched");
    absent(&basis, "watched");

    bench.watched(trade, "consensus_below", 1_300);
    let row = row_of(&page(&bench, "").await, trade);
    assert_eq!(
        row["watched"],
        json!({"reason_code": "consensus_below", "at": 1_300})
    );
    bench.close().await;
}

// 검증: V83 C30 M2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 논의가_오류로_끝나면_failed_칸이_붙고_다시_열리면_없다() {
    let bench = Bench::start().await;
    let trade = bench.discussing(1_000);
    bench.member_said(trade, 1_100);
    bench.failed(trade, "discussion_error", 1_200);

    let row = row_of(&page(&bench, "").await, trade);
    assert_eq!(row["state"], json!("discussing"));
    assert_eq!(
        row["failed"],
        json!({"reason_code": "discussion_error", "at": 1_200}),
        "{row}"
    );
    absent(&row, "watched");
    let (detail, basis) = detail_and_basis(&bench, trade).await;
    assert_eq!(
        detail["failed"],
        json!({"reason_code": "discussion_error", "at": 1_200})
    );
    assert_eq!(
        basis["failed"],
        json!({"reason_code": "discussion_error", "at": 1_200})
    );

    bench.member_said(trade, 1_300);
    let row = row_of(&page(&bench, "").await, trade);
    absent(&row, "failed");
    absent(&row, "watched");
    bench.close().await;
}

// 검증: V83 C30 M2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 논의중이_아닌_매매에는_방_로그가_지켜보기여도_칸이_없다() {
    let bench = Bench::start().await;
    let trade = bench.discussing(1_000);
    bench.watched(trade, "leader_watch", 1_100);
    bench.judge(trade, 1_200);
    let row = row_of(&page(&bench, "").await, trade);
    assert_eq!(row["state"], json!("judged"));
    absent(&row, "watched");
    absent(&row, "failed");
    let (detail, basis) = detail_and_basis(&bench, trade).await;
    absent(&detail, "watched");
    absent(&basis, "watched");
    bench.close().await;
}

// 검증: V83 C30 M2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상태로_거르면_논의중과_지켜봄과_끝나지_못함이_나뉘어_쪽을_넘겨_나온다() {
    let bench = Bench::start().await;
    let plain = bench.discussing(1_000);
    let watch_a = bench.discussing(1_001);
    bench.watched(watch_a, "missing_data", 1_100);
    let reopened = bench.discussing(1_002);
    bench.watched(reopened, "missing_data", 1_100);
    bench.member_said(reopened, 1_200);
    let failed = bench.discussing(1_003);
    bench.failed(failed, "discussion_error", 1_100);
    let watch_b = bench.discussing(1_004);
    bench.watched(watch_b, "role_unavailable", 1_100);
    let judged = bench.discussing(1_005);
    bench.judge(judged, 1_300);

    let discussing = page(&bench, "?state=discussing").await;
    assert_eq!(ids(&discussing), vec![reopened.0, plain.0], "{discussing}");
    let watched = page(&bench, "?state=watched").await;
    assert_eq!(ids(&watched), vec![watch_b.0, watch_a.0], "{watched}");
    for row in watched["trades"].as_array().unwrap() {
        assert_eq!(row["state"], json!("discussing"), "{row}");
        assert!(row["watched"].is_object(), "{row}");
    }
    assert_eq!(watched["next"], Value::Null);
    let first = page(&bench, "?state=watched&max=1").await;
    assert_eq!(ids(&first), vec![watch_b.0], "{first}");
    assert_eq!(first["next"], json!(watch_b.0), "{first}");
    let second = page(
        &bench,
        &format!("?state=watched&max=1&before={}", watch_b.0),
    )
    .await;
    assert_eq!(ids(&second), vec![watch_a.0], "{second}");
    assert_eq!(second["next"], Value::Null, "거른 뒤로 더 없다: {second}");
    let ended = page(&bench, "?state=failed").await;
    assert_eq!(ids(&ended), vec![failed.0], "{ended}");
    let decided = page(&bench, "?state=judged").await;
    assert_eq!(ids(&decided), vec![judged.0], "{decided}");
    let all = page(&bench, "").await;
    assert_eq!(
        all["trades"].as_array().unwrap().len(),
        6,
        "거르지 않으면 모두: {all}"
    );
    bench.close().await;
}
