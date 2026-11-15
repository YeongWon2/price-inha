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
    let _ = (text, tokens, overlap);
    todo!()
}
