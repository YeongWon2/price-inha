//! (결정 30) 지침 승격: 복기의 실수 · 결정 메모의 교훈 · 익절로 끝난 매매의 계기에서 되풀이되는 것을 지침 후보로
//! 만들고, 문턱을 넘은 것을 장부에 올린다. 글은 AI 없이 고정 틀과 기존 글에서 만든다(AI 호출을 늘리지 않는다,
//! 결정 15 · N14).

use pi_store::DecisionMemo;
use rust_decimal::Decimal;

use crate::guideline::{GuidelineBook, GuidelineChange, GuidelineKind, GuidelineRules};
use crate::review::ReviewRecord;

/// 끝난(팔린) 매매 하나: 번호 · 종목 · 계기 · 손익.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedTrade {
    /// 매매 번호.
    pub trade: i64,
    /// 종목 코드.
    pub symbol: String,
    /// 판단 근거의 계기. 없으면 `None`.
    pub trigger: Option<String>,
    /// 손익(판 금액 − 산 금액).
    pub pnl: Decimal,
}

/// 지침 후보.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// 열쇠(같은 후보를 두 번 올리지 않는다).
    pub key: String,
    /// 종류.
    pub kind: GuidelineKind,
    /// 글.
    pub text: String,
    /// 조건.
    pub when: String,
    /// 한 종목에만 맞으면 그 종목.
    pub symbol: Option<String>,
    /// 근거.
    pub basis: Vec<String>,
}

/// 승격의 결과.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Promotion {
    /// 올림 · 고침.
    pub changes: Vec<GuidelineChange>,
    /// 1층(한도 · 금지 · 승인 길)을 바꾸려는 후보: 지침이 되지 못하고 1층 개선안 길로만 간다.
    pub to_rules: Vec<Candidate>,
}

/// 지침 후보를 만든다(문턱 [`GuidelineRules::promote_repeats`] 이상 되풀이된 것만).
///
/// - 복기의 같은 실수 유형이 문턱 이상 → 실수 유형마다 정한 틀의 글(자유 글 실수는 그 글을 담는다).
/// - 정산된 결정 메모의 비슷한 교훈이 문턱 이상 → 가장 최근 교훈의 첫 문장. 그 메모들이 대개 손실이면 피하기,
///   아니면 운용. 모두 같은 종목이면 그 종목에만.
/// - 익절로 끝난 매매의 비슷한 계기가 문턱 이상이고 같은 계기의 손절보다 많으면 → 들어가기.
pub fn guideline_candidates(
    reviews: &[ReviewRecord],
    memos: &[DecisionMemo],
    closed: &[ClosedTrade],
    rules: &GuidelineRules,
) -> Vec<Candidate> {
    let _ = (reviews, memos, closed, rules);
    Vec::new()
}

impl GuidelineBook {
    /// 후보를 올린다. 1층을 건드리는 후보는 올리지 않고 [`Promotion::to_rules`]로 돌려준다. 같은 열쇠의 활성
    /// 지침이 있고 근거가 늘었으면 글 · 조건 · 근거를 고친다(고침). 같은 열쇠의 지침을 내렸으면 다시 올리지 않는다.
    pub fn promote(
        &mut self,
        candidates: Vec<Candidate>,
        now: i64,
        rules: &GuidelineRules,
    ) -> Promotion {
        let _ = (candidates, now, rules);
        Promotion::default()
    }
}
