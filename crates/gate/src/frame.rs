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
    // 다 적은 글을 글자 치환으로 가리지 않는다(JSON이 깨질 수 있다). 값의 나무를 돌며 가린다:
    // 가려진 글자는 언제나 문자열 자리에 들어가고, 뼈대(`seq` · `at`)는 이 모듈이 만든 값이라 그대로다.
    map.insert("kind".to_owned(), Value::String(redactor.redact(&kind)));
    map.insert("payload".to_owned(), redact_value(redactor, payload));
    map.insert("at".to_owned(), at);
    Frame::from(Value::Object(map).to_string())
}

/// 값을 JSON으로 적은 글의 바이트 수. 글을 만들지 않고 센다.
pub(crate) fn json_len(value: &Value) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    match serde_json::to_writer(&mut count, value) {
        Ok(()) => count.0,
        // 세는 쪽은 실패하지 않는다. 그래도 실패하면 "잴 수 없이 크다"로 본다.
        Err(_) => usize::MAX,
    }
}

/// 글자가 아닌 값(숫자 · 불린 · null)을 가린다: JSON으로 적은 글자가 가리기 대상과 겹치면
/// 가려진 **문자열**로 바꾸고, 겹치지 않으면 그대로 둔다.
fn redact_scalar(redactor: &dyn Redactor, value: Value) -> Value {
    let text = value.to_string();
    let redacted = redactor.redact(&text);
    if redacted == text {
        value
    } else {
        Value::String(redacted)
    }
}

/// JSON 값 안의 모든 글자(값과 이름, 숫자로 적힌 값까지)를 가린다. 결과는 언제나 JSON 값이다.
pub(crate) fn redact_value(redactor: &dyn Redactor, value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(redactor.redact(&text)),
        scalar @ (Value::Number(_) | Value::Bool(_) | Value::Null) => {
            redact_scalar(redactor, scalar)
        }
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
    fn 숫자로_적힌_값은_가려진_문자열이_되고_프레임은_json이다() {
        let frame = transient(
            &Mask("123456789"),
            "n",
            json!({ "v": 123456789, "in": 91234567890_u64, "list": [123456789, 5], "ok": true }),
            UnixMillis(1_234_567_890),
        );
        assert!(!frame.as_str().contains("123456789\""));
        let value = parse(&frame);
        assert_eq!(value["payload"]["v"], json!("[x]"));
        assert_eq!(value["payload"]["in"], json!("9[x]0"));
        assert_eq!(value["payload"]["list"], json!(["[x]", 5]));
        assert_eq!(value["payload"]["ok"], json!(true));
        // 뼈대는 이 모듈이 만든 값이라 숫자로 남는다.
        assert_eq!(value["at"], json!(1_234_567_890));
    }

    #[test]
    fn 불린과_null도_겹치면_문자열로_가린다() {
        let frame = transient(
            &Mask("true"),
            "n",
            json!({ "a": true, "b": false, "c": null }),
            UnixMillis(1),
        );
        let value = parse(&frame);
        assert_eq!(
            value["payload"],
            json!({ "a": "[x]", "b": false, "c": null })
        );
    }

    #[test]
    fn 종류_이름도_가린다() {
        let frame = transient(&Mask("hidden"), "k-hidden", json!(1), UnixMillis(1));
        assert_eq!(parse(&frame)["kind"], json!("k-[x]"));
    }

    #[test]
    fn 글의_바이트_수는_적은_글과_같다() {
        for value in [
            json!({ "text": "가나다", "n": [1, 2.5, null] }),
            json!("\"\n"),
        ] {
            assert_eq!(json_len(&value), value.to_string().len());
        }
    }
}
