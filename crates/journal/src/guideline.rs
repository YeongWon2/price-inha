//! (결정 30) 2층 팀 지침: 지침 한 줄 · 장부 · 문턱 · 판단 글에 넣을 것 고르기.
//!
//! 원본 동작 근거: Hermes `MEMORY.md`(글자 상한 안에서 프롬프트의 정해진 자리에 넣는다, `docs/research/hermes-agent.md`
//! 3.6) · OpenClaw `MEMORY.md`("오래 갈 사실 · 결정 · 요약, 원문 기록 아님", 예산을 넘으면 주입본만 자른다, "기억은
//! 정책을 강제하지 않는다" — 행동 기억은 조건 · 피할 행동을 같이, `docs/research/openclaw.md` 3.6). Hermes 스킬 curator의
//! 승격 · 내림은 결정 30으로 들인다(우리 것은 스킬 파일이 아니라 지침 목록이다).
//!
//! 지침은 참고다: 주문은 지금처럼 1층(pi-rules · 절대 상한 · 금지 종목 · 업종표 · 주문 단위 · AI 호출 상한) 검사를 그대로
//! 거친다. 한도 · 금지 · 승인 길을 바꾸라는 내용은 지침이 되지 못한다([`touches_layer_one`]).
//!
//! 순수 계산만 한다. 파일 읽고 쓰기 · 이벤트 · 화면은 server가 한다. 시각 · 날 수는 인자로 받는다.

use serde::{Deserialize, Serialize};

/// 같은 것이 몇 번 되풀이되면 지침으로 올리는가(실수 유형 · 비슷한 교훈 · 익절 계기).
pub const GUIDELINE_PROMOTE_REPEATS: usize = 3;
/// 내림을 따지기 시작하는 쓰임 수.
pub const GUIDELINE_RETIRE_MIN_USES: u32 = 5;
/// 쓰임이 충분한데 익절 비율이 이 백분율 아래면 내린다.
pub const GUIDELINE_RETIRE_WIN_PCT: u32 = 40;
/// 이만큼의 거래일(하루 정리 수) 동안 판단 글에 들지 않았으면 내린다.
pub const GUIDELINE_STALE_DAYS: u32 = 30;
/// 막힘 풀기: 최근 몇 거래일과 그 앞 같은 길이를 견주는가.
pub const GUIDELINE_UNBLOCK_DAYS: usize = 5;
/// 막힘 풀기: 앞 구간의 사자 판단이 이 수 이상일 때만 견준다(너무 적으면 줄었다고 보지 않는다).
pub const GUIDELINE_UNBLOCK_MIN_EARLIER: u32 = 4;
/// 막힘 풀기: 최근 구간의 사자 판단이 앞 구간의 이 백분율 아래면 "크게 줄었다"로 본다.
pub const GUIDELINE_UNBLOCK_KEEP_PCT: u32 = 50;
/// 판단 글의 "팀 지침" 블록에 넣는 지침의 가장 많은 수.
pub const GUIDELINE_BLOCK_MAX_ITEMS: usize = 7;
/// 판단 글의 "팀 지침" 블록에 넣는 지침 줄들의 글자 상한(줄 글자 수의 합).
pub const GUIDELINE_BLOCK_MAX_CHARS: usize = 1_200;
/// 지침 글 하나의 글자 상한(넘으면 자른다).
pub const GUIDELINE_TEXT_MAX_CHARS: usize = 160;
/// 교훈 · 계기를 "같은 것"으로 묶는 비슷함(백분율, 글자 두 개 묶음의 겹침).
pub const GUIDELINE_SIMILAR_PCT: u32 = 60;
/// 지침 둘을 "글이 거의 같다"로 보고 합치는 비슷함(백분율).
pub const GUIDELINE_MERGE_PCT: u32 = 80;
/// 장부에 남기는 하루 사자 판단 수의 가장 많은 날 수.
pub const GUIDELINE_TALLY_KEEP: usize = 60;

