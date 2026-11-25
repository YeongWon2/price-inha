//! 35차 2번: 금지 종목 · 업종표 읽기 · 고치기 `GET` · `POST /api/rules/config`.

use price_inha::{KIND_RULES_CONFIG, RULES_CONFIG_FILE, RULES_CONFIG_PATH, RULES_SECTOR_MAX_CHARS};
use serde_json::{Value, json};

use crate::common::{
    Bench, HYNIX, WATCHED, blocked_detail, config_file, events, get, get_with, kst, monday, passed,
    post, post_raw, wait_for,
};

/// 그 컴퓨터의 세션으로 읽은 지금 값(200이어야 한다).
async fn current(bench: &Bench) -> Value {
    let reply = get(bench.addr(), RULES_CONFIG_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.body
}

/// 업종표 한 줄을 고치는 몸통(금지 종목 없음).
fn sectors_body(rows: &[(&str, &str)]) -> Value {
    let sectors: Vec<Value> = rows
        .iter()
        .map(|(symbol, sector)| json!({"symbol": symbol, "sector": sector}))
        .collect();
    json!({"banned": [], "sectors": sectors})
}

// 검증: R7-3 F5 V82 P5 P8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 읽기는_금지_종목은_저장된_차례_업종표는_종목_차례이고_고칠_수_있는가와_업종_없는_종목을_싣는다()
 {
    let raw = config_file(
        &[("us", "TSLA"), ("kr", "000270")],
        &[(HYNIX, "반도체"), ("000100", "제약")],
    );
    let bench = Bench::with_file(kst(monday(), 10, 0), &raw).await;
    let body = current(&bench).await;
    assert_eq!(
        body["banned"],
        json!([{"market": "us", "symbol": "TSLA"}, {"market": "kr", "symbol": "000270"}]),
        "{body}"
    );
    assert_eq!(
        body["sectors"],
        json!([
            {"symbol": "000100", "sector": "제약"},
            {"symbol": HYNIX, "sector": "반도체"},
        ]),
        "{body}"
    );
    assert_eq!(
        body["editable"],
        json!(true),
        "그 컴퓨터에서는 고칠 수 있다: {body}"
    );
    assert_eq!(
        body["unsectored"],
        json!([WATCHED]),
        "지켜보는 종목 가운데 업종이 없는 것: {body}"
    );
    bench.close().await;
}

// 검증: R7-3 V82 P8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 파일이_없으면_읽기는_빈_표다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let body = current(&bench).await;
    assert_eq!(body["banned"], json!([]), "{body}");
    assert_eq!(body["sectors"], json!([]), "{body}");
    assert_eq!(body["unsectored"], json!([WATCHED]), "{body}");
    bench.close().await;
}

// 검증: C3 C16 M6 R7-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기의_세션은_읽기만_하고_고치기는_403이며_저장하지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let cookie = bench.remote_login().await;
    let reply = get_with(bench.addr(), RULES_CONFIG_PATH, Some(&cookie)).await;
    assert_eq!(reply.status, 200, "원격도 읽을 수는 있다: {}", reply.text);
    assert_eq!(
        reply.body["editable"],
        json!(false),
        "원격이면 고칠 수 없다고 알린다: {}",
        reply.text
    );
    let body = sectors_body(&[(HYNIX, "반도체")]).to_string();
    let reply = post_raw(bench.addr(), RULES_CONFIG_PATH, Some(&cookie), &body).await;
    assert_eq!(reply.status, 403, "그 컴퓨터에서만 고친다: {}", reply.text);
    assert!(
        !bench.home().join(RULES_CONFIG_FILE).exists(),
        "거부된 고치기는 저장하지 않는다"
    );
    assert!(
        events(&bench.store(), KIND_RULES_CONFIG).is_empty(),
        "거부된 고치기는 바꾼 일로 남지 않는다"
    );
    let bench = bench.restart().await;
    let body = current(&bench).await;
    assert_eq!(body["sectors"], json!([]), "값은 그대로다: {body}");
    bench.close().await;
}

// 검증: C2 C15 D2 R7-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 세션이_없으면_읽기도_고치기도_401이다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let reply = get_with(bench.addr(), RULES_CONFIG_PATH, None).await;
    assert_eq!(reply.status, 401, "{}", reply.text);
    let body = sectors_body(&[(HYNIX, "반도체")]).to_string();
    let reply = post_raw(bench.addr(), RULES_CONFIG_PATH, None, &body).await;
    assert_eq!(reply.status, 401, "{}", reply.text);
    assert!(!bench.home().join(RULES_CONFIG_FILE).exists());
    bench.close().await;
}

