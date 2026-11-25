//! 긴 글 나누기(원본 OpenClaw 텔레그램 4000자 나누기와 같은 뜻).

/// `text`를 조각마다 `max_chars` 글자(유니코드 글자 기준) 안으로 나눈다.
///
/// - 한도 안이면 조각 하나(글 그대로)다. 빈 글이면 빈 목록이다.
/// - 한도 안의 마지막 줄바꿈에서 자른다. 자른 자리의 줄바꿈 하나는 조각에 넣지 않는다
///   (그래서 줄마다 한도 안인 글은 `조각들.join("\n") == text`다).
/// - 한 줄이 한도보다 길면 그 줄은 한도 글자에서 자른다(글자를 버리지 않는다).
/// - 빈 조각은 만들지 않는다. `max_chars`가 0이면 빈 목록이다.
pub fn split_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    if max_chars == 0 {
        return pieces;
    }
    let mut rest = text;
    loop {
        // 한도 바로 뒤 글자의 바이트 자리. 없으면 남은 글 전부가 한도 안이다.
        let Some((limit, _)) = rest.char_indices().nth(max_chars) else {
            if !rest.is_empty() {
                pieces.push(rest.to_owned());
            }
            return pieces;
        };
        let window = &rest[..limit];
        let (piece, next) = if rest[limit..].starts_with('\n') {
            // 한도 글자 바로 뒤가 줄바꿈이면 한도까지가 한 조각이다.
            (window, &rest[limit + 1..])
        } else if let Some(newline) = window.rfind('\n') {
            (&window[..newline], &rest[newline + 1..])
        } else {
            (window, &rest[limit..])
        };
        if !piece.is_empty() {
            pieces.push(piece.to_owned());
        }
        rest = next;
    }
}
