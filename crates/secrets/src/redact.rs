//! 글에서 비밀 값을 가린다 (E38 · N8).
//!
//! 값 하나마다 세 가지 모양을 찾는다.
//! - 그대로 든 값.
//! - URL 인코딩된 값: 바이트마다 그대로든 `%XX`(대소문자 모두)든 받는다. 어떤 글자를
//!   인코딩하는지는 보내는 쪽마다 달라서, 일부만 인코딩된 모양도 잡는다. 공백 자리의 `+`도 받는다.
//! - Base64로 든 값: 표준 · URL 안전, 채움 있음 · 없음. 값이 더 긴 글의 일부로 인코딩된 경우
//!   (예: `아이디:비밀`을 통째로 인코딩한 Basic 인증)도 세 가지 자리 맞춤을 모두 찾아 가린다.

use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use zeroize::Zeroizing;

use crate::{MIN_SECRET_CHARS, REDACTED};

/// 값이 Base64 흐름 안에 들어갔을 때 값만으로 정해지는 글자들.
struct B64Core {
    text: Zeroizing<String>,
    /// 바로 앞 글자에 값의 비트가 섞여 있다(앞 글자도 가린다).
    lead: bool,
    /// 바로 뒤 글자에 값의 비트가 섞여 있다(뒤 글자와 채움도 가린다).
    trail: bool,
}

/// 가릴 값 하나. 버려질 때 값을 지운다.
pub(crate) struct Pattern {
    raw: Zeroizing<Vec<u8>>,
    b64: Vec<B64Core>,
}

/// 값이 흐름의 `offset`(0 · 1 · 2)번째 바이트 자리에서 시작할 때의 [`B64Core`]들(표준 · URL 안전).
fn b64_cores(value: &[u8], offset: usize) -> Vec<B64Core> {
    let mut padded = Zeroizing::new(vec![0_u8; offset]);
    padded.extend_from_slice(value);
    let encoded = Zeroizing::new(STANDARD_NO_PAD.encode(padded.as_slice()));
    // 앞: 채운 바이트만으로 정해지는 글자와 값이 섞인 글자 하나를 뺀다.
    let lead_drop = [0, 2, 3][offset];
    // 뒤: 마지막 묶음이 덜 찼으면 끝 글자는 뒤따르는 바이트와 섞인다.
    let trail = !padded.len().is_multiple_of(3);
    let end = encoded.len() - usize::from(trail);
    let Some(core) = encoded.get(lead_drop..end).filter(|core| !core.is_empty()) else {
        return Vec::new();
    };
    let lead = offset != 0;
    let url_safe: String = core
        .chars()
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            other => other,
        })
        .collect();
    let mut cores = vec![B64Core {
        text: Zeroizing::new(core.to_owned()),
        lead,
        trail,
    }];
    if url_safe != core {
        cores.push(B64Core {
            text: Zeroizing::new(url_safe),
            lead,
            trail,
        });
    }
    cores
}

fn is_b64_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'-' | b'_')
}

/// `at` 자리의 `%XX`가 나타내는 바이트.
fn percent_byte(text: &[u8], at: usize) -> Option<u8> {
    if text.get(at) != Some(&b'%') {
        return None;
    }
    let hex = |byte: &u8| char::from(*byte).to_digit(16);
    let high = text.get(at + 1).and_then(hex)?;
    let low = text.get(at + 2).and_then(hex)?;
    u8::try_from(high * 16 + low).ok()
}

/// `text`의 `at`에서 `byte` 하나를 읽을 수 있는 길들: 읽은 뒤의 자리(그대로 · `%XX`).
fn step(text: &[u8], at: usize, byte: u8) -> [Option<usize>; 2] {
    let plain = text
        .get(at)
        .is_some_and(|&found| found == byte || (byte == b' ' && found == b'+'));
    let encoded = percent_byte(text, at) == Some(byte);
    [plain.then_some(at + 1), encoded.then_some(at + 3)]
}

/// `start`에서 `secret`이 그대로든 URL 인코딩으로든 통째로 나오면 그 끝 자리.
///
/// 값에 `%`가 있으면 한 자리에서 두 길이 모두 열릴 수 있어, 가능한 자리들을 함께 따라간다.
fn match_at(text: &[u8], start: usize, secret: &[u8]) -> Option<usize> {
    let (&first, rest) = secret.split_first()?;
    let mut current: Vec<usize> = step(text, start, first).into_iter().flatten().collect();
    for &byte in rest {
        if current.is_empty() {
            return None;
        }
        let mut next: Vec<usize> = current
            .iter()
            .flat_map(|&at| step(text, at, byte))
            .flatten()
            .collect();
        next.sort_unstable();
        next.dedup();
        current = next;
    }
    current.into_iter().max()
}

impl Pattern {
    /// 값 하나를 가릴 모양으로 만든다. [`MIN_SECRET_CHARS`]보다 짧은 값은 `None`이다
    /// (짧은 값은 흔한 글자와 겹쳐 글을 망가뜨린다. 등록도 읽기도 그런 값을 받지 않는다).
    pub(crate) fn new(value: &str) -> Option<Self> {
        if value.chars().count() < MIN_SECRET_CHARS {
            return None;
        }
        let raw = Zeroizing::new(value.as_bytes().to_vec());
        let b64 = (0..3).flat_map(|offset| b64_cores(&raw, offset)).collect();
        Some(Self { raw, b64 })
    }