/// 지침의 문턱을 한곳에 모은 것. 기본은 [`GuidelineRules::DEFAULT`](위의 이름 붙은 상수들).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuidelineRules {
    /// [`GUIDELINE_PROMOTE_REPEATS`]
    pub promote_repeats: usize,
    /// [`GUIDELINE_RETIRE_MIN_USES`]
    pub retire_min_uses: u32,
    /// [`GUIDELINE_RETIRE_WIN_PCT`]
    pub retire_win_pct: u32,
    /// [`GUIDELINE_STALE_DAYS`]
    pub stale_days: u32,
    /// [`GUIDELINE_UNBLOCK_DAYS`]
    pub unblock_days: usize,
    /// [`GUIDELINE_UNBLOCK_MIN_EARLIER`]
    pub unblock_min_earlier: u32,
    /// [`GUIDELINE_UNBLOCK_KEEP_PCT`]
    pub unblock_keep_pct: u32,
    /// [`GUIDELINE_BLOCK_MAX_ITEMS`]
    pub max_items: usize,
    /// [`GUIDELINE_BLOCK_MAX_CHARS`]
    pub max_chars: usize,
    /// [`GUIDELINE_TEXT_MAX_CHARS`]
    pub text_max_chars: usize,
    /// [`GUIDELINE_SIMILAR_PCT`]
    pub similar_pct: u32,
    /// [`GUIDELINE_MERGE_PCT`]
    pub merge_pct: u32,
}

impl GuidelineRules {
    /// 기본 문턱(조정자 해석 · 결정 30).
    pub const DEFAULT: GuidelineRules = GuidelineRules {
        promote_repeats: GUIDELINE_PROMOTE_REPEATS,
        retire_min_uses: GUIDELINE_RETIRE_MIN_USES,
        retire_win_pct: GUIDELINE_RETIRE_WIN_PCT,
        stale_days: GUIDELINE_STALE_DAYS,
        unblock_days: GUIDELINE_UNBLOCK_DAYS,
        unblock_min_earlier: GUIDELINE_UNBLOCK_MIN_EARLIER,
        unblock_keep_pct: GUIDELINE_UNBLOCK_KEEP_PCT,
        max_items: GUIDELINE_BLOCK_MAX_ITEMS,
        max_chars: GUIDELINE_BLOCK_MAX_CHARS,
        text_max_chars: GUIDELINE_TEXT_MAX_CHARS,
        similar_pct: GUIDELINE_SIMILAR_PCT,
        merge_pct: GUIDELINE_MERGE_PCT,
    };
}

impl Default for GuidelineRules {
    fn default() -> GuidelineRules {
        GuidelineRules::DEFAULT
    }
}

/// 지침 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuidelineKind {
    /// 이럴 때는 하지 않는다.
    Avoid,
    /// 이럴 때 들어간다(익절로 끝난 매매들의 공통점).
    Enter,
    /// 들고 있을 때 · 팔 때 이렇게 한다.
    Manage,
}

impl GuidelineKind {
    /// 기계 글자(`avoid` · `enter` · `manage`).
    pub fn word(self) -> &'static str {
        match self {
            GuidelineKind::Avoid => "avoid",
            GuidelineKind::Enter => "enter",
            GuidelineKind::Manage => "manage",
        }
    }

    /// 판단 글에 붙이는 쉬운 말.
    pub fn label(self) -> &'static str {
        match self {
            GuidelineKind::Avoid => "피하기",
            GuidelineKind::Enter => "들어가기",
            GuidelineKind::Manage => "운용",
        }
    }
}

/// 지침 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuidelineState {
    /// 판단 글에 들어갈 수 있다.
    Active,
    /// 내렸다(합침 · 성적 · 오래 안 씀 · 막힘 풀기 · 사람이 끔).
    Retired,
}

