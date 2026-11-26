//! (기능 L · 결정 30) 팀 지침 잇기: 논의를 열 때 판단 글에 넣을 지침 줄 · 논의가 끝난 뒤 쓰임 · 하루 정리 때 승격 ·
//! 성적 세기 · 회고, 바뀔 때마다 [`super::KIND_GUIDELINE`] 이벤트.
//!
//! 원본 동작 근거: `docs/research/hermes-agent.md` 3.6 · `docs/research/openclaw.md` 3.6 (pi_journal `guideline` 모듈 문서).

use pi_agent::DiscussionOutcome;
use pi_store::TradeId;

use super::Crew;

impl Crew {
    /// 그 매매(종목)의 논의에 넣을 지침 줄. 넣은 지침은 그 매매의 쓰임으로 장부에 남긴다.
    pub(super) fn guideline_lines(&self, trade: TradeId) -> Vec<String> {
        let _ = trade;
        Vec::new()
    }

    /// 논의가 끝났다: 샀는지 장부에 남긴다.
    pub(super) fn note_guideline_outcome(&self, trade: TradeId, outcome: &DiscussionOutcome) {
        let _ = (trade, outcome);
    }

    /// 하루 정리 때: 끝난 매매의 쓰임을 세고, 되풀이된 것을 올리고, 그날 처음이면 회고한다.
    pub(super) async fn learn_guidelines(&self) {}
}
