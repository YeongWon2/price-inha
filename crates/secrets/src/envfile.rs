//! `.env` 읽고 쓰기. `이름=값` 줄로 둔다.
//!
//! 쓰는 규칙:
//! - 공백 · 따옴표 · `#` · `\` · `$` · 백틱이 없는 값은 그대로 쓴다.
//! - 그런 글자가 있으면 큰따옴표로 싸고 `\` · `"` · `$` · 탭을 `\`로 풀어 쓴다.
//!
//! 읽는 규칙은 위 모양에 더해, 손으로 고친 파일에서 흔한 모양
//! (작은따옴표, `export ` 머리, 값 뒤 ` # 설명`)도 받는다.
//! 우리 이름이 아닌 줄과 설명 줄은 건드리지 않고 그대로 둔다.

use zeroize::Zeroizing;

use crate::kind::Entry;
use crate::{Credentials, ENV_FILE, SecretString, SecretsError};

/// 줄의 이름(`=` 앞)과 값 자리(`=` 뒤). 빈 줄 · 설명 줄 · `=`가 없는 줄은 `None`.
fn split_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line
        .strip_prefix("export ")
        .map_or(line, |rest| rest.trim_start());
    let (name, value) = line.split_once('=')?;
    Some((name.trim(), value))
}

/// 값 뒤에 남은 글이 공백이나 설명뿐인가.
fn is_tail(rest: &str) -> bool {
    let rest = rest.trim();
    rest.is_empty() || rest.starts_with('#')
}

/// 큰따옴표 안쪽(여는 따옴표 다음부터)을 푼다. 닫는 따옴표가 없으면 `None`.
fn parse_double_quoted(body: &str) -> Option<Zeroizing<String>> {
    let mut out = Zeroizing::new(String::with_capacity(body.len()));
    let mut chars = body.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return is_tail(&body[at + 1..]).then_some(out),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other @ ('\\' | '"' | '$') => out.push(other),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            },
            other => out.push(other),
        }
    }
    None
}

/// 값 자리를 푼다. 따옴표가 닫히지 않는 등 모양이 깨졌으면 `None`.
fn parse_value(src: &str) -> Option<Zeroizing<String>> {
    let src = src.trim_start();
    if let Some(body) = src.strip_prefix('"') {
        return parse_double_quoted(body);
    }
    if let Some(body) = src.strip_prefix('\'') {
        let (value, rest) = body.split_once('\'')?;
        return is_tail(rest).then(|| Zeroizing::new(value.to_owned()));
    }
    // 따옴표 없는 값: 공백 뒤의 `#`부터는 설명이다.
    let end = src
        .char_indices()
        .zip(src.chars().skip(1))
        .find(|((_, c), next)| c.is_whitespace() && *next == '#')
        .map_or(src.len(), |((at, _), _)| at);
    Some(Zeroizing::new(src[..end].trim_end().to_owned()))
}

/// 값을 `.env`에 쓸 모양으로 바꾼다. [`parse_value`]로 풀면 같은 값이 나온다.
fn encode_value(value: &str) -> Zeroizing<String> {
    let needs_quote = value
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '#' | '\\' | '$' | '`'));
    if !needs_quote {
        return Zeroizing::new(value.to_owned());
    }
    let mut out = Zeroizing::new(String::with_capacity(value.len() + 8));
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' | '"' | '$' => {
                out.push('\\');
                out.push(c);
            }
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// `.env` 글에서 한 묶음(종류 · 칸 · VAPID)의 값을 읽는다.
///
/// - 그 종류의 칸이 하나라도 없거나 비었으면 등록되지 않은 것이다(`Ok(None)`).
/// - 그 종류의 줄이 깨졌으면 [`SecretsError::Corrupt`]. 다른 종류의 줄은 보지 않는다.
/// - 같은 이름이 여러 번 나오면 마지막 줄이 남는다.
pub(crate) fn read_kind(
    text: &str,
    entry: impl Into<Entry>,
) -> Result<Option<Credentials>, SecretsError> {
    let kind = entry.into();
    let mut found = Credentials::new();
    let mut complete = true;
    for (&field, env_name) in kind.fields().iter().zip(kind.env_names()) {
        let last = text
            .lines()
            .filter_map(split_line)
            .rfind(|(name, _)| *name == env_name);
        let Some((_, src)) = last else {
            complete = false;
            continue;
        };
        let value = parse_value(src).ok_or(SecretsError::Corrupt { file: ENV_FILE })?;
        if value.is_empty() {
            complete = false;
            continue;
        }
        found.set(field, SecretString::new(value.as_str()));
    }
    Ok(complete.then_some(found))
}

