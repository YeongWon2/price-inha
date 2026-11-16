//! pi-journal: `docs/dev/modules/journal.md` 를 따른다. 복기 · 규칙 개선안 · 하루 정리(A4, T12 · T15).
//!
//! 순수 계산과 저장소 호출만 한다. 시각은 모두 인자로 받는다. AI 호출 · 알림 · 화면은 하지 않는다.

mod basis;
mod day;
pub mod error;
mod improve;
mod review;

pub use basis::{missing_fields, plain_summary};
pub use day::{Day, DayFacts, DaySummary, TradeLine};
pub use error::Error;
pub use improve::{Improvement, RuleOutcome, increases_risk};
pub use review::{Mistake, ReviewRecord};

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
        let proposal =
            if risky_now && !stored.increases_risk && stored.state == ProposalState::Proposed {
                self.approve_as_risky(&stored, approval, at)?
            } else {
                self.store.approve_proposal(id, approval, at)?
            };
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
        for state in TradeState::ALL {
            for trade in self.store.trades_in_state(state)? {
                if in_day(trade.updated_at) {
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
    /// 제안 때는 안전했지만 지금은 위험을 키우는 개선안: 위험 표시를 단 같은 내용의 개선안으로 바꿔 승인한다
    /// (저장소는 표시를 고칠 수 없다). 승인이 안 되면 새 개선안은 거절됨으로 닫고 원래 개선안은 그대로 둔다.
    fn approve_as_risky(
        &self,
        stored: &Proposal,
        approval: ApprovalUse,
        at: UnixMillis,
    ) -> Result<Proposal, Error> {
        let replacement = self.store.create_proposal(NewProposal {
            content: stored.content.clone(),
            evidence: stored.evidence.clone(),
            increases_risk: true,
            at,
        })?;
        match self.store.approve_proposal(replacement.id, approval, at) {
            Ok(approved) => {
                self.store.reject_proposal(stored.id, at)?;
                Ok(approved)
            }
            Err(err) => {
                self.store.reject_proposal(replacement.id, at)?;
                Err(err.into())
            }
        }
    }

    fn checked_improvement(&self, id: ProposalId) -> Result<Improvement, Error> {
        let improvement = Improvement::from_proposal(&self.store.get_proposal(id)?)?;
        improve::check_caps(&improvement.after)?;
        Ok(improvement)
    }
}