/// 지침 하나.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guideline {
    /// 번호(`g1`, `g2`, …).
    pub id: String,
    /// 승격 근거의 열쇠(같은 후보를 두 번 올리지 않는다): `mistake:<실수>` · `lesson:<메모 번호>` · `enter:<매매 번호>`.
    pub key: String,
    /// 종류.
    pub kind: GuidelineKind,
    /// 한두 문장(글자 상한 [`GUIDELINE_TEXT_MAX_CHARS`]).
    pub text: String,
    /// 언제 쓰는가(조건).
    pub when: String,
    /// 한 종목에만 맞는 지침이면 그 종목. 없으면 모든 종목.
    #[serde(default)]
    pub symbol: Option<String>,
    /// 근거(`review:<매매>` · `memo:<메모>` · `trade:<매매>`).
    pub basis: Vec<String>,
    /// 판단 글에 넣은 매매가 끝난 수.
    pub uses: u32,
    /// 그 가운데 익절(손익 > 0).
    pub wins: u32,
    /// 그 가운데 손절(손익 < 0).
    pub losses: u32,
    /// (피하기) 판단 글에 넣었는데 사지 않고 끝난 논의 수 — 막힘 풀기가 가장 많이 막은 것을 고를 때 쓴다.
    #[serde(default)]
    pub blocks: u32,
    /// 만든 시각(유닉스 밀리초).
    pub created_at: i64,
    /// 마지막으로 바뀐 시각.
    pub updated_at: i64,
    /// 마지막으로 판단 글에 든(또는 만든) 날 수([`GuidelineBook::day`] 기준).
    #[serde(default)]
    pub last_used_day: u32,
    /// 상태.
    pub state: GuidelineState,
    /// 내린 까닭(`merged` · `poor_win_rate` · `unused` · `blocking_trades` · `person`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retired_why: Option<String>,
}

impl Guideline {
    /// 활성인가.
    pub fn is_active(&self) -> bool {
        self.state == GuidelineState::Active
    }

    /// 익절 비율(백분율, 내림). 쓰임이 없으면 `None`.
    pub fn win_rate_pct(&self) -> Option<u32> {
        (self.uses > 0).then(|| self.wins.saturating_mul(100) / self.uses)
    }

    /// 판단 글의 한 줄: `[<종류 말>] <글> (언제: <조건>)`.
    pub fn line(&self) -> String {
        format!(
            "[{}] {} (언제: {})",
            self.kind.label(),
            self.text,
            self.when
        )
    }

    /// 고르기의 성적: `(익절 + 1) / (쓰임 + 2)`를 견줄 수 있는 분자 · 분모로.
    fn score(&self) -> (u64, u64) {
        (u64::from(self.wins) + 1, u64::from(self.uses) + 2)
    }

    /// 내린다.
    pub(crate) fn retire(&mut self, why: &str, now: i64) {
        self.state = GuidelineState::Retired;
        self.retired_why = Some(why.to_string());
        self.updated_at = now;
    }
}

/// 판단 글에 넣었으나 아직 끝나지 않은 매매.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingUse {
    /// 매매 번호.
    pub trade: i64,
    /// 넣은 지침 번호들.
    pub ids: Vec<String>,
}

/// 하루 정리 하나의 사자 판단 수.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayTally {
    /// 그날(`YYYY-MM-DD`).
    pub date: String,
    /// 사자 판단 수.
    pub buys: u32,
}

/// 무엇이 바뀌었나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuidelineAction {
    /// 새로 올렸다.
    Promoted,
    /// 근거가 늘어 글 · 근거를 고쳤다.
    Revised,
    /// 내렸다.
    Retired,
    /// 다른 지침에 합쳤다(이 지침은 내림).
    Merged,
}

impl GuidelineAction {
    /// 기계 글자(`promoted` · `revised` · `retired` · `merged`).
    pub fn word(self) -> &'static str {
        match self {
            GuidelineAction::Promoted => "promoted",
            GuidelineAction::Revised => "revised",
            GuidelineAction::Retired => "retired",
            GuidelineAction::Merged => "merged",
        }
    }
}

/// 지침 하나의 바뀜(이벤트 한 건).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidelineChange {
    /// 무엇이.
    pub action: GuidelineAction,
    /// 지침 번호.
    pub id: String,
    /// 까닭(기계 글자): 올림 `<열쇠 머리>`(`mistake` · `lesson` · `enter`), 고침 `new_basis`, 내림은
    /// [`Guideline::retired_why`]와 같다.
    pub why: String,
    /// 합침이면 합쳐진 쪽 번호.
    pub into: Option<String>,
}

