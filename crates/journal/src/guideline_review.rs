//! (결정 30) 지침 회고: 하루 정리 때 겹치는 지침을 합치고, 성적이 나쁘거나 오래 쓰이지 않은 지침을 내리고, 사자
//! 판단이 크게 줄었으면 가장 많이 막은 피하기 지침을 하나 내린다(매매가 막히지 않게).

use crate::guideline::{
    GuidelineAction, GuidelineBook, GuidelineChange, GuidelineKind, GuidelineRules,
};
use crate::guideline_text::similarity_pct;

/// 합친 까닭.
const WHY_MERGED: &str = "merged";
/// 성적이 나빠 내린 까닭.
const WHY_POOR: &str = "poor_win_rate";
/// 오래 쓰이지 않아 내린 까닭.
const WHY_UNUSED: &str = "unused";
/// 매매를 막아 내린 까닭.
const WHY_BLOCKING: &str = "blocking_trades";

fn retired(id: &str, why: &str) -> GuidelineChange {
    GuidelineChange {
        action: GuidelineAction::Retired,
        id: id.to_string(),
        why: why.to_string(),
        into: None,
    }
}

impl GuidelineBook {
    /// 회고. 차례: 합침 → 성적 내림 → 오래 안 씀 내림 → 막힘 풀기. 바뀐 것을 그 차례로 돌려준다.
    ///
    /// - 합침: 같은 종류 · 같은 조건 · 같은 종목이고 글의 비슷함이 [`GuidelineRules::merge_pct`] 이상인 활성 지침
    ///   둘은 먼저 만든 쪽에 합친다(쓰임 · 익절 · 손절 · 막음을 더하고 근거를 합친다). 뒤의 것은 내림(`merged`).
    /// - 성적: 쓰임이 [`GuidelineRules::retire_min_uses`] 이상이고 익절 비율이 [`GuidelineRules::retire_win_pct`]
    ///   아래면 내림(`poor_win_rate`).
    /// - 오래 안 씀: 마지막으로 판단 글에 든 뒤 [`GuidelineRules::stale_days`]날 이상이면 내림(`unused`).
    /// - 막힘 풀기: 최근 [`GuidelineRules::unblock_days`]날의 사자 판단 수가 그 앞 같은 길이의
    ///   [`GuidelineRules::unblock_keep_pct`]% 아래이고(앞 구간이 [`GuidelineRules::unblock_min_earlier`] 이상일 때),
    ///   막은 적이 있는 활성 피하기 지침이 있으면 가장 많이 막은 것 하나를 내림(`blocking_trades`).
    pub fn retrospect(&mut self, now: i64, rules: &GuidelineRules) -> Vec<GuidelineChange> {
        let mut changes = self.merge_overlaps(now, rules);
        let day = self.day;
        for guideline in self.guidelines.iter_mut().filter(|g| g.is_active()) {
            let poor = guideline.uses >= rules.retire_min_uses
                && guideline.wins.saturating_mul(100)
                    < rules.retire_win_pct.saturating_mul(guideline.uses);
            let stale = day.saturating_sub(guideline.last_used_day) >= rules.stale_days;
            let why = if poor {
                WHY_POOR
            } else if stale {
                WHY_UNUSED
            } else {
                continue;
            };
            guideline.retire(why, now);
            changes.push(retired(&guideline.id, why));
        }
        changes.extend(self.unblock(now, rules));
        changes
    }

    /// 겹치는 활성 지침을 먼저 만든 쪽에 합친다.
    fn merge_overlaps(&mut self, now: i64, rules: &GuidelineRules) -> Vec<GuidelineChange> {
        let mut changes = Vec::new();
        for later in 0..self.guidelines.len() {
            let target = (0..later).find(|&earlier| {
                let (a, b) = (&self.guidelines[earlier], &self.guidelines[later]);
                a.is_active()
                    && b.is_active()
                    && a.kind == b.kind
                    && a.when == b.when
                    && a.symbol == b.symbol
                    && similarity_pct(&a.text, &b.text) >= rules.merge_pct
            });
            let Some(earlier) = target else {
                continue;
            };
            let gone = self.guidelines[later].clone();
            let keep = &mut self.guidelines[earlier];
            keep.uses = keep.uses.saturating_add(gone.uses);
            keep.wins = keep.wins.saturating_add(gone.wins);
            keep.losses = keep.losses.saturating_add(gone.losses);
            keep.blocks = keep.blocks.saturating_add(gone.blocks);
            keep.last_used_day = keep.last_used_day.max(gone.last_used_day);
            for item in gone.basis {
                if !keep.basis.contains(&item) {
                    keep.basis.push(item);
                }
            }
            keep.updated_at = now;
            let into = keep.id.clone();
            self.guidelines[later].retire(WHY_MERGED, now);
            changes.push(GuidelineChange {
                action: GuidelineAction::Merged,
                id: gone.id,
                why: WHY_MERGED.to_string(),
                into: Some(into),
            });
        }
        changes
    }

    /// 사자 판단이 크게 줄었으면 가장 많이 막은 활성 피하기 지침 하나를 내린다.
    fn unblock(&mut self, now: i64, rules: &GuidelineRules) -> Option<GuidelineChange> {
        let span = rules.unblock_days;
        if span == 0 || self.days.len() < span * 2 {
            return None;
        }
        let tail = &self.days[self.days.len() - span * 2..];
        let earlier: u32 = tail[..span].iter().map(|d| d.buys).sum();
        let recent: u32 = tail[span..].iter().map(|d| d.buys).sum();
        let dropped = earlier >= rules.unblock_min_earlier
            && recent.saturating_mul(100) < earlier.saturating_mul(rules.unblock_keep_pct);
        if !dropped {
            return None;
        }
        let most = self
            .guidelines
            .iter_mut()
            .filter(|g| g.is_active() && g.kind == GuidelineKind::Avoid && g.blocks > 0)
            .reduce(|best, next| {
                if next.blocks > best.blocks {
                    next
                } else {
                    best
                }
            })?;
        most.retire(WHY_BLOCKING, now);
        Some(retired(&most.id, WHY_BLOCKING))
    }
}
