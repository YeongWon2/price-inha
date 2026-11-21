//! 팀장 판단 · 매매 담당 주문 안 답 읽기(만드는 것 3-3 · 3-4).

use std::str::FromStr;

use pi_store::{Amount, Rationale};
use rust_decimal::Decimal;
use serde_json::{Map, Value};

use crate::action::find_object_with;
use crate::ports::Side;

/// 팀장 판단.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// 사기 · 팔기와 근거(빈 칸이 있을 수 있다).
    Trade {
        /// 방향.
        side: Side,
        /// 근거.
        rationale: Rationale,
    },
    /// 지켜보기.
    Watch {
        /// 이유.
        reason: String,
        /// 다시 볼 조건.
        revisit: String,
    },
}

impl Verdict {
    /// 방 로그의 `decision` 값.
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Verdict::Trade {
                side: Side::Buy, ..
            } => "buy",
            Verdict::Trade {
                side: Side::Sell, ..
            } => "sell",
            Verdict::Watch { .. } => "watch",
        }
    }
}

/// 팀장의 답을 읽는다. 모양이 틀리면 `None`(팀장 응답 실패).
///
/// 근거 칸이 없거나 `null`이거나 공백뿐인 글이면 빈 칸이다. 칸의 형이 틀리면(글 칸이 글이 아님,
/// 금액 칸이 정수가 아님) 모양이 틀린 것이다.
pub(crate) fn parse_verdict(text: &str) -> Option<Verdict> {
    let object = find_object_with(text, "decision")?;
    let decision = object
        .get("decision")?
        .as_str()?
        .trim()
        .to_ascii_lowercase();
    match decision.as_str() {
        "buy" => trade(&object, Side::Buy),
        "sell" => trade(&object, Side::Sell),
        "watch" => Some(Verdict::Watch {
            reason: object.get("reason")?.as_str()?.trim().to_string(),
            revisit: object.get("revisit")?.as_str()?.trim().to_string(),
        }),
        _ => None,
    }
}

/// 근거 칸이 모두 찼는가(P9). 금액 칸은 0보다 커야 찬 것이다.
pub(crate) fn is_complete(rationale: &Rationale) -> bool {
    let positive = |amount: Option<Amount>| amount.is_some_and(|a| a.0 > 0);
    rationale.hypothesis.is_some()
        && rationale.trigger.is_some()
        && positive(rationale.entry)
        && positive(rationale.stop)
        && positive(rationale.target)
        && positive(rationale.expected_value)
}

/// 매매 담당의 주문 안 답.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DraftReply {
    /// 설명.
    pub(crate) note: String,
    /// 적은 수량(참고값).
    pub(crate) ai_quantity: Option<Decimal>,
    /// 적은 가격(참고값).
    pub(crate) ai_price: Option<Decimal>,
}

/// 매매 담당의 주문 안 답을 읽는다: 방향이 `buy`인 객체만 된다.
/// 수량 · 가격은 0보다 큰 숫자일 때만 참고값으로 담고, 아니면 버린다(P7).
pub(crate) fn parse_draft(text: &str) -> Option<DraftReply> {
    parse_draft_as(text, "buy")
}

/// 매매 담당의 팔기 주문 안 답을 읽는다: 방향이 `sell`인 객체만 된다(9차). 나머지는 [`parse_draft`]와 같다.
pub(crate) fn parse_sell_draft(text: &str) -> Option<DraftReply> {
    parse_draft_as(text, "sell")
}

/// 방향이 `expected`인 주문 안 답 객체를 읽는다.
fn parse_draft_as(text: &str, expected: &str) -> Option<DraftReply> {
    let object = find_object_with(text, "side")?;
    let side = object.get("side")?.as_str()?.trim().to_ascii_lowercase();
    if side != expected {
        return None;
    }
    let note = match object.get("note") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(note)) => note.trim().to_string(),
        Some(_) => return None,
    };
    Some(DraftReply {
        note,
        ai_quantity: reference(&object, "quantity"),
        ai_price: reference(&object, "price"),
    })
}

/// 참고값 칸: 0보다 큰 숫자만 된다.
fn reference(object: &Map<String, Value>, key: &str) -> Option<Decimal> {
    let Value::Number(number) = object.get(key)? else {
        return None;
    };
    let text = number.to_string();
    let value = Decimal::from_str(&text)
        .or_else(|_| Decimal::from_scientific(&text))
        .ok()?;
    (value > Decimal::ZERO).then_some(value)
}

fn trade(object: &Map<String, Value>, side: Side) -> Option<Verdict> {
    let rationale = Rationale {
        hypothesis: text_field(object, "hypothesis")?,
        trigger: text_field(object, "trigger")?,
        entry: amount_field(object, "entry")?,
        stop: amount_field(object, "stop")?,
        target: amount_field(object, "target")?,
        expected_value: amount_field(object, "expected_value")?,
    };
    Some(Verdict::Trade { side, rationale })
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

/// 금액 칸(원, 정수). 바깥 `None`은 형이 틀림, 안쪽 `None`은 빈 칸.
fn amount_field(object: &Map<String, Value>, key: &str) -> Option<Option<Amount>> {
    match object.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(value) => value.as_i64().map(|won| Some(Amount(won))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 팔기도_근거를_읽는다() {
        let text = "{\"decision\": \"SELL\", \"hypothesis\": \"고점\", \"entry\": 1}";
        let expected = Rationale {
            hypothesis: Some("고점".into()),
            entry: Some(Amount(1)),
            ..Rationale::default()
        };
        assert!(!is_complete(&expected));
        let trade = Verdict::Trade {
            side: Side::Sell,
            rationale: expected,
        };
        assert_eq!(parse_verdict(text), Some(trade));
    }

    #[test]
    fn 지켜보기는_이유와_조건이_글이어야_한다() {
        assert!(parse_verdict("{\"decision\": \"watch\", \"reason\": \"x\"}").is_none());
        assert!(parse_draft("{\"side\": \"buy\", \"note\": 3}").is_none());
        let empty = parse_draft("{\"side\": \"buy\"}").unwrap();
        assert_eq!(empty.note, "");
    }

    #[test]
    fn 사기_답과_팔기_답은_서로_받지_않는다() {
        let sell = "{\"side\": \"Sell\", \"note\": \" 정리 \", \"quantity\": 2}";
        assert!(parse_draft(sell).is_none());
        let reply = parse_sell_draft(sell).unwrap();
        assert_eq!(reply.note, "정리");
        assert_eq!(reply.ai_quantity, Some(Decimal::from(2)));
        assert!(parse_sell_draft("{\"side\": \"buy\", \"note\": \"x\"}").is_none());
    }
}