/// 사람이 끈 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersonRetire {
    /// 그런 번호가 없다.
    NotFound,
    /// 이미 내린 지침이다(바뀐 것 없음).
    AlreadyRetired,
    /// 내렸다.
    Retired(GuidelineChange),
}

/// 지침 장부: 지침 목록 · 아직 끝나지 않은 쓰임 · 하루 사자 판단 수. server가 파일 하나로 둔다.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuidelineBook {
    /// 지침들(만든 순서).
    #[serde(default)]
    pub guidelines: Vec<Guideline>,
    /// 판단 글에 넣었으나 아직 끝나지 않은 매매.
    #[serde(default)]
    pub pending: Vec<PendingUse>,
    /// 하루 정리마다의 사자 판단 수(오래된 것부터, 많아야 [`GUIDELINE_TALLY_KEEP`]날).
    #[serde(default)]
    pub days: Vec<DayTally>,
    /// 하루 정리를 한 날 수(거래일 수로 쓴다).
    #[serde(default)]
    pub day: u32,
    /// 마지막 하루 정리 이후의 사자 판단 수.
    #[serde(default)]
    pub today_buys: u32,
    /// 마지막 하루 정리 날(`YYYY-MM-DD`).
    #[serde(default)]
    pub last_close: Option<String>,
    /// 다음 지침 번호.
    #[serde(default)]
    pub next_id: u32,
    /// 1층 길로 보낸 후보의 열쇠(같은 후보를 날마다 다시 보내지 않는다).
    #[serde(default)]
    pub sent_to_rules: Vec<String>,
}

impl GuidelineBook {
    /// 판단 글에 넣을 지침: 활성이고 종목이 맞는(종목 없음 또는 같은 종목) 것을 성적 순으로, 개수 · 글자 상한 안에서.
    ///
    /// 성적은 익절 비율을 쓰임이 적을 때 반으로 당긴 값(`(익절 + 1) / (쓰임 + 2)`)이고, 같으면 최근에 바뀐 것 · 번호
    /// 순서다. 줄([`Guideline::line`])을 더해 글자 상한을 넘는 지침은 건너뛰고 다음 것을 본다.
    pub fn select(&self, symbol: &str, rules: &GuidelineRules) -> Vec<&Guideline> {
        let fitting = self
            .ranked()
            .into_iter()
            .filter(|g| g.symbol.as_deref().is_none_or(|only| only == symbol));
        let mut picked = Vec::new();
        let mut used = 0_usize;
        for guideline in fitting {
            if picked.len() >= rules.max_items {
                break;
            }
            let chars = guideline.line().chars().count();
            if used + chars > rules.max_chars {
                continue;
            }
            used += chars;
            picked.push(guideline);
        }
        picked
    }

    /// 활성 지침 전부를 성적 순으로([`GuidelineBook::select`]와 같은 차례, 상한 · 종목 없이).
    pub fn ranked(&self) -> Vec<&Guideline> {
        let mut active: Vec<&Guideline> =
            self.guidelines.iter().filter(|g| g.is_active()).collect();
        active.sort_by(|a, b| {
            let ((an, ad), (bn, bd)) = (a.score(), b.score());
            (bn * ad)
                .cmp(&(an * bd))
                .then(b.updated_at.cmp(&a.updated_at))
                .then(a.id.cmp(&b.id))
        });
        active
    }

    /// 그 매매의 쓰임을 센 것 없이 지운다(팔기 판단처럼 사지도 막지도 않은 논의).
    pub fn forget(&mut self, trade: i64) {
        self.pending.retain(|use_| use_.trade != trade);
    }

    /// 번호로 찾는다(바꿀 수 있게).
    pub(crate) fn find_mut(&mut self, id: &str) -> Option<&mut Guideline> {
        self.guidelines.iter_mut().find(|g| g.id == id)
    }

    /// 새 지침 번호.
    pub(crate) fn fresh_id(&mut self) -> String {
        self.next_id += 1;
        format!("g{}", self.next_id)
    }

