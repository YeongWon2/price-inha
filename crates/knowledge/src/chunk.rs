//! 글을 조각으로 나눈다(조각 400/80, 줄 번호 인용: `docs/research/triggers-approval-memory-deep.md` 4.1 · 4.3).

/// 한 조각의 크기(낱말 수). 낱말은 공백으로 끊은 덩어리다.
pub const CHUNK_TOKENS: usize = 400;
/// 이웃 조각이 겹치는 크기(낱말 수).
pub const CHUNK_OVERLAP: usize = 80;
/// 색인에 올리는 글의 앞부분 길이(글자 수). 그 뒤는 색인하지 않는다(시세 · 호가 원자료 방지).
pub const INDEX_PREFIX_CHARS: usize = 8192;

/// 나눈 조각 하나. 줄 번호는 1부터, 양끝 포함이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChunk {
    /// 조각의 글(줄 단위로 이어 붙인 것).
    pub text: String,
    /// 첫 줄 번호.
    pub start_line: u32,
    /// 끝 줄 번호.
    pub end_line: u32,
}

/// 글을 줄 단위로 `tokens` 낱말 안팎의 조각으로 나눈다. 다음 조각은 앞 조각의 끝 `overlap` 낱말만큼
/// 겹쳐 시작한다. 낱말이 없는 글은 빈 목록이다. 글자 수 상한([`INDEX_PREFIX_CHARS`])은 여기서 자르지 않는다.
pub fn split_chunks(text: &str, tokens: usize, overlap: usize) -> Vec<TextChunk> {
    let tokens = tokens.max(1);
    let lines: Vec<(&str, usize)> = text
        .lines()
        .map(|line| (line, line.split_whitespace().count()))
        .collect();
    let mut out = Vec::new();
    let mut start = skip_blank(&lines, 0);
    while start < lines.len() {
        let end = chunk_end(&lines, start, tokens);
        let mut last = end;
        while last > start + 1 && lines[last - 1].1 == 0 {
            last -= 1;
        }
        out.push(TextChunk {
            text: lines[start..last]
                .iter()
                .map(|(line, _)| *line)
                .collect::<Vec<_>>()
                .join("\n"),
            start_line: line_number(start + 1),
            end_line: line_number(last),
        });
        if end >= lines.len() {
            break;
        }
        start = skip_blank(&lines, overlap_start(&lines, start, end, overlap));
    }
    out
}

/// `from`부터 처음 낱말이 있는 줄의 위치(없으면 줄 수).
fn skip_blank(lines: &[(&str, usize)], from: usize) -> usize {
    let mut at = from;
    while at < lines.len() && lines[at].1 == 0 {
        at += 1;
    }
    at
}

/// `start`에서 시작하는 조각이 끝나는 위치(미포함). 첫 줄은 낱말이 넘쳐도 들어간다.
fn chunk_end(lines: &[(&str, usize)], start: usize, tokens: usize) -> usize {
    let mut end = start;
    let mut words = 0;
    while end < lines.len() {
        let w = lines[end].1;
        if end > start && words + w > tokens {
            break;
        }
        words += w;
        end += 1;
    }
    end
}

/// 다음 조각의 시작 위치: 끝에서부터 `overlap` 낱말 이하만큼 거슬러 오른다(적어도 한 줄은 나아간다).
fn overlap_start(lines: &[(&str, usize)], start: usize, end: usize, overlap: usize) -> usize {
    let mut next = end;
    let mut carried = 0;
    while next > start + 1 {
        let w = lines[next - 1].1;
        if carried + w > overlap {
            break;
        }
        carried += w;
        next -= 1;
    }
    next
}

fn line_number(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_lines_are_kept_inside_but_trimmed_at_the_edges() {
        let chunks = split_chunks("\n\na b\n\nc d\n\n", 400, 80);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "a b\n\nc d");
        assert_eq!((chunks[0].start_line, chunks[0].end_line), (3, 5));
    }

    #[test]
    fn an_oversized_line_still_makes_progress() {
        let big = "x ".repeat(10);
        let text = format!("{big}\n{big}\n{big}");
        let chunks = split_chunks(&text, 5, 2);
        assert_eq!(chunks.len(), 3);
    }
}
