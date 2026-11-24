//! 27차 1번(C28 · V79 · V81): 템플릿 화면 읽기 `GET` [`TEMPLATES_VIEW_PATH`]의 모양.
//! 템플릿 없음 · 모의중 칸 목록 · 판 이력(바뀐 칸 · 넓힘) · 실전중 잠김 · 승격 기준 줄 · 승격대기의 승인 번호 ·
//! 원격 세션 관문.

use pi_store::TemplateState;
use price_inha::{TEMPLATE_ADJUST_PATH, TEMPLATES_VIEW_PATH};
use reqwest::Method;
use serde_json::{Value, json};

use crate::common::{Bench, bare, entry, get, num, post};

async fn view(bench: &Bench) -> Value {
    let reply = get(bench.addr(), TEMPLATES_VIEW_PATH, None).await;
    assert_eq!(reply.status, 200, "읽기: {}", reply.text);
    reply.body
}

fn field<'a>(template: &'a Value, key: &str) -> &'a Value {
    template["fields"]
        .as_array()
        .and_then(|fields| fields.iter().find(|f| f["key"] == json!(key)))
        .unwrap_or_else(|| panic!("칸 {key}가 있다: {template}"))
}

fn line<'a>(template: &'a Value, criterion: &str) -> &'a Value {
    template["promotion"]
        .as_array()
        .and_then(|lines| lines.iter().find(|l| l["criterion"] == json!(criterion)))
        .unwrap_or_else(|| panic!("승격 기준 줄 {criterion}가 있다: {template}"))
}

// 검증: C28 V79 R10-1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 템플릿이_없으면_빈_목록이고_쓰는_템플릿도_없다() {
    let bench = Bench::start().await;
    let body = view(&bench).await;
    assert_eq!(body["current"], Value::Null, "{body}");
    assert_eq!(body["templates"], json!([]), "{body}");
    bench.close().await;
}

// 검증: C28 V79 R10-1 R10-2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 스윙을_고르면_모의중이고_칸마다_키_값_상한_바꿀_수_있음이_보인다() {
    let bench = Bench::start().await;
    let id = bench.choose("swing").await;
    let body = view(&bench).await;
    assert_eq!(
        body["current"],
        json!(id.0),
        "고른 것이 쓰는 템플릿: {body}"
    );
    let template = entry(&body, id);
    assert_eq!(template["style"], json!("swing"), "{template}");
    assert_eq!(template["state"], json!("simulating"), "{template}");
    assert_eq!(template["revision"], json!(1), "{template}");

    let fields = template["fields"].as_array().expect("칸 목록");
    assert_eq!(fields.len(), 26, "template_view의 26칸: {template}");
    assert_eq!(
        fields[0]["key"],
        json!("risk_per_trade_pct"),
        "순서는 template_view 그대로"
    );
    let risk = field(&template, "risk_per_trade_pct");
    assert!(
        (num(&risk["value"]) - 1.0).abs() < 1e-9,
        "스윙 1회 위험 1%: {risk}"
    );
    assert!(
        (num(&risk["cap"]) - 2.0).abs() < 1e-9,
        "1회 위험 상한 2%: {risk}"
    );
    assert_eq!(
        risk["editable"],
        json!(true),
        "모의중은 바꿀 수 있다: {risk}"
    );
    let sector = field(&template, "sector_weight_pct");
    assert!(
        (num(&sector["cap"]) - 50.0).abs() < 1e-9,
        "업종 상한 50%: {sector}"
    );
    let positions = field(&template, "max_positions");
    assert_eq!(positions["cap"], Value::Null, "상한이 없는 칸: {positions}");
    for f in fields {
        assert_eq!(
            f["editable"],
            json!(true),
            "모의중은 모든 칸을 바꿀 수 있다: {f}"
        );
    }
    assert_eq!(
        template["approval"],
        Value::Null,
        "모의중에는 승격 승인 번호가 없다"
    );
    bench.close().await;
}

