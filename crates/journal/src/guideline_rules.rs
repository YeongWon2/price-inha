//! (백로그 T · 결정 30) 1층으로 가는 지침 후보를 지금의 규칙 개선안 길로.
//!
//! 한도 · 금지 · 승인 길을 바꾸려는 되풀이([`crate::touches_layer_one`])는 지침이 되지 못한다. 그 가운데 **어느
//! RiskParams 값을 어떻게 바꾸자는 것인지 글에서 확실히 읽히는 것만** 지금의 개선안(`Improvement` — 바꾸기 전 · 후
//! 값 · 근거 매매 · 위험 표시)으로 낸다. 승인 · 냉각 · 상한은 지금 길 그대로다(사람 승인이 있어야 반영된다).
//!
//! 확실히 읽힌다(조정자 해석):
//!
//! - 칸: 글에 칸 이름([`FIELD_WORDS`])이 **하나의 칸**만 나온다(둘 이상 · 없음이면 읽지 않는다).
//! - 값: 숫자 뒤 `%`(`퍼센트`)가 하나뿐이면 그 값이 목표다. 여럿이면 읽지 않는다. 숫자가 없으면
//!   늘림 · 줄임 낱말([`UP_WORDS`] · [`DOWN_WORDS`]) 한쪽만 있을 때 지금 값의 [`GUIDELINE_RULE_STEP_PCT`]%만큼
//!   (소수 한 자리 반올림, 적어도 0.1) 움직인다.
//! - 목표와 늘림 · 줄임 낱말이 함께 있으면 쪽이 맞아야 한다. 지금 값과 같으면 바뀜이 없다.

use pi_store::{DecisionMemo, NewProposal, PROPOSAL_MIN_EVIDENCE, Proposal, TradeId, UnixMillis};
use pi_template::{Change, PctRange, RiskParams, adjust};
use rust_decimal::{Decimal, RoundingStrategy};

use crate::guideline_learn::Candidate;
use crate::{Error, Improvement, Journal, Mistake, increases_risk};

/// 값 없이 늘림 · 줄임만 적힌 글에서 움직이는 몫(지금 값의 백분율).
pub const GUIDELINE_RULE_STEP_PCT: u32 = 10;

/// 글에서 읽는 규칙 칸.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    RiskPerTrade,
    DailyLoss,
    MaxDrawdown,
    SymbolWeight,
    ConvictionWeight,
    SectorWeight,
    MinCash,
    AutoOrder,
    Stop,
}

/// 칸 이름(공백을 뺀 글에서 찾는다).
const FIELD_WORDS: [(&str, Field); 16] = [
    ("1회위험", Field::RiskPerTrade),
    ("매매당위험", Field::RiskPerTrade),
    ("일일손실", Field::DailyLoss),
    ("하루손실", Field::DailyLoss),
    ("최대낙폭", Field::MaxDrawdown),
    ("낙폭", Field::MaxDrawdown),
    ("확신종목", Field::ConvictionWeight),
    ("종목당비중", Field::SymbolWeight),
    ("종목비중", Field::SymbolWeight),
    ("업종당비중", Field::SectorWeight),
    ("업종비중", Field::SectorWeight),
    ("현금비중", Field::MinCash),
    ("최소현금", Field::MinCash),
    ("자동주문", Field::AutoOrder),
    ("손절폭", Field::Stop),
    ("손절", Field::Stop),
];

/// 늘리자는 낱말.
const UP_WORDS: [&str; 15] = [
    "넓히", "넓혀", "넓힌", "늘리", "늘려", "늘린", "올리", "올려", "올린", "키우", "키워", "키운",
    "높이", "높여", "높인",
];

/// 줄이자는 낱말.
const DOWN_WORDS: [&str; 12] = [
    "좁히", "좁혀", "좁힌", "줄이", "줄여", "줄인", "낮추", "낮춰", "낮춘", "내리", "내려", "내린",
];

/// 바꿀 쪽.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    Up,
    Down,
}

