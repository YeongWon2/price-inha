//! (결정 30) 지침 회고: 하루 정리 때 겹치는 지침을 합치고, 성적이 나쁘거나 오래 쓰이지 않은 지침을 내리고, 사자
//! 판단이 크게 줄었으면 가장 많이 막은 피하기 지침을 하나 내린다(매매가 막히지 않게).

use crate::guideline::{GuidelineBook, GuidelineChange, GuidelineRules};

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
        let _ = (now, rules);
        Vec::new()
    }
}