// 검증: C28 V81 R11-2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 판_이력은_판_1에서_시작하고_값을_바꾸면_판_2에_바뀐_칸과_넓힘이_보인다() {
    let bench = Bench::start().await;
    let id = bench.choose("swing").await;

    let first = entry(&view(&bench).await, id);
    let revisions = first["revisions"].as_array().expect("판 이력");
    assert_eq!(revisions.len(), 1, "{first}");
    assert_eq!(revisions[0]["revision"], json!(1));
    assert!(revisions[0]["at"].is_i64(), "쓴 시각(밀리초): {first}");
    assert_eq!(revisions[0]["changes"], json!([]), "첫 판은 앞 판이 없다");
    assert_eq!(revisions[0]["proposal"], Value::Null);

    // 줄이기: 1회 위험 1 → 0.5(넓힘 아님).
    let changes = json!({"id": id.0, "changes": {"risk_per_trade_pct": "0.5"}});
    let reply = post(bench.addr(), TEMPLATE_ADJUST_PATH, &changes, None).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    // 넓히기: 일일 손실 한도 3 → 4(상한 5 안, 넓힘).
    let changes = json!({"id": id.0, "changes": {"daily_loss_limit_pct": "4"}});
    let reply = post(bench.addr(), TEMPLATE_ADJUST_PATH, &changes, None).await;
    assert_eq!(reply.status, 200, "{}", reply.text);

    let template = entry(&view(&bench).await, id);
    assert_eq!(template["revision"], json!(3), "{template}");
    let revisions = template["revisions"].as_array().expect("판 이력");
    let numbers: Vec<Value> = revisions.iter().map(|r| r["revision"].clone()).collect();
    assert_eq!(
        numbers,
        vec![json!(1), json!(2), json!(3)],
        "판 순서: {template}"
    );

    let second = revisions[1]["changes"].as_array().expect("판 2의 바뀐 칸");
    assert_eq!(second.len(), 1, "바뀐 칸만: {template}");
    assert_eq!(second[0]["key"], json!("risk_per_trade_pct"));
    assert!((num(&second[0]["before"]) - 1.0).abs() < 1e-9, "{template}");
    assert!((num(&second[0]["after"]) - 0.5).abs() < 1e-9, "{template}");
    assert_eq!(second[0]["widened"], json!(false), "줄이기는 넓힘이 아니다");

    let third = revisions[2]["changes"].as_array().expect("판 3의 바뀐 칸");
    assert_eq!(third.len(), 1, "바뀐 칸만: {template}");
    assert_eq!(third[0]["key"], json!("daily_loss_limit_pct"));
    assert_eq!(
        third[0]["widened"],
        json!(true),
        "한도를 키우면 넓힘: {template}"
    );
    assert_eq!(
        revisions[2]["proposal"],
        Value::Null,
        "바로 반영한 판은 개선안 번호가 없다"
    );
    assert!(
        num(&field(&template, "risk_per_trade_pct")["value"]) < 0.6,
        "칸 목록은 지금 판의 값: {template}"
    );
    bench.close().await;
}

// 검증: C28 V79 E28 R10-2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실전중_템플릿은_모든_칸이_바꿀_수_없음으로_보인다() {
    let bench = Bench::start().await;
    let id = bench.live_template().await;
    let body = view(&bench).await;
    assert_eq!(body["current"], json!(id.0), "{body}");
    let template = entry(&body, id);
    assert_eq!(template["state"], json!("live"), "{template}");
    let fields = template["fields"].as_array().expect("칸 목록");
    assert_eq!(fields.len(), 26);
    for f in fields {
        assert_eq!(f["editable"], json!(false), "실전중은 잠겼다: {f}");
    }
    assert!(
        field(&template, "risk_per_trade_pct")["cap"].is_string(),
        "잠겨도 상한은 보인다"
    );
    assert_eq!(
        template["approval"],
        Value::Null,
        "실전중에는 기다리는 승격 승인이 없다"
    );
    bench.close().await;
}