fn squeeze(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 글에 나온 칸 하나. 없거나 둘 이상이면 없음(확신 종목 비중은 종목 비중을 덮는다).
fn field_of(text: &str) -> Option<Field> {
    let mut found: Vec<Field> = Vec::new();
    for (word, field) in FIELD_WORDS {
        if text.contains(word) && !found.contains(&field) {
            found.push(field);
        }
    }
    if found.contains(&Field::ConvictionWeight) {
        found.retain(|f| *f != Field::SymbolWeight);
    }
    match found.as_slice() {
        [one] => Some(*one),
        _ => None,
    }
}

/// 글의 바꿀 쪽. 두 쪽이 다 있거나 없으면 없음.
fn way_of(text: &str) -> Option<Way> {
    let up = UP_WORDS.iter().any(|w| text.contains(w));
    let down = DOWN_WORDS.iter().any(|w| text.contains(w));
    match (up, down) {
        (true, false) => Some(Way::Up),
        (false, true) => Some(Way::Down),
        _ => None,
    }
}

/// 숫자 뒤 `%` · `％` · `퍼센트`인 값들.
fn percents(text: &str) -> Vec<Decimal> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if !chars[at].is_ascii_digit() {
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && (chars[at].is_ascii_digit() || chars[at] == '.') {
            at += 1;
        }
        let number: String = chars[start..at].iter().collect();
        let rest: String = chars[at..].iter().collect();
        let unit = ["%", "％", "퍼센트"].iter().any(|u| rest.starts_with(u));
        if let (true, Ok(value)) = (unit, number.parse::<Decimal>()) {
            out.push(value);
        }
    }
    out
}

/// 글의 목표 값: 백분율 값이 하나뿐이면 그 값, 없으면 없음. 여럿이면(무엇이 목표인지 확실하지 않다) 읽지 않는다.
fn target_of(text: &str) -> Result<Option<Decimal>, ()> {
    match percents(text).as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(*value)),
        _ => Err(()),
    }
}

/// 지금 값의 [`GUIDELINE_RULE_STEP_PCT`]%(소수 한 자리 반올림, 적어도 0.1).
fn step_of(current: Decimal) -> Decimal {
    let step = (current * Decimal::from(GUIDELINE_RULE_STEP_PCT) / Decimal::ONE_HUNDRED)
        .round_dp_with_strategy(1, RoundingStrategy::MidpointAwayFromZero);
    step.max(Decimal::new(1, 1))
}

/// 지금 값 `(아래, 위)`에서 새 값 `(아래, 위)`. 쪽이 어긋나거나 바뀜이 없거나 0 이하가 되면 없음.
fn moved(
    (low, high): (Decimal, Decimal),
    way: Option<Way>,
    target: Option<Decimal>,
) -> Option<(Decimal, Decimal)> {
    let next = match (target, way) {
        (Some(value), Some(Way::Up)) if value <= low => return None,
        (Some(value), Some(Way::Down)) if value >= high => return None,
        (Some(value), _) => (value, value),
        (None, Some(way)) => {
            let step = step_of(high);
            match way {
                Way::Up => (low + step, high + step),
                Way::Down => (low - step, high - step),
            }
        }
        (None, None) => return None,
    };
    (next != (low, high) && next.0 > Decimal::ZERO).then_some(next)
}

