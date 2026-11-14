//! `auth.json` 읽고 쓰기. `{ "<종류>": { "<칸>": "<값>" } }` 모양으로 둔다.

use serde_json::{Map, Value};
use zeroize::{Zeroize, Zeroizing};

use crate::{AUTH_FILE, Credentials, KeyKind, SecretString, SecretsError};

const CORRUPT: SecretsError = SecretsError::Corrupt { file: AUTH_FILE };

/// JSON 값 안의 글자를 모두 지운다(값이 메모리에 남지 않게).
fn wipe(value: &mut Value) {
    match value {
        Value::String(text) => text.zeroize(),
        Value::Array(items) => items.iter_mut().for_each(wipe),
        Value::Object(map) => map.values_mut().for_each(wipe),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// 글을 JSON 묶음으로 읽는다. 묶음이 아니거나 깨졌으면 `None`.
fn parse_root(text: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Some(map),
        Ok(mut other) => {
            wipe(&mut other);
            None
        }
        Err(_) => None,
    }
}

/// 한 종류의 묶음에서 칸들을 꺼낸다.
fn take_fields(
    entry: &mut Map<String, Value>,
    kind: KeyKind,
) -> Result<Option<Credentials>, SecretsError> {
    let mut found = Credentials::new();
    let mut complete = true;
    for &field in kind.fields() {
        match entry.remove(field) {
            None | Some(Value::Null) => complete = false,
            Some(Value::String(text)) if text.is_empty() => complete = false,
            Some(Value::String(text)) => found.set(field, SecretString::new(text)),
            Some(mut other) => {
                wipe(&mut other);
                return Err(CORRUPT);
            }
        }
    }
    Ok(complete.then_some(found))
}

/// `auth.json` 글에서 한 종류의 값을 읽는다.
///
/// - 그 종류가 없거나 칸이 하나라도 없으면 등록되지 않은 것이다(`Ok(None)`).
/// - JSON이 깨졌거나 모양이 다르면 [`SecretsError::Corrupt`]. 파일 내용은 싣지 않는다.
pub(crate) fn read_kind(text: &str, kind: KeyKind) -> Result<Option<Credentials>, SecretsError> {
    let mut root = parse_root(text).ok_or(CORRUPT)?;
    let result = match root.get_mut(kind.json_key()) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(entry)) => take_fields(entry, kind),
        Some(_) => Err(CORRUPT),
    };
    root.values_mut().for_each(wipe);
    result
}

/// `auth.json` 글에서 한 종류를 새 값으로 바꾼 글을 만든다. 다른 묶음은 그대로 둔다.
///
/// 지금 파일이 없거나(`None`) 읽을 수 없게 깨졌으면 빈 묶음에서 시작한다
/// (확인을 지난 다시 등록이 깨진 파일을 되살리는 길이다).
pub(crate) fn replace_kind(
    text: Option<&str>,
    kind: KeyKind,
    values: &Credentials,
) -> Zeroizing<String> {
    let mut root = text.and_then(parse_root).unwrap_or_default();
    let mut entry = Map::new();
    for &field in kind.fields() {
        if let Some(value) = values.get(field) {
            entry.insert(field.to_owned(), Value::String(value.expose().to_owned()));
        }
    }
    if let Some(mut old) = root.insert(kind.json_key().to_owned(), Value::Object(entry)) {
        wipe(&mut old);
    }
    let mut root = Value::Object(root);
    let mut out = Zeroizing::new(format!("{root:#}"));
    out.push('\n');
    wipe(&mut root);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oauth(access: &str, refresh: &str) -> Credentials {
        Credentials::new()
            .with("access_token", access)
            .with("refresh_token", refresh)
    }

    #[test]
    fn 쓴_글을_읽으면_같은_값이다() {
        let access = "접근 \"토큰\" \\ 0001";
        let text = replace_kind(None, KeyKind::AiOAuth, &oauth(access, "갱신-토큰-0001"));
        let read = read_kind(&text, KeyKind::AiOAuth).unwrap().unwrap();
        assert_eq!(read.get("access_token").unwrap().expose(), access);
        assert_eq!(
            read.get("refresh_token").unwrap().expose(),
            "갱신-토큰-0001"
        );
    }

    #[test]
    fn 바꿀_때_다른_묶음은_남는다() {
        let text = r#"{"other": {"keep": 1}, "ai_oauth": {"access_token": "옛-접근-0001"}}"#;
        let out = replace_kind(
            Some(text),
            KeyKind::AiOAuth,
            &oauth("새-접근-0002", "새-갱신-0002"),
        );
        assert!(out.contains("\"keep\": 1"));
        assert!(!out.contains("옛-접근-0001"));
        assert!(read_kind(&out, KeyKind::AiOAuth).unwrap().is_some());
    }

    #[test]
    fn 깨진_글은_오류고_다시_쓰면_되살아난다() {
        let broken = "{\"ai_oauth\": {\"access_token\": \"잘린";
        assert!(matches!(
            read_kind(broken, KeyKind::AiOAuth),
            Err(SecretsError::Corrupt { file: AUTH_FILE })
        ));
        assert!(matches!(
            read_kind("[1, 2]", KeyKind::AiOAuth),
            Err(SecretsError::Corrupt { .. })
        ));
        let out = replace_kind(
            Some(broken),
            KeyKind::AiOAuth,
            &oauth("새-접근-0002", "새-갱신-0002"),
        );
        assert!(read_kind(&out, KeyKind::AiOAuth).unwrap().is_some());
        assert!(!out.contains("잘린"));
    }

    #[test]
    fn 칸이_빠졌으면_등록되지_않은_것이다() {
        let text = r#"{"ai_oauth": {"access_token": "접근-토큰-0001"}}"#;
        assert!(read_kind(text, KeyKind::AiOAuth).unwrap().is_none());
        assert!(read_kind("{}", KeyKind::AiOAuth).unwrap().is_none());
    }
}
