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

use pi_store::{ApprovalUse, Proposal, ProposalId, Store, Trade, TradeId, UnixMillis};
use pi_template::{Change, RiskParams};

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
        todo!()
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
        todo!()
    }

    /// 복기 기록 전체. 남긴 순서다. 다시 열어도 그대로다.
    pub fn reviews(&self) -> Result<Vec<ReviewRecord>, Error> {
        todo!()
    }

    /// 아직 복기하지 못한 팔린 매매들(E17). 번호 순서다. 다음 마감 뒤에도 같은 함수로 이어서 복기한다.
    pub fn pending_reviews(&self) -> Result<Vec<Trade>, Error> {
        todo!()
    }

    /// 그 실수가 기록된 서로 다른 매매들. 처음 기록된 순서다.
    pub fn mistake_trades(&self, mistake: &Mistake) -> Result<Vec<TradeId>, Error> {
        todo!()
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
        todo!()
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
        todo!()
    }

    /// 개선안을 거절한다(V21). 규칙은 `current` 그대로다.
    pub fn reject(
        &self,
        id: ProposalId,
        current: &RiskParams,
        at: UnixMillis,
    ) -> Result<RuleOutcome, Error> {
        todo!()
    }

    /// 냉각중인 개선안을 다음 거래일 시작에 반영한다(E28). `at`이 `trading_day_start`보다 이르면
    /// [`Error::StillCooling`]이고 그대로 냉각중이다(같은 시각은 된다). 반영할 값도 상한을 다시 확인한다.
    pub fn apply_cooled(
        &self,
        id: ProposalId,
        current: &RiskParams,
        trading_day_start: UnixMillis,
        at: UnixMillis,
    ) -> Result<RuleOutcome, Error> {
        todo!()
    }

    /// 하루 정리 내용(V12 · V50).
    ///
    /// 하루 범위가 올바르지 않으면 [`Error::BadDay`], 손익 합이 정수 범위를 넘으면 [`Error::Overflow`].
    pub fn daily_summary(&self, day: Day, facts: &DayFacts) -> Result<DaySummary, Error> {
        todo!()
    }
}
