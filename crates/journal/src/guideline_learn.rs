//! (결정 30) 지침 승격: 복기의 실수 · 결정 메모의 교훈 · 익절로 끝난 매매의 계기에서 되풀이되는 것을 지침 후보로
//! 만들고, 문턱을 넘은 것을 장부에 올린다. 글은 AI 없이 고정 틀과 기존 글에서 만든다(AI 호출을 늘리지 않는다,
//! 결정 15 · N14).

use std::collections::BTreeMap;

use pi_store::{DecisionMemo, DecisionMemoState, TradeSide};
use rust_decimal::Decimal;

use crate::guideline::{
    Guideline, GuidelineAction, GuidelineBook, GuidelineChange, GuidelineKind, GuidelineRules,
    GuidelineState,
};
use crate::guideline_text::{clip, group_similar, similarity_pct, touches_layer_one};
use crate::review::{Mistake, ReviewRecord};

/// 고침의 까닭.
const WHY_NEW_BASIS: &str = "new_basis";
/// 계기 · 조건 글에 넣는 계기의 글자 상한.
const TRIGGER_CLIP: usize = 40;

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
    let mut out = mistake_candidates(reviews, rules);
    out.extend(lesson_candidates(memos, rules));
    out.extend(enter_candidates(closed, rules));
    out
}

/// 실수 유형의 기계 글자(server의 복기 글자와 같다).
fn mistake_key(mistake: &Mistake) -> String {
    match mistake {
        Mistake::ChasedEntry => "chased_entry".into(),
        Mistake::LateStop => "late_stop".into(),
        Mistake::OversizedPosition => "oversized_position".into(),
        Mistake::NoRationale => "no_rationale".into(),
        Mistake::PanicExit => "panic_exit".into(),
        Mistake::Other(text) => text.clone(),
    }
}

/// 실수 유형마다 정한 틀(종류 · 글 · 조건). 자유 글 실수는 그 글을 담는다.
fn mistake_template(mistake: &Mistake) -> (GuidelineKind, String, String) {
    let fixed = |kind, text: &str, when: &str| (kind, text.to_string(), when.to_string());
    match mistake {
        Mistake::ChasedEntry => fixed(
            GuidelineKind::Avoid,
            "계기 직후 이미 크게 오른 종목을 뒤따라 사지 않는다",
            "사기 판단에서 가격이 계기 뒤 이미 크게 움직였을 때",
        ),
        Mistake::LateStop => fixed(
            GuidelineKind::Manage,
            "손절 기준에 닿으면 미루지 않고 판다",
            "보유 중 가격이 손절 기준에 다가갈 때",
        ),
        Mistake::OversizedPosition => fixed(
            GuidelineKind::Avoid,
            "확신이 높아도 한 종목에 너무 많이 싣지 않는다",
            "확신이 높아 많이 사고 싶을 때",
        ),
        Mistake::NoRationale => fixed(
            GuidelineKind::Avoid,
            "가설과 계기가 분명하지 않으면 들어가지 않는다",
            "근거를 한 줄로 말하기 어려울 때",
        ),
        Mistake::PanicExit => fixed(
            GuidelineKind::Manage,
            "가설이 깨지지 않았으면 짧은 흔들림만으로 서둘러 팔지 않는다",
            "보유 중 가격이 짧게 출렁일 때",
        ),
        Mistake::Other(text) => (
            GuidelineKind::Avoid,
            format!("복기에서 되풀이된 실수를 피한다: {text}"),
            "같은 상황이 다시 보일 때".to_string(),
        ),
    }
}

/// 같은 실수 유형이 문턱 이상인 것(실수 글자 차례).
fn mistake_candidates(reviews: &[ReviewRecord], rules: &GuidelineRules) -> Vec<Candidate> {
    let mut by_kind: BTreeMap<String, (Mistake, Vec<i64>)> = BTreeMap::new();
    for review in reviews {
        for mistake in &review.mistakes {
            let entry = by_kind
                .entry(mistake_key(mistake))
                .or_insert_with(|| (mistake.clone(), Vec::new()));
            if !entry.1.contains(&review.trade.0) {
                entry.1.push(review.trade.0);
            }
        }
    }
    by_kind
        .into_iter()
        .filter(|(_, (_, trades))| trades.len() >= rules.promote_repeats)
        .map(|(key, (mistake, mut trades))| {
            trades.sort_unstable();
            let (kind, text, when) = mistake_template(&mistake);
            Candidate {
                key: format!("mistake:{key}"),
                kind,
                text: clip(&text, rules.text_max_chars),
                when,
                symbol: None,
                basis: trades.iter().map(|t| format!("review:{t}")).collect(),
            }
        })
        .collect()
}

