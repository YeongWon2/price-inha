//! 검색 도구가 돌려주는 모양(원본 `memory_search`: `docs/research/triggers-approval-memory-deep.md` 4.1).

use serde::Serialize;

use crate::SemanticStatus;

/// 적중 한 건의 글 길이 상한(글자 수).
pub const SNIPPET_CHARS: usize = 700;

/// 어떤 검색이었나.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// 키워드.
    Keyword,
    /// 조건(종목 · 기간 · 손익).
    Condition,
    /// 뜻(임베딩).
    Semantic,
}

/// 적중 한 건. 에이전트가 읽고 다시 검색한다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolHit {
    /// 저장소의 조각 번호.
    pub chunk_id: i64,
    /// 기록 종류 글자(`trade` · `review` · `mistake` · `market_note`).
    pub kind: String,
    /// 출처 등급 글자(`mine` · `team` · `external`).
    pub source: String,
    /// 기록 키.
    pub record_ref: String,
    /// 첫 줄 번호(1부터).
    pub start_line: u32,
    /// 끝 줄 번호.
    pub end_line: u32,
    /// 점수(백만분의 1 정수). 순위가 없는 적중(두 글자 낱말 · 조건 검색)은 `None`.
    pub score_micros: Option<i64>,
    /// 인용: `<kind>/<record_ref>#L<시작>-L<끝>`, 한 줄이면 `#L<줄>`.
    pub citation: String,
    /// 조각 글의 앞 [`SNIPPET_CHARS`]자 + `"\n\nSource: <citation>"`.
    pub snippet: String,
}

/// 검색 도구의 결과.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchOutput {
    /// 적중(순서가 순위).
    pub results: Vec<ToolHit>,
    /// 검색 방식.
    pub mode: SearchMode,
    /// 모든 낱말이 맞는 적중이 없어 하나라도 맞는 것으로 풀어서 찾았으면 참.
    pub relaxed: bool,
    /// 뜻 검색을 쓸 수 있는지.
    pub semantic: SemanticStatus,
}

impl SearchOutput {
    /// 적중한 **매매 기록**(종류가 `trade`)의 기록 키를 순위 순서로, 겹치지 않게(V22: 담당의 판단 기록에 남길 것).
    pub fn trade_refs(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for hit in self.results.iter().filter(|h| h.kind == "trade") {
            if !seen.contains(&hit.record_ref) {
                seen.push(hit.record_ref.clone());
            }
        }
        seen
    }
}
