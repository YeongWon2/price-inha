//! 28차 1번(C29 · V82): 규칙 읽기 API의 모양과 개선안 목록의 앞 → 뒤 값.

use pi_template::{Style, Template, template_view};
use price_inha::{RULES_VIEW_PATH, TEMPLATE_PATH};
use serde_json::{Value, json};

use crate::common::*;

/// 원칙의 절대 상한: `template_view`에서 상한이 있는 칸의 키 → 상한.
fn expected_caps() -> Vec<(&'static str, rust_decimal::Decimal)> {
    template_view(&Template::new(Style::Swing).params)
        .into_iter()
        .filter_map(|field| field.cap.map(|cap| (field.key, cap)))
        .collect()
}

/// 응답의 `caps`가 원칙의 절대 상한 여섯 칸과 같다.
fn caps_are_fixed(view: &Value) {
    let caps = view["caps"]
        .as_object()
        .unwrap_or_else(|| panic!("caps: {view}"));
    let expected = expected_caps();
    assert_eq!(expected.len(), 6, "상한이 있는 칸은 여섯");
    assert_eq!(caps.len(), expected.len(), "상한 칸 수: {view}");
    for (key, cap) in expected {
        assert_eq!(num(&caps[key]), cap, "{key}의 상한: {view}");
    }
}

// 검증: V82 C29
#[tokio::test]
async fn 템플릿이_없으면_빈_규칙과_절대_상한을_준다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let view = rules(&bench).await;
    assert_eq!(view["template"], Value::Null, "{view}");
    assert_eq!(view["fields"], json!([]), "{view}");
    assert_eq!(view["cooling"], json!([]), "{view}");
    assert_eq!(view["recent"], json!([]), "{view}");
    caps_are_fixed(&view);
    bench.close().await;
}

// 검증: V82 C29 R7-3
#[tokio::test]
async fn 스윙을_고르면_지금_규칙의_칸_값_상한이_보인다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    // 지금 경로로 스윙을 고르고 종목 비중을 10%로 맞춘다.
    let template = swing_with(bench.addr(), json!({"symbol_weight_pct": "10"})).await;
    let view = rules(&bench).await;
    assert_eq!(view["template"], json!(template), "{view}");

    let expected = template_view(&params_of(&bench.store(), template));
    let fields = view["fields"]
        .as_array()
        .unwrap_or_else(|| panic!("fields: {view}"));
    let keys: Vec<&str> = fields.iter().filter_map(|f| f["key"].as_str()).collect();
    let want: Vec<&str> = expected.iter().map(|f| f.key).collect();
    assert_eq!(keys, want, "칸 순서는 template_view와 같다");
    for (got, want) in fields.iter().zip(&expected) {
        assert_eq!(got["value"], want.value, "{}의 값", want.key);
        match want.cap {
            Some(cap) => assert_eq!(num(&got["cap"]), cap, "{}의 상한", want.key),
            None => assert_eq!(got["cap"], Value::Null, "{}에는 상한이 없다", want.key),
        }
    }
    assert_eq!(
        num(&field(&view, "symbol_weight_pct")["value"]),
        d("10"),
        "맞춘 값이 지금 규칙이다"
    );
    caps_are_fixed(&view);
    assert_eq!(view["cooling"], json!([]));
    assert_eq!(view["recent"], json!([]));
    bench.close().await;
}

// 검증: V82 C29
#[tokio::test]
async fn 템플릿을_다시_고르면_가장_나중에_고른_템플릿의_규칙이다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    swing_with(bench.addr(), json!({"symbol_weight_pct": "10"})).await;
    let reply = post(bench.addr(), TEMPLATE_PATH, &json!({"style": "day_trade"})).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let latest = reply.body["id"].as_i64().expect("템플릿 번호");

    let view = rules(&bench).await;
    assert_eq!(view["template"], json!(latest), "{view}");
    let expected = template_view(&params_of(&bench.store(), latest));
    assert_eq!(
        field(&view, "symbol_weight_pct")["value"],
        expected
            .iter()
            .find(|f| f.key == "symbol_weight_pct")
            .unwrap()
            .value,
        "새로 고른 템플릿의 값"
    );
    bench.close().await;
}

