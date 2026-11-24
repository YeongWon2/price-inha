//! (25차 2번) 팀장 판단 글(방 로그 원문)을 사람이 읽는 문장으로 바꾼다([`crate::leader_sentence`]).
//!
//! 읽는 규칙은 팀이 판단을 읽는 규칙과 같다: 글 안에서 `plan` · `decision` 칸이 있는 JSON 객체를 찾고, 칸의 형이
//! 틀리면 판단 모양이 아니다(`None` — 부르는 쪽이 원문을 그대로 쓴다).

use pi_market::Market;
use serde_json::{Map, Value};

/// 쉬는 판단에 이유가 비었을 때(팀의 하루 판단 읽기와 같은 말).
const REST_DEFAULT: &str = "팀장이 쉬기로 정했다";

/// 판단 글을 문장으로. 판단 모양이 아니면 `None`.
pub(super) fn sentence(text: &str, market: Market) -> Option<String> {
    if let Some(object) = pi_agent::find_object_with(text, "decision") {
        return verdict(&object, market);
    }
    let object = pi_agent::find_object_with(text, "plan")?;
    day_plan(&object)
}

/// 하루 판단: `trade` · `rest`.
fn day_plan(object: &Map<String, Value>) -> Option<String> {
    match object.get("plan")?.as_str()?.trim() {
        "trade" => Some("오늘은 운용합니다.".to_string()),
        "rest" => {
            let reason = text_field(object, "reason")?.unwrap_or_else(|| REST_DEFAULT.to_string());
            Some(format!("오늘은 쉽니다 — {reason}"))
        }
        _ => None,
    }
}

/// 종목 판단: `buy` · `sell` · `watch`.
fn verdict(object: &Map<String, Value>, market: Market) -> Option<String> {
    let decision = object
        .get("decision")?
        .as_str()?
        .trim()
        .to_ascii_lowercase();
    match decision.as_str() {
        "buy" => trade(object, "사기로 정했습니다", market),
        "sell" => trade(object, "팔기로 정했습니다", market),
        "watch" => {
            let reason = object.get("reason")?.as_str()?.trim();
            object.get("revisit")?.as_str()?;
            if reason.is_empty() {
                Some("지켜보기로 했습니다.".to_string())
            } else {
                Some(format!("지켜보기로 했습니다 — {reason}"))
            }
        }
        _ => None,
    }
}

/// 사기 · 팔기: "{머리} — 진입 … · 손절 … · 목표 …. {가설}".
fn trade(object: &Map<String, Value>, head: &str, market: Market) -> Option<String> {
    let hypothesis = text_field(object, "hypothesis")?;
    text_field(object, "trigger")?;
    amount_field(object, "expected_value")?;
    let entry = amount_field(object, "entry")?;
    let stop = amount_field(object, "stop")?;
    let target = amount_field(object, "target")?;
    let show = |amount: Option<i64>| amount.map_or_else(|| "-".to_string(), |a| money(a, market));
    let mut line = format!(
        "{head} — 진입 {} · 손절 {} · 목표 {}.",
        show(entry),
        show(stop),
        show(target)
    );
    if let Some(hypothesis) = hypothesis {
        line.push(' ');
        line.push_str(&hypothesis);
    }
    Some(line)
}

/// 글 칸. 바깥 `None`은 형이 틀림, 안쪽 `None`은 빈 칸.
fn text_field(object: &Map<String, Value>, key: &str) -> Option<Option<String>> {
    match object.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(text)) => {
            let text = text.trim();
            Some((!text.is_empty()).then(|| text.to_string()))
        }
        Some(_) => None,
    }
}

/// 금액 칸(통화의 최소 단위, 정수). 바깥 `None`은 형이 틀림, 안쪽 `None`은 빈 칸.
fn amount_field(object: &Map<String, Value>, key: &str) -> Option<Option<i64>> {
    match object.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(value) => value.as_i64().map(Some),
    }
}

/// 그 시장의 돈 모양: 국내는 세 자리마다 쉼표, 미국은 센트 ÷ 100의 소수 둘째 자리(정수 부분은 쉼표).
fn money(amount: i64, market: Market) -> String {
    let sign = if amount < 0 { "-" } else { "" };
    let abs = amount.unsigned_abs();
    match market {
        Market::Us => format!("{sign}{}.{:02}", grouped(abs / 100), abs % 100),
        _ => format!("{sign}{}", grouped(abs)),
    }
}

/// 세 자리마다 쉼표.
fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 돈_모양() {
        assert_eq!(money(0, Market::Kr), "0");
        assert_eq!(money(999, Market::Kr), "999");
        assert_eq!(money(1_000, Market::Kr), "1,000");
        assert_eq!(money(5, Market::Us), "0.05");
        assert_eq!(money(12_345_678, Market::Us), "123,456.78");
    }

    #[test]
    fn 빈_이유와_빈_금액() {
        assert_eq!(
            sentence(r#"{"plan":"rest"}"#, Market::Kr).as_deref(),
            Some("오늘은 쉽니다 — 팀장이 쉬기로 정했다")
        );
        assert_eq!(
            sentence(r#"{"decision":"buy","entry":100}"#, Market::Kr).as_deref(),
            Some("사기로 정했습니다 — 진입 100 · 손절 - · 목표 -.")
        );
        assert_eq!(
            sentence(r#"{"decision":"buy","entry":"100"}"#, Market::Kr),
            None
        );
    }
}
