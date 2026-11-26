//! pi-journal: `docs/dev/modules/journal.md` 를 따른다. 복기 · 규칙 개선안 · 하루 정리(A4, T12 · T15).
//!
//! 순수 계산과 저장소 호출만 한다. 시각은 모두 인자로 받는다. AI 호출 · 알림 · 화면은 하지 않는다.

mod basis;
mod day;
pub mod error;
// (기능 L · 결정 30) 2층 팀 지침.
mod guideline;
mod guideline_learn;
mod guideline_review;
mod guideline_text;
mod improve;
mod memo;
mod review;

pub use basis::{missing_fields, plain_summary};
pub use day::{Day, DayFacts, DaySummary, TradeLine};
pub use error::Error;
pub use guideline::{
    DayTally, GUIDELINE_BLOCK_MAX_CHARS, GUIDELINE_BLOCK_MAX_ITEMS, GUIDELINE_MERGE_PCT,
    GUIDELINE_PROMOTE_REPEATS, GUIDELINE_RETIRE_MIN_USES, GUIDELINE_RETIRE_WIN_PCT,
    GUIDELINE_SIMILAR_PCT, GUIDELINE_STALE_DAYS, GUIDELINE_TALLY_KEEP, GUIDELINE_TEXT_MAX_CHARS,
    GUIDELINE_UNBLOCK_DAYS, GUIDELINE_UNBLOCK_KEEP_PCT, GUIDELINE_UNBLOCK_MIN_EARLIER, Guideline,
    GuidelineAction, GuidelineBook, GuidelineChange, GuidelineKind, GuidelineRules, GuidelineState,
    PendingUse, PersonRetire,
};
pub use guideline_learn::{Candidate, ClosedTrade, Promotion, guideline_candidates};
pub use guideline_text::{similarity_pct, touches_layer_one};
pub use improve::{Improvement, RuleOutcome, increases_risk};
pub use memo::{
    ClosePrices, LessonInput, LessonWriter, MemoBrief, OTHER_SYMBOL_LESSONS, Redactor,
    SAME_SYMBOL_MEMOS, SETTLE_AFTER_TRADING_DAYS, SettlePorts, TradingCalendar,
    UNSETTLEABLE_AFTER_TRADING_DAYS,
};
pub use review::{Mistake, ReviewRecord};

use std::collections::HashSet;

use pi_store::{
    Amount, ApprovalUse, NewProposal, PROPOSAL_MIN_EVIDENCE, Proposal, ProposalId, ProposalState,
    Store, Trade, TradeId, TradeState, UnixMillis,
};
use pi_template::{Change, RiskParams, adjust};
use serde::{Deserialize, Serialize};

/// 복기 기록을 저장소 이벤트로 남길 때의 종류 이름.
const REVIEW_KIND: &str = "journal.review";
/// 이벤트를 한 번에 읽는 개수.
const EVENT_PAGE: usize = 500;

/// 저장하는 복기 기록의 모양.
#[derive(Serialize, Deserialize)]
struct ReviewPayload {
    trade: i64,
    mistakes: Vec<Mistake>,
    note: String,
    at: i64,
}

impl From<&ReviewRecord> for ReviewPayload {
    fn from(record: &ReviewRecord) -> ReviewPayload {
        ReviewPayload {
            trade: record.trade.0,
            mistakes: record.mistakes.clone(),
            note: record.note.clone(),
            at: record.at.0,
        }
    }
}

impl From<ReviewPayload> for ReviewRecord {
    fn from(payload: ReviewPayload) -> ReviewRecord {
        ReviewRecord {
            trade: TradeId(payload.trade),
            mistakes: payload.mistakes,
            note: payload.note,
            at: UnixMillis(payload.at),
        }
    }
}

/// 자유 글은 앞뒤 공백을 떼고, 빈 글은 거부한다.
fn normalize(mistake: &Mistake) -> Result<Mistake, Error> {
    match mistake {
        Mistake::Other(text) if text.trim().is_empty() => Err(Error::BlankMistake),
        Mistake::Other(text) => Ok(Mistake::Other(text.trim().to_string())),
        other => Ok(other.clone()),
    }
}

/// 복기 · 개선안 · 하루 정리 창구. 저장소 손잡이 하나를 가진다.
pub struct Journal {
    store: Store,
}

impl Journal {
    /// 저장소 위에 연다. 다시 열어도(같은 파일로 새 손잡이) 기록이 이어진다.
    pub fn new(store: Store) -> Journal {
        Journal { store }
    }

    /// 매매 근거 읽기(C6). 없는 매매면 [`Error::Store`]([`pi_store::StoreError::TradeNotFound`]).
    pub fn read_basis(&self, id: TradeId) -> Result<String, Error> {
        Ok(plain_summary(&self.store.get_trade(id)?))
    }