/// 1층 후보 글에서 RiskParams 칸 하나의 바뀜을 읽는다. 확실히 읽히지 않으면 없음(모듈 문서의 규칙).
pub fn rule_change_of(text: &str, current: &RiskParams) -> Option<Change> {
    let text = squeeze(text);
    let field = field_of(&text)?;
    let way = way_of(&text);
    let target = target_of(&text).ok()?;
    let one = |value: Decimal| moved((value, value), way, target).map(|(_, v)| v);
    Some(match field {
        Field::RiskPerTrade => Change::RiskPerTradePct(one(current.risk_per_trade_pct)?),
        Field::DailyLoss => Change::DailyLossLimitPct(one(current.daily_loss_limit_pct)?),
        Field::MaxDrawdown => Change::MaxDrawdownPct(one(current.max_drawdown_pct)?),
        Field::SymbolWeight => Change::SymbolWeightPct(one(current.symbol_weight_pct)?),
        Field::SectorWeight => Change::SectorWeightPct(one(current.sector_weight_pct)?),
        Field::MinCash => Change::MinCashPct(one(current.min_cash_pct)?),
        Field::AutoOrder => Change::AutoOrderLimitPct(one(current.auto_order_limit_pct)?),
        Field::ConvictionWeight => {
            let value = match current.conviction_weight_pct {
                Some(now) => one(now)?,
                None => target?,
            };
            Change::ConvictionWeightPct(Some(value))
        }
        Field::Stop => {
            let now = current.stop.fixed_loss_pct?;
            let (low, high) = moved((now.low, now.high), way, target)?;
            let mut stop = current.stop.clone();
            stop.fixed_loss_pct = Some(PctRange { low, high });
            Change::Stop(stop)
        }
    })
}

/// 후보의 근거 매매: `review:<매매>` · `trade:<매매>`는 그 매매, `memo:<메모>`는 그 메모의 매매(없는 메모는 뺀다).
/// 같은 매매는 한 번만, 처음 나온 차례.
pub fn candidate_trades(candidate: &Candidate, memos: &[DecisionMemo]) -> Vec<TradeId> {
    let mut out: Vec<TradeId> = Vec::new();
    for basis in &candidate.basis {
        let found = match basis.split_once(':') {
            Some(("review" | "trade", id)) => id.parse::<i64>().ok().map(TradeId),
            Some(("memo", id)) => id
                .parse::<i64>()
                .ok()
                .and_then(|id| memos.iter().find(|m| m.id.0 == id))
                .map(|m| m.trade),
            _ => None,
        };
        if let Some(trade) = found
            && !out.contains(&trade)
        {
            out.push(trade);
        }
    }
    out
}

/// 개선안의 까닭: 실수 후보(`mistake:<실수>`)면 그 실수, 아니면 후보 글을 자유 글 실수로.
fn reason_of(candidate: &Candidate) -> Mistake {
    match candidate.key.strip_prefix("mistake:") {
        Some("chased_entry") => Mistake::ChasedEntry,
        Some("late_stop") => Mistake::LateStop,
        Some("oversized_position") => Mistake::OversizedPosition,
        Some("no_rationale") => Mistake::NoRationale,
        Some("panic_exit") => Mistake::PanicExit,
        Some(text) if !text.trim().is_empty() => Mistake::Other(text.trim().to_string()),
        _ => Mistake::Other(candidate.text.trim().to_string()),
    }
}

impl Journal {
    /// 1층 후보에서 개선안을 낸다(제안됨). 글이 확실히 읽히지 않거나([`rule_change_of`]) 근거 매매가
    /// [`PROPOSAL_MIN_EVIDENCE`]건이 안 되면 `Ok(None)`이고 아무것도 만들지 않는다. 바뀐 값이 절대 상한 · 경계를
    /// 어기면 [`Error::OverCap`]이고 만들지 않는다. 위험 표시는 지금 개선안 길과 같은 판정이다.
    pub fn propose_from_candidate(
        &self,
        candidate: &Candidate,
        memos: &[DecisionMemo],
        current: &RiskParams,
        at: UnixMillis,
    ) -> Result<Option<Proposal>, Error> {
        let Some(change) = rule_change_of(&candidate.text, current) else {
            return Ok(None);
        };
        let evidence = candidate_trades(candidate, memos);
        if evidence.len() < PROPOSAL_MIN_EVIDENCE {
            return Ok(None);
        }
        let after = adjust(current, &[change])?;
        if &after == current {
            return Ok(None);
        }
        let risky = increases_risk(current, &after);
        let content = serde_json::to_value(Improvement {
            mistake: reason_of(candidate),
            before: current.clone(),
            after,
        })
        .map_err(|err| Error::BadProposal(err.to_string()))?;
        let proposal = self.store.create_proposal(NewProposal {
            content,
            evidence,
            increases_risk: risky,
            at,
        })?;
        Ok(Some(proposal))
    }
}
