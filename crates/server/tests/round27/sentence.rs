//! 2번: 팀장 판단 글(방 로그 원문)을 사람이 읽는 문장으로([`leader_sentence`]). 하루 판단 운용 · 쉼, 종목 판단
//! 사기 · 팔기 · 지켜보기. 금액은 그 시장 돈 모양(국내 세 자리 쉼표, 미국 센트 ÷ 100). 판단 모양이 아니면 `None`
//! (부르는 쪽이 원문 그대로 쓴다).

use pi_market::Market;
use price_inha::leader_sentence;
use serde_json::json;

/// 사기 · 팔기 판단 글(국내 금액, 원).
fn trade(decision: &str, hypothesis: &str) -> String {
    json!({
        "decision": decision,
        "hypothesis": hypothesis,
        "trigger": "시연 계기",
        "entry": 71_100,
        "stop": 67_545,
        "target": 78_210,
        "expected_value": 3_555
    })
    .to_string()
}

// 검증: V14 R5-2
#[test]
fn 하루_판단_운용은_오늘은_운용합니다로_읽힌다() {
    assert_eq!(
        leader_sentence(r#"{"plan": "trade"}"#, Market::Kr).as_deref(),
        Some("오늘은 운용합니다.")
    );
}

// 검증: V14 R5-2
#[test]
fn 하루_판단_쉼은_이유를_붙여_오늘은_쉽니다로_읽힌다() {
    let text = json!({"plan": "rest", "reason": "시장이 크게 흔들린다"}).to_string();
    assert_eq!(
        leader_sentence(&text, Market::Kr).as_deref(),
        Some("오늘은 쉽니다 — 시장이 크게 흔들린다")
    );
}

// 검증: V14 R5-2
#[test]
fn 사기_판단은_진입_손절_목표와_가설을_국내_돈_모양으로_읽힌다() {
    assert_eq!(
        leader_sentence(&trade("buy", "외국인 매수가 이어진다"), Market::Kr).as_deref(),
        Some("사기로 정했습니다 — 진입 71,100 · 손절 67,545 · 목표 78,210. 외국인 매수가 이어진다")
    );
}

// 검증: V14 R5-2
#[test]
fn 팔기_판단도_같은_모양으로_읽힌다() {
    assert_eq!(
        leader_sentence(&trade("sell", "목표가에 닿았다"), Market::Kr).as_deref(),
        Some("팔기로 정했습니다 — 진입 71,100 · 손절 67,545 · 목표 78,210. 목표가에 닿았다")
    );
}

// 검증: V14 R5-2
#[test]
fn 지켜보기_판단은_이유를_붙여_읽힌다() {
    let text = json!({"decision": "watch", "reason": "거래량이 적다", "revisit": "거래량이 늘면"})
        .to_string();
    assert_eq!(
        leader_sentence(&text, Market::Kr).as_deref(),
        Some("지켜보기로 했습니다 — 거래량이 적다")
    );
}

// 검증: V14 R5-2
#[test]
fn 국내_금액은_세_자리마다_쉼표를_찍는다() {
    let text = json!({
        "decision": "buy", "hypothesis": "가설", "trigger": "계기",
        "entry": 1_234_567, "stop": 950, "target": 1_000, "expected_value": 10
    })
    .to_string();
    assert_eq!(
        leader_sentence(&text, Market::Kr).as_deref(),
        Some("사기로 정했습니다 — 진입 1,234,567 · 손절 950 · 목표 1,000. 가설")
    );
}

// 검증: V14 R5-2
#[test]
fn 미국_금액은_센트를_달러로_소수_둘째_자리까지_쓴다() {
    let text = json!({
        "decision": "buy", "hypothesis": "실적 발표 뒤 강세", "trigger": "계기",
        "entry": 18_720, "stop": 17_784, "target": 20_592, "expected_value": 936
    })
    .to_string();
    let sentence = leader_sentence(&text, Market::Us).expect("판단 모양이면 문장이다");
    assert!(
        sentence.starts_with("사기로 정했습니다 — 진입 "),
        "같은 모양이다: {sentence}"
    );
    for amount in ["187.20", "177.84", "205.92"] {
        assert!(sentence.contains(amount), "{amount}가 보인다: {sentence}");
    }
    assert!(
        !sentence.contains("18,720") && !sentence.contains("18720"),
        "센트 그대로 쓰지 않는다: {sentence}"
    );
    assert!(
        sentence.ends_with("실적 발표 뒤 강세"),
        "가설이 끝에 온다: {sentence}"
    );
}

// 검증: V14 R5-2
#[test]
fn 판단_모양이_아닌_글은_문장으로_바꾸지_않는다() {
    for text in [
        "",
        "(pass)",
        "오늘은 시장을 좀 더 보자",
        r#"{"decision": "#,
        r#"{"score": 70, "reason": "점수"}"#,
        r#"{"plan": "maybe"}"#,
        r#"{"decision": "hold", "reason": "모름"}"#,
    ] {
        assert_eq!(
            leader_sentence(text, Market::Kr),
            None,
            "판단 모양이 아니면 원문 그대로 쓴다: {text:?}"
        );
    }
}