// 검증: C28 E28 V81
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 실전중_위험을_키우는_값은_승인_번호와_함께_승인_대기로_답하고_판은_그대로다() {
    let bench = Bench::start().await;
    let id = bench.live_template().await;
    let changes = json!({"id": id.0, "changes": {"risk_per_trade_pct": "1.5"}});
    let reply = post(bench.addr(), TEMPLATE_ADJUST_PATH, &changes, None).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        reply.body["state"],
        json!("needs_approval"),
        "{}",
        reply.text
    );
    let number = reply.body["number"].as_str().expect("승인 번호 글자");
    assert!(!number.is_empty(), "{}", reply.text);

    let template = entry(&view(&bench).await, id);
    assert_eq!(template["state"], json!("live"), "{template}");
    assert_eq!(
        template["revision"],
        json!(1),
        "승인 전에는 새 판이 없다: {template}"
    );
    assert_eq!(template["revisions"].as_array().map(Vec::len), Some(1));
    bench.close().await;
}

// 검증: C28 V81 E58 R11-2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승격_기준_줄에_기준_필요한_값_지금_값_채웠는가가_보인다() {
    let bench = Bench::start().await;
    let id = bench.choose("swing").await;
    let template = entry(&view(&bench).await, id);
    let lines = template["promotion"].as_array().expect("승격 기준 줄");
    let criteria: Vec<Value> = lines.iter().map(|l| l["criterion"].clone()).collect();
    assert_eq!(
        criteria,
        vec![
            json!("trades"),
            json!("trading_days"),
            json!("net_pnl"),
            json!("max_drawdown"),
            json!("out_of_sample"),
        ],
        "promotion_lines 순서(스윙은 표본 밖 기준이 있다): {template}"
    );

    let trades = line(&template, "trades");
    assert!(
        (num(&trades["need"]) - 30.0).abs() < 1e-9,
        "스윙 매매 30건: {trades}"
    );
    assert!(
        num(&trades["have"]).abs() < 1e-9,
        "아직 매매가 없다: {trades}"
    );
    assert_eq!(trades["met"], json!(false), "모자란 기준: {trades}");

    let days = line(&template, "trading_days");
    assert!((num(&days["need"]) - 20.0).abs() < 1e-9, "{days}");
    assert_eq!(days["met"], json!(false), "{days}");

    let drawdown = line(&template, "max_drawdown");
    assert!(
        (num(&drawdown["need"]) - 7.5).abs() < 1e-9,
        "스윙 낙폭 15의 절반: {drawdown}"
    );
    assert_eq!(
        drawdown["met"],
        json!(true),
        "낙폭이 없으면 채웠다: {drawdown}"
    );

    let outside = line(&template, "out_of_sample");
    assert_eq!(
        outside["have"],
        Value::Null,
        "표본 밖 성과는 아직 없다: {outside}"
    );
    assert_eq!(outside["met"], json!(false), "{outside}");
    bench.close().await;
}

// 검증: C28 E58 V81
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승격대기_템플릿에는_기다리는_실전_승격_승인_번호가_보인다() {
    let bench = Bench::start().await;
    let (id, number, _) = bench.pending_template();
    let template = entry(&view(&bench).await, id);
    assert_eq!(template["state"], json!("promotion_pending"), "{template}");
    assert_eq!(template["approval"], json!(number), "{template}");
    for f in template["fields"].as_array().expect("칸 목록") {
        assert_eq!(f["editable"], json!(false), "승격대기는 잠겼다: {f}");
    }
    assert_eq!(
        bench.state_of(id),
        TemplateState::PromotionPending,
        "읽기는 아무것도 바꾸지 않는다"
    );
    bench.close().await;
}

// 검증: C28 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기는_원격_세션_없이_읽기와_멈추기가_401이고_세션이_있으면_읽힌다() {
    let bench = Bench::start().await;
    let id = bench.choose("swing").await;
    bench.go_remote();
    let read = bare(bench.addr(), Method::GET, TEMPLATES_VIEW_PATH).await;
    assert_eq!(read.status, 401, "세션 없는 읽기: {}", read.text);
    let stop = bare(bench.addr(), Method::POST, &crate::common::stop_path(id)).await;
    assert_eq!(stop.status, 401, "세션 없는 멈추기: {}", stop.text);

    let cookie = bench.remote_login().await;
    let reply = get(bench.addr(), TEMPLATES_VIEW_PATH, Some(&cookie)).await;
    assert_eq!(reply.status, 200, "원격 세션으로 읽기: {}", reply.text);
    assert_eq!(entry(&reply.body, id)["state"], json!("simulating"));
    bench.close().await;
}