    /// 이 모양이 `value`를 가리는 것인가.
    pub(crate) fn is(&self, value: &str) -> bool {
        self.raw.as_slice() == value.as_bytes()
    }

    /// 이 값이 든 자리들을 `spans`에 더한다.
    fn find_in(&self, text: &str, spans: &mut Vec<(usize, usize)>) {
        let bytes = text.as_bytes();
        let mut start = 0;
        while start < bytes.len() {
            match match_at(bytes, start, &self.raw) {
                Some(end) => {
                    spans.push((start, end));
                    start = end.max(start + 1);
                }
                None => start += 1,
            }
        }
        for core in &self.b64 {
            for (at, found) in text.match_indices(core.text.as_str()) {
                let mut from = at;
                let mut to = at + found.len();
                if core.lead && from > 0 && is_b64_char(bytes[from - 1]) {
                    from -= 1;
                }
                if core.trail {
                    if bytes.get(to).copied().is_some_and(is_b64_char) {
                        to += 1;
                    }
                    let padding = bytes[to..].iter().take(2).take_while(|&&b| b == b'=');
                    to += padding.count();
                }
                spans.push((from, to));
            }
        }
    }
}

/// 글에서 `items`의 값이 든 자리를 모두 [`REDACTED`]로 바꾼다. 없으면 글을 그대로 돌려준다.
pub(crate) fn redact_all<'a>(items: impl IntoIterator<Item = &'a Pattern>, text: &str) -> String {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for item in items {
        item.find_in(text, &mut spans);
    }
    spans.retain(|&(from, to)| text.is_char_boundary(from) && text.is_char_boundary(to));
    if spans.is_empty() {
        return text.to_owned();
    }
    spans.sort_unstable();
    let mut out = String::with_capacity(text.len());
    let mut done = 0;
    let mut covered_to = 0;
    for (from, to) in spans {
        if from >= covered_to {
            out.push_str(&text[done..from]);
            out.push_str(REDACTED);
        }
        covered_to = covered_to.max(to);
        done = covered_to;
    }
    out.push_str(&text[done..]);
    out
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

    use super::*;

    const SECRET: &str = "시험용>>비밀??값~~0001";

    /// 시험용 값 묶음.
    #[derive(Default)]
    struct Patterns {
        items: Vec<Pattern>,
    }

    impl Patterns {
        fn add(&mut self, value: &str) {
            self.items.extend(Pattern::new(value));
        }

        fn redact(&self, text: &str) -> String {
            redact_all(&self.items, text)
        }
    }

    fn patterns(values: &[&str]) -> Patterns {
        let mut patterns = Patterns::default();
        for value in values {
            patterns.add(value);
        }
        patterns
    }

    #[test]
    fn 더_긴_글의_일부로_base64에_든_값도_가린다() {
        let p = patterns(&[SECRET]);
        for prefix in ["", "a", "ab", "abc", "아이디-0001:"] {
            for suffix in ["", "z", "zz", "zzz"] {
                let joined = format!("{prefix}{SECRET}{suffix}");
                for encoded in [STANDARD.encode(&joined), URL_SAFE_NO_PAD.encode(&joined)] {
                    let out = p.redact(&format!("Authorization: Basic {encoded} 끝"));
                    assert!(
                        out.contains(REDACTED),
                        "가리지 못했다: {prefix:?} {suffix:?}"
                    );
                    assert!(out.starts_with("Authorization: Basic "));
                    assert!(out.ends_with(" 끝"));
                    // 값만으로 정해지는 글자는 하나도 남지 않는다.
                    for core in &p.items[0].b64 {
                        assert!(!out.contains(core.text.as_str()));
                    }
                }
            }
        }
    }

    #[test]
    fn 통째로_base64에_든_값은_채움까지_가린다() {
        let p = patterns(&["abcd1234"]);
        let encoded = STANDARD.encode("abcd1234");
        assert!(encoded.ends_with('='));
        assert_eq!(p.redact(&format!("<{encoded}>")), format!("<{REDACTED}>"));
    }

    #[test]
    fn 일부만_url_인코딩된_값도_가린다() {
        let p = patterns(&[
            "시험용/비밀=값&0001",
            "공백 든 값 0001",
            "퍼센트%25값%-0001",
        ]);
        for text in [
            "%EC%8B%9C험용%2f비밀=값%260001",
            "공백+든%20값+0001",
            "퍼센트%25값%-0001",
            "퍼센트%2525값%25-0001",
        ] {
            assert_eq!(p.redact(&format!("a={text}&b")), format!("a={REDACTED}&b"));
        }
    }

    #[test]
    fn 겹치거나_붙은_값도_남기지_않는다() {
        let p = patterns(&["abcd1234", "1234efgh", "abcd1234-더-긴-값"]);
        assert_eq!(p.redact("abcd1234efgh"), REDACTED);
        assert_eq!(p.redact("x abcd1234-더-긴-값 y"), format!("x {REDACTED} y"));
        assert_eq!(
            p.redact("abcd12341234efgh"),
            format!("{REDACTED}{REDACTED}")
        );
    }

    #[test]
    fn 값이_없는_글과_짧은_값은_건드리지_않는다() {
        let mut p = patterns(&["abcd1234"]);
        p.add("짧음");
        p.add("");
        assert_eq!(p.items.len(), 1);
        let text = "짧음 abcd123 % %zz %4 YWJj 끝%";
        assert_eq!(p.redact(text), text);
        assert_eq!(Patterns::default().redact(text), text);
    }
}