/// `.env` 글에서 한 종류의 줄을 새 값으로 바꾼 글을 만든다.
/// 다른 줄(다른 종류 · 설명 · 모르는 이름)은 글자 그대로 둔다.
/// `values`는 그 종류의 칸을 모두 가진 것이어야 한다(없는 칸은 쓰지 않는다).
pub(crate) fn replace_kind(
    text: &str,
    entry: impl Into<Entry>,
    values: &Credentials,
) -> Zeroizing<String> {
    let kind = entry.into();
    let names = kind.env_names();
    let mut out = Zeroizing::new(String::with_capacity(text.len() + 256));
    for line in text.lines() {
        let ours = split_line(line).is_some_and(|(name, _)| names.iter().any(|ours| ours == name));
        if !ours {
            out.push_str(line);
            out.push('\n');
        }
    }
    for (&field, env_name) in kind.fields().iter().zip(&names) {
        if let Some(value) = values.get(field) {
            out.push_str(env_name);
            out.push('=');
            out.push_str(&encode_value(value.expose()));
            out.push('\n');
        }
    }
    out
}

/// `.env` 글에서 이름이 `prefix`로 시작하는 줄의 값을 모두 읽는다(선언을 몰라도 된다).
///
/// 같은 이름이 여러 번 나오면 마지막 줄이 남는다. 마지막 줄이 깨졌거나 값이 비었으면 그 이름은 뺀다
/// (읽기는 그 공급자만 [`SecretsError::Corrupt`]로 알리고, 여기서는 다른 줄을 막지 않는다).
pub(crate) fn read_prefixed(text: &str, prefix: &str) -> Vec<(String, Zeroizing<String>)> {
    let mut last: Vec<(&str, &str)> = Vec::new();
    for (name, src) in text.lines().filter_map(split_line) {
        if !name.starts_with(prefix) {
            continue;
        }
        match last.iter_mut().find(|(seen, _)| *seen == name) {
            Some(slot) => slot.1 = src,
            None => last.push((name, src)),
        }
    }
    last.into_iter()
        .filter_map(|(name, src)| {
            let value = parse_value(src)?;
            (!value.is_empty()).then(|| (name.to_owned(), value))
        })
        .collect()
}