// 검증: R7-3 E4 P5 P8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 틀린_값은_400이고_아무것도_저장하지_않는다() {
    let raw = config_file(&[("kr", "000270")], &[(HYNIX, "반도체")]);
    let bench = Bench::with_file(kst(monday(), 10, 0), &raw).await;
    let before = current(&bench).await;
    let long = "가".repeat(RULES_SECTOR_MAX_CHARS + 1);
    let bad = [
        json!({"banned": [{"market": "kr", "symbol": "삼성 전자"}], "sectors": []}),
        json!({"banned": [{"market": "jp", "symbol": HYNIX}], "sectors": []}),
        json!({"banned": [{"market": "kr", "symbol": HYNIX}, {"market": "kr", "symbol": HYNIX}], "sectors": []}),
        sectors_body(&[(HYNIX, "   ")]),
        sectors_body(&[(HYNIX, &long)]),
        sectors_body(&[(HYNIX, "반도체"), (HYNIX, "전자")]),
        sectors_body(&[("", "반도체")]),
        json!({"banned": []}),
        json!({"banned": "000660", "sectors": []}),
    ];
    for body in bad {
        let reply = post(bench.addr(), RULES_CONFIG_PATH, &body).await;
        assert_eq!(reply.status, 400, "{body} → {}", reply.text);
        assert_eq!(
            reply.body["error"],
            json!("invalid_input"),
            "{}",
            reply.text
        );
        assert_eq!(current(&bench).await, before, "{body}: 값은 그대로다");
    }
    assert_eq!(
        std::fs::read_to_string(bench.home().join(RULES_CONFIG_FILE)).unwrap(),
        raw,
        "파일도 그대로다"
    );
    assert!(events(&bench.store(), KIND_RULES_CONFIG).is_empty());
    bench.close().await;
}

// 검증: R7-3 F5 P5 P8 V82
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 고친_값은_다시_켜지_않아도_다음_점검부터_쓰이고_바꾼_일이_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let (_trade, placed) = bench.buy(HYNIX).await;
    assert!(
        blocked_detail(&placed).is_some_and(|detail| detail.contains("P8")),
        "고치기 전에는 P8로 막힌다: {placed:?}"
    );

    let body = sectors_body(&[(HYNIX, "  반도체  ")]);
    let reply = post(bench.addr(), RULES_CONFIG_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        reply.body["sectors"],
        json!([{"symbol": HYNIX, "sector": "반도체"}]),
        "답은 읽기와 같은 모양이고 앞뒤 공백을 뗀 값이다: {}",
        reply.text
    );
    assert_eq!(reply.body["banned"], json!([]), "{}", reply.text);
    assert_eq!(reply.body["editable"], json!(true), "{}", reply.text);
    assert_eq!(reply.body["unsectored"], json!([WATCHED]), "{}", reply.text);
    assert!(
        bench.home().join(RULES_CONFIG_FILE).exists(),
        "보관 폴더의 파일에 남는다"
    );
    let (_trade, placed) = bench.buy(HYNIX).await;
    assert!(passed(&placed), "고친 업종표를 바로 쓴다: {placed:?}");

    let banned = json!({
        "banned": [{"market": "kr", "symbol": "000100"}],
        "sectors": [{"symbol": HYNIX, "sector": "반도체"}, {"symbol": "000100", "sector": "제약"}],
    });
    let reply = post(bench.addr(), RULES_CONFIG_PATH, &banned).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let (_trade, placed) = bench.buy("000100").await;
    assert!(
        blocked_detail(&placed).is_some_and(|detail| detail.contains("P5")),
        "고친 금지 종목을 바로 쓴다: {placed:?}"
    );

    let store = bench.store();
    wait_for("바꾼 일 두 번", || {
        events(&store, KIND_RULES_CONFIG).len() == 2
    })
    .await;
    let changes = events(&store, KIND_RULES_CONFIG);
    assert_eq!(changes[0]["by"], json!("local"), "{changes:?}");
    assert_eq!(changes[1]["banned"], json!(1), "{changes:?}");
    assert_eq!(changes[1]["sectors"], json!(2), "{changes:?}");
    assert!(changes[1]["at"].is_i64(), "{changes:?}");
    bench.close().await;
}

// 검증: R7-3 P8 V6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 고친_값은_다시_켜도_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let body = json!({
        "banned": [{"market": "us", "symbol": "TSLA"}],
        "sectors": [{"symbol": HYNIX, "sector": "반도체"}],
    });
    let reply = post(bench.addr(), RULES_CONFIG_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let before = current(&bench).await;

    let bench = bench.restart().await;
    assert_eq!(current(&bench).await, before, "다시 켜도 같은 값이다");
    bench.ready().await;
    let (_trade, placed) = bench.buy(HYNIX).await;
    assert!(
        passed(&placed),
        "다시 켠 뒤에도 그 업종표를 쓴다: {placed:?}"
    );
    bench.close().await;
}