/// 첫 문장(마침표 · 물음표 · 느낌표까지).
fn first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    match trimmed.find(['.', '?', '!', '。']) {
        Some(end) => trimmed[..=end].trim().to_string(),
        None => trimmed.to_string(),
    }
}

/// 비슷한 교훈이 문턱 이상인 것.
fn lesson_candidates(memos: &[DecisionMemo], rules: &GuidelineRules) -> Vec<Candidate> {
    let mut settled: Vec<&DecisionMemo> = memos
        .iter()
        .filter(|m| m.state == DecisionMemoState::Settled)
        .filter(|m| m.lesson.as_deref().is_some_and(|l| !l.trim().is_empty()))
        .collect();
    settled.sort_by_key(|m| m.id.0);
    let texts: Vec<&str> = settled
        .iter()
        .map(|m| m.lesson.as_deref().unwrap_or_default())
        .collect();
    group_similar(&texts, rules.similar_pct)
        .into_iter()
        .filter(|group| group.len() >= rules.promote_repeats)
        .filter_map(|group| {
            let members: Vec<&DecisionMemo> = group.iter().map(|&i| settled[i]).collect();
            lesson_candidate(&members, rules)
        })
        .collect()
}

/// 비슷한 교훈 묶음 하나의 후보.
fn lesson_candidate(members: &[&DecisionMemo], rules: &GuidelineRules) -> Option<Candidate> {
    let first = members.first()?;
    let latest = members.last()?;
    let losing = members
        .iter()
        .filter(|m| m.return_bp.unwrap_or(0) < 0)
        .count();
    let buys = members.iter().filter(|m| m.side == TradeSide::Buy).count();
    let same_symbol = members.iter().all(|m| m.symbol == latest.symbol);
    let kind = if losing * 2 > members.len() {
        GuidelineKind::Avoid
    } else {
        GuidelineKind::Manage
    };
    let when = if buys * 2 >= members.len() {
        "사기 판단을 할 때"
    } else {
        "팔기 판단을 할 때"
    };
    Some(Candidate {
        key: format!("lesson:{}", first.id.0),
        kind,
        text: clip(
            &first_sentence(latest.lesson.as_deref().unwrap_or_default()),
            rules.text_max_chars,
        ),
        when: when.to_string(),
        symbol: same_symbol.then(|| latest.symbol.clone()),
        basis: members.iter().map(|m| format!("memo:{}", m.id.0)).collect(),
    })
}

/// 매매의 계기 글(없으면 빈 글).
fn trigger_of(trade: &ClosedTrade) -> &str {
    trade.trigger.as_deref().unwrap_or_default().trim()
}

/// 익절로 끝난 매매의 비슷한 계기가 문턱 이상이고, 같은 계기의 손절보다 많은 것.
fn enter_candidates(closed: &[ClosedTrade], rules: &GuidelineRules) -> Vec<Candidate> {
    let mut with_trigger: Vec<&ClosedTrade> = closed
        .iter()
        .filter(|c| !trigger_of(c).is_empty())
        .collect();
    with_trigger.sort_by_key(|c| c.trade);
    let winners: Vec<&ClosedTrade> = with_trigger
        .iter()
        .copied()
        .filter(|c| c.pnl > Decimal::ZERO)
        .collect();
    let losers: Vec<&ClosedTrade> = with_trigger
        .iter()
        .copied()
        .filter(|c| c.pnl < Decimal::ZERO)
        .collect();
    let texts: Vec<&str> = winners.iter().map(|c| trigger_of(c)).collect();
    group_similar(&texts, rules.similar_pct)
        .into_iter()
        .filter(|group| group.len() >= rules.promote_repeats)
        .filter_map(|group| {
            let members: Vec<&ClosedTrade> = group.iter().map(|&i| winners[i]).collect();
            let lost = losers
                .iter()
                .filter(|c| {
                    members
                        .iter()
                        .any(|m| similarity_pct(trigger_of(m), trigger_of(c)) >= rules.similar_pct)
                })
                .count();
            (members.len() > lost)
                .then(|| enter_candidate(&members, rules))
                .flatten()
        })
        .collect()
}

