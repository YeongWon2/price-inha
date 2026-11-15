//! 답 글 안의 JSON 행동 해석(결정 12번).

use pi_agent::{ActionCall, Reply, find_object_with, parse_reply};
use serde_json::json;

fn call(name: &str, args: serde_json::Value) -> ActionCall {
    ActionCall {
        name: name.into(),
        args,
    }
}

// 검증: 결정 12 (행동 없는 답은 최종 답)
#[test]
fn 행동이_없으면_최종_답이다() {
    assert_eq!(
        parse_reply("  오늘은 쉽니다.\n"),
        Reply::Final("오늘은 쉽니다.".into())
    );
    let score = "점수입니다 {\"score\": 70, \"reason\": \"좋다\"}";
    assert_eq!(parse_reply(score), Reply::Final(score.into()));
}

// 검증: 결정 12 (빈 답)
#[test]
fn 비었거나_공백뿐이면_빈_답이다() {
    assert_eq!(parse_reply(""), Reply::Empty);
    assert_eq!(parse_reply(" \n\t "), Reply::Empty);
}

// 검증: 결정 12 (산문 · 코드펜스 안의 행동, 나온 순서)
#[test]
fn 산문과_코드펜스_안의_행동을_순서대로_찾는다() {
    let text = "시세부터 봅니다.\n```json\n{\"action\": \"quote\", \"args\": {\"symbol\": \"005930\"}}\n```\n\
                그리고 공시도.\n{\"action\": \"filing\", \"args\": {\"days\": 3, \"note\": \"{괄호}\"}}\n끝.";
    assert_eq!(
        parse_reply(text),
        Reply::Actions(vec![
            call("quote", json!({"symbol": "005930"})),
            call("filing", json!({"days": 3, "note": "{괄호}"})),
        ])
    );
}

// 검증: 결정 12 (args가 없으면 빈 객체)
#[test]
fn 인자가_없으면_빈_객체다() {
    assert_eq!(
        parse_reply("{\"action\": \"quote\"}"),
        Reply::Actions(vec![call("quote", json!({}))])
    );
}

// 검증: E6 결정 12 (JSON이 깨진 경우를 표로)
#[test]
fn 깨진_행동은_표대로_거부한다() {
    let cases = [
        (
            "균형이 안 맞음",
            "{\"action\": \"quote\", \"args\": {\"symbol\": \"005930\"}",
        ),
        ("JSON이 아님", "{\"action\": quote}"),
        ("action이 글이 아님", "{\"action\": 5, \"args\": {}}"),
        ("action이 빈 글", "{\"action\": \"  \", \"args\": {}}"),
        (
            "args가 객체가 아님",
            "{\"action\": \"quote\", \"args\": [1, 2]}",
        ),
        (
            "멀쩡한 것과 깨진 것이 섞임",
            "{\"action\": \"quote\"}\n{\"action\": \"filing\", \"args\": 3}",
        ),
    ];
    for (why, text) in cases {
        assert_eq!(parse_reply(text), Reply::Malformed, "{why}");
    }
}

// 검증: V3 결정 15 (정해진 JSON 찾기)
#[test]
fn 키가_있는_첫_객체를_찾는다() {
    let text = "앞말 {\"other\": 1} 그리고 ```json\n{\"decision\": \"buy\", \"entry\": 70000}\n``` \
                {\"decision\": \"watch\"}";
    let found = find_object_with(text, "decision").unwrap();
    assert_eq!(found["decision"], json!("buy"));
    assert_eq!(found["entry"], json!(70000));
    assert!(find_object_with(text, "score").is_none());
    assert!(find_object_with("{\"decision\": ", "decision").is_none());
}
