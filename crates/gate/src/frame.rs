//! 화면으로 나가는 프레임 글자를 만든다. 모든 프레임은 가리기 자리를 거친다.

use axum::extract::ws::Utf8Bytes;
use pi_store::{TimedEvent, UnixMillis};
use serde_json::{Map, Value};

use crate::Redactor;

/// 보낼 프레임 하나. 복제해도 글자를 다시 만들지 않는다(연결마다 같은 것을 나눠 쓴다).
pub(crate) type Frame = Utf8Bytes;

/// 3차까지 저장소에 넣던 시각 봉투의 칸 이름. 4차부터는 싸지 않고, 옛 줄을 풀 때만 쓴다.
const ENVELOPE_AT: &str = "at";
const ENVELOPE_PAYLOAD: &str = "payload";

/// 지나가는 이벤트의 이름을 저장소처럼 다듬는다(5차 2번, 원본 Hermes
/// `gateway/hosted_rooms_common.py:23-32`): 앞뒤 공백을 떼고 [`kind_follows_rule`]로 본다.
/// 맞으면 뗀 이름, 아니면 `None`.
pub(crate) fn trimmed_kind(kind: &str) -> Option<&str> {
    let trimmed = kind.trim_matches(is_strip_space);
    kind_follows_rule(trimmed).then_some(trimmed)
}

/// 저장소(`pi-store`)가 떼는 공백과 같다: Python `str.isspace`처럼 유니코드 공백에
/// 정보 구분 문자(U+001C ~ U+001F)를 더한 것.
fn is_strip_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// 이벤트 종류 이름 규칙(C9, 저장소와 같다): `^[a-z][a-z0-9_.-]*$`이고
/// [`pi_store::EVENT_KIND_MAX_CHARS`]자 이하. 받은 글자 그대로 본다.
pub(crate) fn kind_follows_rule(kind: &str) -> bool {
    let mut chars = kind.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '.' | '-'))
        && kind.chars().count() <= pi_store::EVENT_KIND_MAX_CHARS
}

/// 저장된 줄을 (시각, 내용)으로 본다.
///
/// - 4차부터 발행한 줄: 시각은 시각 칸, 내용은 저장된 그대로다.
/// - 3차까지 싼 봉투 줄(`{"at": 정수, "payload": …}` 두 칸): 시각 칸이 비었거나(봉투 그대로 쌓인 줄)
///   봉투의 시각과 같으면(버전 3으로 올라오며 시각 칸이 채워진 줄) 한 겹 풀어 예전과 같게 보낸다.
///   (5차) 번호가 저장소의 옛 형식 경계(`legacy_until`) 이하인 줄만 푼다. 경계가 없으면 풀지 않는다.
fn time_and_payload(
    seq: u64,
    legacy_until: Option<u64>,
    column: Option<UnixMillis>,
    stored: Value,
) -> (Value, Value) {
    let column_at = column.map(|at| Value::from(at.0));
    let legacy = legacy_until.is_some_and(|until| seq <= until);
    match stored {
        Value::Object(mut map)
            if legacy
                && map.len() == 2
                && map.contains_key(ENVELOPE_PAYLOAD)
                && map.get(ENVELOPE_AT).is_some_and(Value::is_i64)
                && column_at
                    .as_ref()
                    .is_none_or(|at| map.get(ENVELOPE_AT) == Some(at)) =>
        {
            let at = map.remove(ENVELOPE_AT).unwrap_or(Value::Null);
            let payload = map.remove(ENVELOPE_PAYLOAD).unwrap_or(Value::Null);
            (at, payload)
        }
        other => (column_at.unwrap_or(Value::Null), other),
    }
}

/// 저장된 이벤트의 프레임: `{"seq", "kind", "payload", "at"}`. 종류는 저장소가 저장한 이름이다.
/// `legacy_until`은 저장소의 옛 형식 경계 번호다(`pi_store::Store::legacy_envelope_until`).
pub(crate) fn stored(
    redactor: &dyn Redactor,
    event: TimedEvent,
    legacy_until: Option<u64>,
) -> Frame {
    let (at, payload) = time_and_payload(event.seq, legacy_until, event.at, event.payload);
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

    fn timed(seq: u64, at: Option<i64>, payload: Value) -> TimedEvent {
        TimedEvent {
            seq,
            kind: "chat".to_owned(),
            at: at.map(UnixMillis),
            payload,
        }
    }

    #[test]
    fn 시각_칸의_시각과_저장된_내용_그대로_보낸다() {
        let frame = parse(&stored(
            &NoRedaction,
            timed(7, Some(42), json!({ "n": 1 })),
            Some(100),
        ));
        assert_eq!(
            frame,
            json!({ "seq": 7, "kind": "chat", "payload": { "n": 1 }, "at": 42 })
        );
        let frame = parse(&stored(
            &NoRedaction,
            timed(3, None, json!({ "x": true })),
            Some(100),
        ));
        assert_eq!(frame["payload"], json!({ "x": true }));
        assert_eq!(frame["at"], Value::Null);
    }

    #[test]
    fn 옛_봉투_줄은_한_겹만_푼다() {
        let inner = json!({ "at": 1, "payload": "속" });
        let old = json!({ "at": 9, "payload": inner.clone() });
        for column in [None, Some(9)] {
            let frame = parse(&stored(
                &NoRedaction,
                timed(1, column, old.clone()),
                Some(1),
            ));
            assert_eq!(frame["payload"], inner);
            assert_eq!(frame["at"], json!(9));
        }
    }

    #[test]
    fn 경계_뒤나_경계가_없으면_봉투_모양이어도_풀지_않는다() {
        let old = json!({ "at": 9, "payload": "속" });
        for (seq, until) in [(2, Some(1)), (1, None), (1, Some(0))] {
            for column in [None, Some(9)] {
                let frame = parse(&stored(
                    &NoRedaction,
                    timed(seq, column, old.clone()),
                    until,
                ));
                assert_eq!(frame["payload"], old, "{seq}번, 경계 {until:?}");
                assert_eq!(frame["at"], column.map_or(Value::Null, |at| json!(at)));
            }
        }
    }

    #[test]
    fn 지나가는_이름은_저장소처럼_앞뒤_공백을_뗀다() {
        assert_eq!(trimmed_kind("\u{1c} quotes\t\u{3000}"), Some("quotes"));
        assert_eq!(trimmed_kind("   "), None);
        assert_eq!(trimmed_kind(" Chat "), None);
        assert_eq!(trimmed_kind("\u{200b}chat"), None);
    }

    #[test]
    fn 시각_칸과_다른_봉투_모양의_내용은_내용_그대로다() {
        let payload = json!({ "at": 5, "payload": 1 });
        let frame = parse(&stored(
            &NoRedaction,
            timed(2, Some(42), payload.clone()),
            Some(100),
        ));
        assert_eq!(frame["payload"], payload);
        assert_eq!(frame["at"], json!(42));
    }

    #[test]
    fn 종류_이름_규칙() {
        assert!(kind_follows_rule("run.started"));
        assert!(!kind_follows_rule(" chat"));
        assert!(!kind_follows_rule(
            &"a".repeat(pi_store::EVENT_KIND_MAX_CHARS + 1)
        ));
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
