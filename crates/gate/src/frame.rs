//! 화면으로 나가는 프레임 글자를 만든다. 모든 프레임은 가리기 자리를 거친다.

use axum::extract::ws::Utf8Bytes;
use pi_store::{Event, UnixMillis};
use serde_json::{Map, Value};

use crate::Redactor;

/// 보낼 프레임 하나. 복제해도 글자를 다시 만들지 않는다(연결마다 같은 것을 나눠 쓴다).
pub(crate) type Frame = Utf8Bytes;

/// 저장소에 넣는 모양: 시각과 내용을 함께 싼다(다시 보내도 같은 시각이 나오게).
const STORED_AT: &str = "at";
const STORED_PAYLOAD: &str = "payload";

/// 발행한 내용을 저장할 모양으로 싼다.
pub(crate) fn envelope(at: UnixMillis, payload: Value) -> Value {
    let mut map = Map::new();
    map.insert(STORED_AT.to_owned(), Value::from(at.0));
    map.insert(STORED_PAYLOAD.to_owned(), payload);
    Value::Object(map)
}

/// 저장된 모양을 (시각, 내용)으로 푼다.
///
/// 이 모듈이 싼 모양이 아니면(다른 곳이 저장소에 바로 쌓은 이벤트) 통째로 내용으로 보고
/// 시각은 모른다고(`null`) 한다.
fn open_envelope(stored: Value) -> (Value, Value) {
    match stored {
        Value::Object(mut map)
            if map.len() == 2
                && map.get(STORED_AT).is_some_and(Value::is_i64)
                && map.contains_key(STORED_PAYLOAD) =>
        {
            let at = map.remove(STORED_AT).unwrap_or(Value::Null);
            let payload = map.remove(STORED_PAYLOAD).unwrap_or(Value::Null);
            (at, payload)
        }
        other => (Value::Null, other),
    }
}

/// 저장된 이벤트의 프레임: `{"seq", "kind", "payload", "at"}`.
pub(crate) fn stored(redactor: &dyn Redactor, event: Event) -> Frame {
    let (at, payload) = open_envelope(event.payload);
    build(redactor, Some(event.seq), event.kind, payload, at)
}

/// 지나가는 이벤트의 프레임: `{"kind", "payload", "at"}`. 번호가 없다.
pub(crate) fn transient(
    redactor: &dyn Redactor,
    kind: &str,
    payload: Value,
    at: UnixMillis,
) -> Frame {
    build(redactor, None, kind.to_owned(), payload, Value::from(at.0))
}

fn build(
    redactor: &dyn Redactor,
    seq: Option<u64>,
    kind: String,
    payload: Value,
    at: Value,
) -> Frame {
    let mut map = Map::new();
    if let Some(seq) = seq {
        map.insert("seq".to_owned(), Value::from(seq));
    }
    map.insert("kind".to_owned(), Value::String(kind));
    map.insert("payload".to_owned(), redact_value(redactor, payload));
    map.insert("at".to_owned(), at);
    // 글자 하나하나를 먼저 가리고(JSON으로 적으면 모양이 바뀌는 따옴표 · 역빗금이 든 값도 잡는다),
    // 다 적은 글 전체를 한 번 더 가린다(숫자로 적힌 값 · 종류 이름까지).
    let text = Value::Object(map).to_string();
    Frame::from(redactor.redact(&text))
}

/// JSON 값 안의 모든 글자(값과 이름)를 가린다.
fn redact_value(redactor: &dyn Redactor, value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(redactor.redact(&text)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| redact_value(redactor, item))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(name, item)| (redactor.redact(&name), redact_value(redactor, item)))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::NoRedaction;

    struct Mask(&'static str);

    impl Redactor for Mask {
        fn redact(&self, text: &str) -> String {
            text.replace(self.0, "[x]")
        }
    }

    fn parse(frame: &Frame) -> Value {
        serde_json::from_str(frame.as_str()).unwrap_or(Value::Null)
    }

    #[test]
    fn 저장한_모양을_풀면_시각과_내용이_그대로다() {
        let stored_payload = envelope(UnixMillis(42), json!({ "n": 1 }));
        let event = Event {
            seq: 7,
            kind: "chat".to_owned(),
            payload: stored_payload,
        };
        let frame = parse(&stored(&NoRedaction, event));
        assert_eq!(
            frame,
            json!({ "seq": 7, "kind": "chat", "payload": { "n": 1 }, "at": 42 })
        );
    }

    #[test]
    fn 내용이_at과_payload를_가진_모양이어도_한_겹만_푼다() {
        let inner = json!({ "at": 1, "payload": "속" });
        let event = Event {
            seq: 1,
            kind: "k".to_owned(),
            payload: envelope(UnixMillis(9), inner.clone()),
        };
        let frame = parse(&stored(&NoRedaction, event));
        assert_eq!(frame["payload"], inner);
        assert_eq!(frame["at"], json!(9));
    }

    #[test]
    fn 싸지_않은_이벤트는_통째로_내용이고_시각은_모른다() {
        let event = Event {
            seq: 3,
            kind: "raw".to_owned(),
            payload: json!({ "x": true }),
        };
        let frame = parse(&stored(&NoRedaction, event));
        assert_eq!(frame["payload"], json!({ "x": true }));
        assert_eq!(frame["at"], Value::Null);
    }

    #[test]
    fn 따옴표가_든_값도_가려지고_프레임은_json으로_남는다() {
        let secret = "pa\"ss\\word-0000";
        let mut payload = json!({ "text": format!("key={secret}") });
        payload[secret] = json!([secret]);
        let frame = transient(&Mask(secret), "status", payload, UnixMillis(1));
        assert!(!frame.as_str().contains("ss\\\\word"));
        let value = parse(&frame);
        assert_eq!(value["payload"]["text"], json!("key=[x]"));
        assert_eq!(value["payload"]["[x]"], json!(["[x]"]));
        assert!(value.get("seq").is_none());
    }

    #[test]
    fn 숫자로_적힌_값도_글_전체에서_가린다() {
        let frame = transient(
            &Mask("123456789"),
            "n",
            json!({ "v": 123456789 }),
            UnixMillis(1),
        );
        assert!(!frame.as_str().contains("123456789"));
    }
}