/// `.env` 글에서 이름이 `names` 가운데 하나인 줄을 모두 찾는다(같은 이름이 여러 번이면 모두).
///
/// 줄마다 (이름, 값)이다. 값이 깨졌거나 비었으면 값은 `None`이다(줄은 있으므로 지울 대상이다).
/// 지우기 전에 지울 값을 가리기 대상에 올리려고 쓴다.
pub(crate) fn lines_named<S: AsRef<str>>(
    text: &str,
    names: &[S],
) -> Vec<(String, Option<Zeroizing<String>>)> {
    text.lines()
        .filter_map(split_line)
        .filter(|(name, _)| names.iter().any(|ours| ours.as_ref() == *name))
        .map(|(name, src)| {
            let value = parse_value(src).filter(|value| !value.is_empty());
            (name.to_owned(), value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KeyKind;

    #[test]
    fn 이름이_같은_줄은_모두_찾고_깨진_값은_값_없이_찾는다() {
        let text = "A=첫값\n# A=설명\nB=keep\nA=\"깨진\nA=둘째값\nC=\n";
        let found = lines_named(text, &["A", "C"]);
        let shown: Vec<(&str, Option<&str>)> = found
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_ref().map(|v| v.as_str())))
            .collect();
        assert_eq!(
            shown,
            vec![
                ("A", Some("첫값")),
                ("A", None),
                ("A", Some("둘째값")),
                ("C", None)
            ]
        );
    }

    #[test]
    fn 머리로_시작하는_줄은_마지막_값만_읽고_깨진_줄은_뺀다() {
        let text = "PRICE_INHA_KEY__A__K=첫-값-0001\n# 설명\nOTHER=keep\n\
                    PRICE_INHA_KEY__A__K=둘째-값-0002\nPRICE_INHA_KEY__B__K=\"닫히지 않음\n\
                    PRICE_INHA_KEY__C__K=\n";
        let found = read_prefixed(text, "PRICE_INHA_KEY__");
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        assert_eq!(names, vec![("PRICE_INHA_KEY__A__K", "둘째-값-0002")]);
    }

    fn round_trip(value: &str) -> String {
        parse_value(&encode_value(value)).unwrap().to_string()
    }

    #[test]
    fn 쓴_값을_풀면_같은_값이다() {
        for value in [
            "plain-value-0001",
            "한글-값-0001",
            "공백 있는 값",
            "탭\t값",
            "a=b==",
            "=앞",
            "#샵",
            "가 #나",
            "\"큰\" '작은' `백틱`",
            "역슬래시\\n\\t\\",
            "$HOME ${X} \\$",
            "\u{3000}넓은 공백\u{a0}",
        ] {
            assert_eq!(round_trip(value), value);
        }
    }

    #[test]
    fn 특수_글자가_없는_값은_그대로_쓴다() {
        assert_eq!(encode_value("abc-값=1").as_str(), "abc-값=1");
        assert_eq!(encode_value("a b").as_str(), "\"a b\"");
    }

    #[test]
    fn 손으로_고친_모양도_읽는다() {
        let text = "# 설명\n\nexport PRICE_INHA_AI_API_KEY='작은 따옴표 값' # 뒤 설명\n\
                    PRICE_INHA_DATA_API_KEY = raw-value-0001   # 설명\n모르는 줄\n";
        let ai = read_kind(text, KeyKind::AiApiKey).unwrap().unwrap();
        assert_eq!(ai.get("api_key").unwrap().expose(), "작은 따옴표 값");
        let data = read_kind(text, KeyKind::Data).unwrap().unwrap();
        assert_eq!(data.get("api_key").unwrap().expose(), "raw-value-0001");
        assert!(read_kind(text, KeyKind::Broker).unwrap().is_none());
    }

    #[test]
    fn 깨진_줄은_그_종류만_오류다() {
        let text = "PRICE_INHA_AI_API_KEY=\"닫히지 않음\nPRICE_INHA_DATA_API_KEY=raw-value-0001\n";
        assert!(matches!(
            read_kind(text, KeyKind::AiApiKey),
            Err(SecretsError::Corrupt { file: ENV_FILE })
        ));
        assert!(read_kind(text, KeyKind::Data).unwrap().is_some());
    }

    #[test]
    fn 칸이_하나만_있거나_비었으면_등록되지_않은_것이다() {
        let text = "PRICE_INHA_BROKER_CLIENT_ID=only-id-0001\nPRICE_INHA_DATA_API_KEY=\n";
        assert!(read_kind(text, KeyKind::Broker).unwrap().is_none());
        assert!(read_kind(text, KeyKind::Data).unwrap().is_none());
    }

    #[test]
    fn 바꿀_때_다른_줄은_글자_그대로다() {
        let text = "# 머리 설명\nOTHER_NAME=keep me\nPRICE_INHA_DATA_API_KEY=old-value-0001\n\
                    PRICE_INHA_AI_API_KEY=\"ai 값 0001\"";
        let new = Credentials::new().with("api_key", "new value 0002");
        let out = replace_kind(text, KeyKind::Data, &new);
        assert_eq!(
            out.as_str(),
            "# 머리 설명\nOTHER_NAME=keep me\nPRICE_INHA_AI_API_KEY=\"ai 값 0001\"\n\
             PRICE_INHA_DATA_API_KEY=\"new value 0002\"\n"
        );
    }
}