// 검증: V82 C29 E28
#[tokio::test]
async fn 냉각중인_개선안은_앞_뒤_값과_다음_거래일_반영이_보이고_다음_거래일에_최근으로_옮겨진다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (template, id) = proposed(&bench, json!({"risk_per_trade_pct": "1.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;
    let body = answer_proposal(&bench, id, &number, &fingerprint, "approve").await;
    assert_eq!(body["result"], json!("cooling"), "{body}");

    let view = rules(&bench).await;
    let cooling = view["cooling"]
        .as_array()
        .unwrap_or_else(|| panic!("cooling: {view}"));
    assert_eq!(cooling.len(), 1, "{view}");
    assert_eq!(cooling[0]["proposal"], json!(id.0));
    assert_eq!(cooling[0]["applies_on"], json!("next_trading_day"));
    only_change(
        &cooling[0]["changes"],
        "risk_per_trade_pct",
        "1",
        "1.5",
        true,
    );
    assert_eq!(
        num(&field(&view, "risk_per_trade_pct")["value"]),
        d("1"),
        "오늘은 그대로"
    );
    assert_eq!(view["recent"], json!([]), "아직 반영되지 않았다");

    next_trading_day(&bench).await;
    assert_eq!(risk_pct(&bench.store(), template), d("1.5"));
    let view = rules(&bench).await;
    assert_eq!(
        view["cooling"],
        json!([]),
        "반영되면 냉각중이 아니다: {view}"
    );
    assert_eq!(num(&field(&view, "risk_per_trade_pct")["value"]), d("1.5"));
    let recent = view["recent"]
        .as_array()
        .unwrap_or_else(|| panic!("recent: {view}"));
    assert_eq!(recent.len(), 1, "{view}");
    assert_eq!(recent[0]["proposal"], json!(id.0));
    only_change(
        &recent[0]["changes"],
        "risk_per_trade_pct",
        "1",
        "1.5",
        true,
    );
    bench.close().await;
}

// 검증: V82 C29 V21
#[tokio::test]
async fn 반영된_개선안은_최근에_앞_뒤_값과_반영_시각이_있다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (_, id) = proposed(&bench, json!({"risk_per_trade_pct": "0.5"})).await;
    let (number, fingerprint) = approval_of(&bench, id).await;
    let body = answer_proposal(&bench, id, &number, &fingerprint, "approve").await;
    assert_eq!(body["result"], json!("applied"), "{body}");

    let view = rules(&bench).await;
    assert_eq!(
        num(&field(&view, "risk_per_trade_pct")["value"]),
        d("0.5"),
        "바뀐 값이 보인다"
    );
    assert_eq!(view["cooling"], json!([]));
    let recent = view["recent"]
        .as_array()
        .unwrap_or_else(|| panic!("recent: {view}"));
    assert_eq!(recent.len(), 1, "{view}");
    assert_eq!(recent[0]["proposal"], json!(id.0));
    let applied_at = bench
        .store()
        .get_proposal(id)
        .unwrap()
        .applied_at
        .expect("반영 시각");
    assert_eq!(
        recent[0]["applied_at"],
        json!(applied_at.0),
        "저장소의 반영 시각"
    );
    only_change(
        &recent[0]["changes"],
        "risk_per_trade_pct",
        "1",
        "0.5",
        false,
    );
    bench.close().await;
}

// 검증: V82 C29 V21 E35
#[tokio::test]
async fn 제안됨_거절됨_개선안은_냉각중에도_최근에도_없다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (_, id) = proposed(&bench, json!({"risk_per_trade_pct": "1.5"})).await;
    let view = rules(&bench).await;
    assert_eq!(view["cooling"], json!([]), "답하지 않은 개선안: {view}");
    assert_eq!(view["recent"], json!([]));

    let (number, fingerprint) = approval_of(&bench, id).await;
    let body = answer_proposal(&bench, id, &number, &fingerprint, "reject").await;
    assert_eq!(body["result"], json!("rejected"), "{body}");
    let view = rules(&bench).await;
    assert_eq!(view["cooling"], json!([]), "거절한 개선안: {view}");
    assert_eq!(view["recent"], json!([]));
    assert_eq!(
        num(&field(&view, "risk_per_trade_pct")["value"]),
        d("1"),
        "규칙은 그대로"
    );
    bench.close().await;
}

// 검증: V82 C29
#[tokio::test]
async fn 개선안_목록에_앞_뒤_값과_넓힘이_있다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let (_, id) = proposed(&bench, json!({"risk_per_trade_pct": "1.5"})).await;
    let found = listed(&bench, id).await;
    assert_eq!(found["state"], json!("proposed"));
    assert_eq!(found["increases_risk"], json!(true));
    only_change(&found["changes"], "risk_per_trade_pct", "1", "1.5", true);
    bench.close().await;
}

// 검증: V82 C29
#[tokio::test]
async fn 원격은_세션_없이_규칙을_읽을_수_없고_세션이_있으면_읽는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.go_remote();
    let reply = get_anonymous(bench.addr(), RULES_VIEW_PATH).await;
    assert_eq!(reply.status, 401, "{}", reply.body);

    let cookie = bench.remote_login().await;
    let reply = get_with(bench.addr(), RULES_VIEW_PATH, &cookie).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["template"], Value::Null, "{}", reply.body);
    caps_are_fixed(&reply.body);
    bench.close().await;
}