    /// 판단 글에 넣었다: 그 매매의 쓰임을 남기고(같은 매매면 바꾼다) 넣은 지침의 `last_used_day`를 오늘로 한다.
    pub fn note_injected(&mut self, trade: i64, ids: &[String]) {
        let day = self.day;
        for id in ids {
            if let Some(guideline) = self.find_mut(id) {
                guideline.last_used_day = day;
            }
        }
        self.pending.retain(|use_| use_.trade != trade);
        if !ids.is_empty() {
            self.pending.push(PendingUse {
                trade,
                ids: ids.to_vec(),
            });
        }
    }

    /// 논의가 끝났다. 샀으면 오늘의 사자 판단 수를 하나 늘리고 쓰임을 남겨 둔다(끝날 때 센다). 사지 않았으면 그
    /// 매매의 쓰임을 지우고, 넣었던 활성 피하기 지침마다 `blocks`를 하나 늘린다.
    pub fn note_decision(&mut self, trade: i64, bought: bool) {
        if bought {
            self.today_buys = self.today_buys.saturating_add(1);
            return;
        }
        let Some(at) = self.pending.iter().position(|use_| use_.trade == trade) else {
            return;
        };
        let dropped = self.pending.remove(at);
        for id in &dropped.ids {
            if let Some(guideline) = self.find_mut(id)
                && guideline.is_active()
                && guideline.kind == GuidelineKind::Avoid
            {
                guideline.blocks = guideline.blocks.saturating_add(1);
            }
        }
    }

    /// 끝난 매매(번호 · 손익)로 쓰임을 센다: 넣은 지침마다 쓰임 +1, 손익 > 0이면 익절, < 0이면 손절. 센 매매는
    /// 쓰임 목록에서 지우고 그 번호들을 돌려준다.
    pub fn settle(&mut self, closed: &[(i64, rust_decimal::Decimal)], now: i64) -> Vec<i64> {
        let mut done = Vec::new();
        let pending = std::mem::take(&mut self.pending);
        for use_ in pending {
            let Some((_, pnl)) = closed.iter().find(|(trade, _)| *trade == use_.trade) else {
                self.pending.push(use_);
                continue;
            };
            for id in &use_.ids {
                if let Some(guideline) = self.find_mut(id) {
                    guideline.uses = guideline.uses.saturating_add(1);
                    if pnl.is_sign_positive() && !pnl.is_zero() {
                        guideline.wins = guideline.wins.saturating_add(1);
                    } else if pnl.is_sign_negative() && !pnl.is_zero() {
                        guideline.losses = guideline.losses.saturating_add(1);
                    }
                    guideline.updated_at = now;
                }
            }
            done.push(use_.trade);
        }
        done
    }

    /// 하루 정리: 날짜가 마지막 정리와 다르면 날 수를 하나 늘리고 그날의 사자 판단 수를 남긴 뒤 참. 같은 날이면 거짓.
    pub fn close_day(&mut self, date: &str) -> bool {
        if self.last_close.as_deref() == Some(date) {
            return false;
        }
        self.day = self.day.saturating_add(1);
        self.days.push(DayTally {
            date: date.to_string(),
            buys: self.today_buys,
        });
        let over = self.days.len().saturating_sub(GUIDELINE_TALLY_KEEP);
        self.days.drain(..over);
        self.today_buys = 0;
        self.last_close = Some(date.to_string());
        true
    }

    /// 사람이 끈다(까닭 `person`).
    pub fn retire_by_person(&mut self, id: &str, now: i64) -> PersonRetire {
        let Some(guideline) = self.find_mut(id) else {
            return PersonRetire::NotFound;
        };
        if !guideline.is_active() {
            return PersonRetire::AlreadyRetired;
        }
        guideline.retire(WHY_PERSON, now);
        PersonRetire::Retired(GuidelineChange {
            action: GuidelineAction::Retired,
            id: id.to_string(),
            why: WHY_PERSON.to_string(),
            into: None,
        })
    }
}

/// 사람이 끈 까닭.
const WHY_PERSON: &str = "person";