/// 익절 계기 묶음 하나의 후보.
fn enter_candidate(members: &[&ClosedTrade], rules: &GuidelineRules) -> Option<Candidate> {
    let first = members.first()?;
    let shown = clip(trigger_of(first), TRIGGER_CLIP);
    let text = format!(
        "'{shown}' 계기에서 들어간 매매가 거듭 익절로 끝났다. 같은 계기면 들어가는 쪽을 먼저 본다"
    );
    Some(Candidate {
        key: format!("enter:{}", first.trade),
        kind: GuidelineKind::Enter,
        text: clip(&text, rules.text_max_chars),
        when: format!("계기가 '{shown}'와 비슷할 때"),
        symbol: None,
        basis: members
            .iter()
            .map(|m| format!("trade:{}", m.trade))
            .collect(),
    })
}

impl GuidelineBook {
    /// 후보를 올린다. 1층을 건드리는 후보는 올리지 않고 [`Promotion::to_rules`]로 돌려준다(같은 열쇠는 한 번만). 같은 열쇠의 활성
    /// 지침이 있고 근거가 늘었으면 글 · 조건 · 근거를 고친다(고침). 같은 열쇠의 지침을 내렸으면 다시 올리지 않는다.
    pub fn promote(
        &mut self,
        candidates: Vec<Candidate>,
        now: i64,
        rules: &GuidelineRules,
    ) -> Promotion {
        let mut promotion = Promotion::default();
        for candidate in candidates {
            if touches_layer_one(&candidate.text) || touches_layer_one(&candidate.when) {
                if !self.sent_to_rules.contains(&candidate.key) {
                    self.sent_to_rules.push(candidate.key.clone());
                    promotion.to_rules.push(candidate);
                }
                continue;
            }
            if let Some(change) = self.place(candidate, now, rules) {
                promotion.changes.push(change);
            }
        }
        promotion
    }

    /// 후보 하나를 올리거나 고친다. 바뀐 것이 없으면 없음.
    fn place(
        &mut self,
        candidate: Candidate,
        now: i64,
        rules: &GuidelineRules,
    ) -> Option<GuidelineChange> {
        if let Some(existing) = self.guidelines.iter_mut().find(|g| g.key == candidate.key) {
            return revise(existing, candidate, now, rules);
        }
        let id = self.fresh_id();
        let why = candidate
            .key
            .split(':')
            .next()
            .unwrap_or_default()
            .to_string();
        let day = self.day;
        self.guidelines.push(Guideline {
            id: id.clone(),
            key: candidate.key,
            kind: candidate.kind,
            text: clip(&candidate.text, rules.text_max_chars),
            when: candidate.when,
            symbol: candidate.symbol,
            basis: candidate.basis,
            uses: 0,
            wins: 0,
            losses: 0,
            blocks: 0,
            created_at: now,
            updated_at: now,
            last_used_day: day,
            state: GuidelineState::Active,
            retired_why: None,
        });
        Some(GuidelineChange {
            action: GuidelineAction::Promoted,
            id,
            why,
            into: None,
        })
    }
}

/// 같은 열쇠의 지침을 고친다: 활성이고 근거가 늘었을 때만.
fn revise(
    existing: &mut Guideline,
    candidate: Candidate,
    now: i64,
    rules: &GuidelineRules,
) -> Option<GuidelineChange> {
    let grew = candidate.basis.iter().any(|b| !existing.basis.contains(b));
    if !existing.is_active() || !grew {
        return None;
    }
    for item in candidate.basis {
        if !existing.basis.contains(&item) {
            existing.basis.push(item);
        }
    }
    existing.text = clip(&candidate.text, rules.text_max_chars);
    existing.when = candidate.when;
    existing.updated_at = now;
    Some(GuidelineChange {
        action: GuidelineAction::Revised,
        id: existing.id.clone(),
        why: WHY_NEW_BASIS.to_string(),
        into: None,
    })
}