    /// 팔린 매매를 복기됨으로 바꾸고 복기 기록을 남긴다(V19). 둘은 함께 되거나 함께 안 된다.
    ///
    /// - 팔림이 아니면 [`Error::NotSold`]. 없는 매매면 [`Error::Store`]. 이미 복기됨이면 [`Error::Store`].
    /// - 자유 글 실수가 비었거나 공백뿐이면 [`Error::BlankMistake`].
    /// - 실수가 하나도 없어도 된다.
    pub fn review_trade(
        &self,
        id: TradeId,
        mistakes: Vec<Mistake>,
        note: &str,
        at: UnixMillis,
    ) -> Result<ReviewRecord, Error> {
        let trade = self.store.get_trade(id)?;
        let mistakes = mistakes
            .iter()
            .map(normalize)
            .collect::<Result<Vec<_>, Error>>()?;
        if !matches!(trade.state, TradeState::Sold | TradeState::Reviewed) {
            return Err(Error::NotSold {
                id,
                state: trade.state,
            });
        }
        // 이미 복기됨이면 여기서 저장소가 전이를 거부해 기록이 늘지 않는다.
        self.store.transition_trade(id, TradeState::Reviewed, at)?;
        let record = ReviewRecord {
            trade: id,
            mistakes,
            note: note.to_string(),
            at,
        };
        let payload = serde_json::to_value(ReviewPayload::from(&record))
            .map_err(|err| Error::BadRecord(err.to_string()))?;
        self.store.append_at(REVIEW_KIND, &payload, at)?;
        Ok(record)
    }

    /// 복기 기록 전체. 남긴 순서다. 다시 열어도 그대로다.
    pub fn reviews(&self) -> Result<Vec<ReviewRecord>, Error> {
        let mut records = Vec::new();
        let mut after = 0;
        loop {
            let events = self.store.read_after(after, EVENT_PAGE)?;
            let Some(last) = events.last() else {
                return Ok(records);
            };
            after = last.seq;
            for event in events.iter().filter(|event| event.kind == REVIEW_KIND) {
                let payload: ReviewPayload = serde_json::from_value(event.payload.clone())
                    .map_err(|err| Error::BadRecord(err.to_string()))?;
                records.push(payload.into());
            }
        }
    }

    /// 아직 복기하지 못한 팔린 매매들(E17). 번호 순서다. 다음 마감 뒤에도 같은 함수로 이어서 복기한다.
    pub fn pending_reviews(&self) -> Result<Vec<Trade>, Error> {
        let mut sold = self.store.trades_in_state(TradeState::Sold)?;
        sold.sort_by_key(|trade| trade.id);
        Ok(sold)
    }

    /// 그 실수가 기록된 서로 다른 매매들. 처음 기록된 순서다.
    pub fn mistake_trades(&self, mistake: &Mistake) -> Result<Vec<TradeId>, Error> {
        let wanted = normalize(mistake).unwrap_or_else(|_| mistake.clone());
        let mut trades = Vec::new();
        for record in self.reviews()? {
            if record.mistakes.contains(&wanted) && !trades.contains(&record.trade) {
                trades.push(record.trade);
            }
        }
        Ok(trades)
    }

    /// 같은 실수가 서로 다른 매매 2건 이상이면 규칙 개선안을 낸다(V20). 2건이 안 되면 `Ok(None)`이고 아무것도 만들지 않는다.
    ///
    /// `current`에 `changes`를 [`pi_template::adjust`]로 적용한 값이 `after`다. 상한 · 경계를 어기면
    /// [`Error::OverCap`]이고 개선안은 만들지 않는다. `increases_risk`는 [`increases_risk`]로 정한다.
    /// 근거 매매는 그 실수가 기록된 매매들이다.
    pub fn propose(
        &self,
        mistake: &Mistake,
        current: &RiskParams,
        changes: &[Change],
        at: UnixMillis,
    ) -> Result<Option<Proposal>, Error> {
        let mistake = normalize(mistake)?;
        let evidence = self.mistake_trades(&mistake)?;
        if evidence.len() < PROPOSAL_MIN_EVIDENCE {
            return Ok(None);
        }
        let after = adjust(current, changes)?;
        let risky = increases_risk(current, &after);
        let content = serde_json::to_value(Improvement {
            mistake,
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

    /// 승인한 개선안을 넘긴다(E35 · E28). 승인이 `None`이면 [`Error::ApprovalRequired`]이고 아무것도 바뀌지 않는다.
    ///
    /// 위험을 키우지 않으면 반영됨이고 `rules`는 `after`. 키우면 냉각중이고 `rules`는 `current` 그대로다.
    /// 반영할 값이 절대 상한을 넘으면 [`Error::OverCap`]이고 개선안도 승인도 그대로다(상한 확인이 먼저다).
    pub fn approve(
        &self,
        id: ProposalId,
        approval: Option<ApprovalUse>,
        current: &RiskParams,
        at: UnixMillis,
    ) -> Result<RuleOutcome, Error> {
        let approval = approval.ok_or(Error::ApprovalRequired)?;
        let improvement = self.checked_improvement(id)?;
        let stored = self.store.get_proposal(id)?;
        // 제안 때가 아니라 지금 규칙 기준으로 다시 판정한다(다른 개선안이 먼저 반영됐을 수 있다).
        let risky_now = increases_risk(current, &improvement.after);
        // 제안 때는 안전했지만 지금은 위험을 키우면 같은 개선안에 위험 표시를 올린 뒤 승인한다(냉각을 거친다).
        // 승인이 안 되면 개선안은 제안됨으로 남고 표시는 그대로다(지금 규칙 기준으로 맞는 표시다).
        if risky_now && !stored.increases_risk && stored.state == ProposalState::Proposed {
            self.store.mark_proposal_risky(id, at)?;
        }
        let proposal = self.store.approve_proposal(id, approval, at)?;
        let rules = match proposal.state {
            ProposalState::Applied => improvement.after,
            _ => current.clone(),
        };
        Ok(RuleOutcome { proposal, rules })
    }

    /// 개선안을 거절한다(V21). 규칙은 `current` 그대로다.
    pub fn reject(
        &self,
        id: ProposalId,
        current: &RiskParams,
        at: UnixMillis,
    ) -> Result<RuleOutcome, Error> {
        let proposal = self.store.reject_proposal(id, at)?;
        Ok(RuleOutcome {
            proposal,
            rules: current.clone(),
        })
    }

    /// 냉각중인 개선안을 다음 거래일 시작에 반영한다(E28). `at`이 `trading_day_start`보다 이르면
    /// [`Error::StillCooling`]이고 그대로 냉각중이다(같은 시각은 된다). 반영할 값도 상한을 다시 확인한다.
    /// 개선안은 `after`로 통째로 바꾸는 것이라 `current`는 쓰지 않는다(위험 판정은 승인 때 지금 규칙과 견주어 끝났다).
    pub fn apply_cooled(
        &self,
        id: ProposalId,
        _current: &RiskParams,
        trading_day_start: UnixMillis,
        at: UnixMillis,
    ) -> Result<RuleOutcome, Error> {
        let improvement = self.checked_improvement(id)?;
        if self.store.get_proposal(id)?.state == ProposalState::Cooling && at < trading_day_start {
            return Err(Error::StillCooling {
                until: trading_day_start,
                at,
            });
        }
        let proposal = self.store.apply_cooled_proposal(id, at)?;
        Ok(RuleOutcome {
            proposal,
            rules: improvement.after,
        })
    }

    /// 하루 정리 내용(V12 · V50).
    ///
    /// 하루 범위가 올바르지 않으면 [`Error::BadDay`], 손익 합이 정수 범위를 넘으면 [`Error::Overflow`].
    pub fn daily_summary(&self, day: Day, facts: &DayFacts) -> Result<DaySummary, Error> {
        if day.end <= day.start {
            return Err(Error::BadDay);
        }
        let in_day = |at: UnixMillis| day.start <= at && at < day.end;
        let mut done = Vec::new();
        // 그날 상태가 바뀐 매매(중간에만 바뀌고 뒤에 또 바뀐 것도).
        let changed: HashSet<TradeId> = self
            .store
            .trade_state_changes_between(day.start, day.end)?
            .into_iter()
            .map(|change| change.trade)
            .collect();
        for state in TradeState::ALL {
            for trade in self.store.trades_in_state(state)? {
                // 뒤에 갱신돼(예: 다음 날 복기) `updated_at`이 바뀌어도 그날 만들거나 판단한 매매는 남긴다.
                let touched = [
                    Some(trade.created_at),
                    trade.decided_at,
                    Some(trade.updated_at),
                ];
                if changed.contains(&trade.id) || touched.into_iter().flatten().any(in_day) {
                    done.push(trade);
                }
            }
        }
        done.sort_by_key(|trade| trade.id);
        let mut pnl: i64 = 0;
        for (_, Amount(value)) in &facts.realized {
            pnl = pnl.checked_add(*value).ok_or(Error::Overflow)?;
        }
        Ok(DaySummary {
            done: done
                .into_iter()
                .map(|trade| TradeLine {
                    id: trade.id,
                    symbol: trade.symbol,
                    state: trade.state,
                })
                .collect(),
            pnl: Amount(pnl),
            reviewed: self
                .reviews()?
                .into_iter()
                .filter(|record| in_day(record.at))
                .collect(),
            unreviewed: self
                .pending_reviews()?
                .into_iter()
                .map(|trade| trade.id)
                .collect(),
            last_backup_at: facts.last_backup_at,
        })
    }

    /// 개선안을 읽고 반영할 값이 절대 상한 안인지 확인한다(상한 확인이 먼저다).
    fn checked_improvement(&self, id: ProposalId) -> Result<Improvement, Error> {
        let improvement = Improvement::from_proposal(&self.store.get_proposal(id)?)?;
        improve::check_caps(&improvement.after)?;
        Ok(improvement)
    }
}
