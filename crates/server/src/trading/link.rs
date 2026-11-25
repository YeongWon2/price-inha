//! (W5 3차) 주문 길이 3차 조립(알림 · 하루 정리 · 지식 검색)에 내주는 것.

use chrono::NaiveDate;
use pi_approval::{ApprovalNumber, ApprovalView, Decision, Notice, Subject};
use pi_knowledge::TradeRecord;
use pi_paper::{Broker, Order, OrderGroup, OrderStatus, Side};
use pi_store::{ApprovalStatus, ProposalId, TradeId, TradeState};
use rust_decimal::Decimal;

use super::Trading;
use super::answer::{Answered, rejected};
use super::base::exec_text;
use crate::TradingError;

/// 승인 요청이 생겼을 때 부른다(승인 번호, 사람이 읽는 요약). 오래 붙잡지 않는다.
pub(crate) type ApprovalHook = Box<dyn Fn(ApprovalNumber, String) + Send + Sync>;
/// (10c 14번) 승인 요청에 답이 났을 때(승인 · 거절 · 기한 지남) 부른다(승인 번호). 오래 붙잡지 않는다.
pub(crate) type AnsweredHook = Box<dyn Fn(ApprovalNumber) + Send + Sync>;
/// 하루 마감을 남긴 뒤 부른다(시장 날짜). 오래 붙잡지 않는다.
pub(crate) type CloseHook = Box<dyn Fn(NaiveDate) + Send + Sync>;

/// (10b 0번 라) 자료 이상으로 이상 감지 멈춤이 됐을 때 부른다(묶기는 주문 길이 한다). 오래 붙잡지 않는다.
pub(crate) type DataHaltHook = Box<dyn Fn() + Send + Sync>;

/// (10b 보강 5번) 손절 대기가 막힌 채 오래 이어질 때 부른다(종목, 수량 — 까닭 글은 싣지 않는다). 오래 붙잡지 않는다.
pub(crate) type StuckStopHook = Box<dyn Fn(String, String) + Send + Sync>;

/// (28차 3번) 규칙 개선안의 승인 번호에 승인 카드 길로 답이 왔을 때 부른다(개선안 번호, 승인 번호, 보인 지문,
/// 답). 개선안 답하기([`crate::PROPOSAL_ANSWER_PATH`])와 같은 함수를 타고, 결과를 승인 카드 길의 말로 돌려준다.
pub(crate) type ProposalHook = Box<
    dyn Fn(ProposalId, &ApprovalNumber, &str, Decision) -> Result<Answered, TradingError>
        + Send
        + Sync,
>;

/// (28차 3번) 개선안에 답할 팀이 없다(팀을 열지 않았다 · 이미 닫혔다): 승인 번호를 건드리지 않고 거절한다.
pub(crate) const PROPOSAL_UNAVAILABLE: &str = "unavailable";

/// 주문 길이 알리는 자리.
pub(crate) struct Hooks {
    pub(crate) approval: ApprovalHook,
    pub(crate) answered: AnsweredHook,
    pub(crate) closed: CloseHook,
    pub(crate) data_halt: DataHaltHook,
    pub(crate) stuck_stop: StuckStopHook,
}

/// 승인 결과의 글자.
pub(crate) fn approval_status_word(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::Pending => "pending",
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Rejected => "rejected",
        ApprovalStatus::NoAnswer => "no_answer",
        ApprovalStatus::Expired => "expired",
    }
}

/// (34차 1번, C27) 승인 카드 이벤트의 `"state"` 글자: 기다림 · 답함(승인 · 거절) · 만료(답 없음 · 만료).
pub(crate) fn approval_state_word(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::Pending => "waiting",
        ApprovalStatus::Approved | ApprovalStatus::Rejected => "answered",
        ApprovalStatus::NoAnswer | ApprovalStatus::Expired => "expired",
    }
}

/// (34차 1번, C27) ✋를 띄울 담당 id(`pi_office::Agent::id`): 주문 → 매매 담당, 규칙 개선안 → 복기 담당,
/// 실전 승격 · 다시 시작 → 팀장.
pub(crate) fn approval_agent(subject: &Subject) -> &'static str {
    match subject {
        Subject::Order(_) => pi_office::Agent::Trader.id(),
        Subject::RuleImprovement { .. } => pi_office::Agent::Review.id(),
        Subject::LivePromotion { .. } | Subject::Restart => pi_office::Agent::Leader.id(),
    }
}

/// 실행기가 단 주문 키(`pi-exec-<매매>-…`)에서 매매 번호를 읽는다.
fn trade_of(order: &Order) -> Option<i64> {
    let key = order.client_order_id.as_deref()?;
    key.strip_prefix("pi-exec-")?
        .split('-')
        .next()?
        .parse()
        .ok()
}

/// 체결된 만큼의 돈 흐름(판 것은 +, 산 것은 −, 비용은 −).
fn cash_flow(order: &Order) -> Decimal {
    let e = &order.execution;
    let costs = e.commission + e.tax;
    match order.side {
        Side::Sell => e.filled_amount - costs,
        Side::Buy => -(e.filled_amount + costs),
    }
}

impl Trading {
    /// 알리는 자리를 붙인다(한 번만).
    pub(crate) fn hook(&self, hooks: Hooks) {
        let _ = self.hooks.set(hooks);
    }

    /// (28차 3번) 개선안에 답하는 자리를 붙인다(한 번만). 팀을 연 조립이 붙인다.
    pub(crate) fn hook_proposals(&self, hook: ProposalHook) {
        let _ = self.proposal_hook.set(hook);
    }

    /// 그 승인 번호가 규칙 개선안(복기 · 사용자 변경)을 묻는 것이면 그 개선안 번호.
    pub(super) fn proposal_of(
        &self,
        number: &ApprovalNumber,
    ) -> Result<Option<ProposalId>, TradingError> {
        let view = self.approvals.get(number).map_err(exec_text)?;
        Ok(match view.map(|view| view.subject) {
            Some(Subject::RuleImprovement { proposal }) => Some(proposal),
            _ => None,
        })
    }

    /// (28차 3번) 개선안의 승인 번호에 온 승인 카드 답을 개선안 답하기와 같은 길로 보낸다. 답할 팀이 없으면
    /// 승인 번호를 건드리지 않고 [`PROPOSAL_UNAVAILABLE`]로 거절한다(승인만 닫히고 개선안이 남지 않게).
    pub(super) fn answer_proposal_card(
        &self,
        proposal: ProposalId,
        number: &ApprovalNumber,
        shown: &str,
        decision: Decision,
    ) -> Result<Answered, TradingError> {
        match self.proposal_hook.get() {
            Some(hook) => hook(proposal, number, shown, decision),
            None => Ok(rejected(PROPOSAL_UNAVAILABLE)),
        }
    }

    pub(super) fn announce_approval(&self, number: &ApprovalNumber, summary: &str) {
        if let Some(hooks) = self.hooks.get() {
            (hooks.approval)(number.clone(), summary.to_string());
        }
    }

    pub(super) fn announce_answered(&self, number: &ApprovalNumber) {
        if let Some(hooks) = self.hooks.get() {
            (hooks.answered)(number.clone());
        }
    }

    pub(super) fn announce_close(&self, date: NaiveDate) {
        if let Some(hooks) = self.hooks.get() {
            (hooks.closed)(date);
        }
    }

    pub(super) fn announce_data_halt(&self) {
        if let Some(hooks) = self.hooks.get() {
            (hooks.data_halt)();
        }
    }

    pub(super) fn announce_stuck_stop(&self, symbol: &str, quantity: &str) {
        if let Some(hooks) = self.hooks.get() {
            (hooks.stuck_stop)(symbol.to_string(), quantity.to_string());
        }
    }

    /// 승인 요청 하나(없으면 `None`).
    pub(crate) fn approval_view(&self, number: &str) -> Result<Option<ApprovalView>, TradingError> {
        self.approvals
            .get(&ApprovalNumber::new(number))
            .map_err(exec_text)
    }

    /// (34차 1번, C27 · E49) 아직 기다리는 승인 요청과 만든 시각, 만든 차례(같은 시각이면 번호 순)로.
    /// 저장소의 결과가 기다림이어도 실행 때 지문이 달라 무효가 된 것(거절로 보임)은 빠진다.
    pub(crate) fn pending_approvals(&self) -> Result<Vec<(ApprovalView, i64)>, TradingError> {
        let rows = self.store.approvals_in_status(ApprovalStatus::Pending)?;
        let mut out = Vec::new();
        for row in rows {
            let Some(number) = row
                .summary
                .get("number")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            if let Some(view) = self.approval_view(number)?
                && view.status == ApprovalStatus::Pending
            {
                out.push((row.id.0, view, row.created_at.0));
            }
        }
        out.sort_by_key(|(id, _, at)| (*at, *id));
        Ok(out.into_iter().map(|(_, view, at)| (view, at)).collect())
    }

    /// 승인 요청 알림을 보낸 결과를 남긴다(P20).
    pub(crate) fn record_approval_notice(&self, number: &ApprovalNumber, sent: bool) {
        let notice = if sent { Notice::Sent } else { Notice::Failed };
        if let Err(error) = self.approvals.record_notice(number, notice) {
            tracing::warn!(%error, "승인 요청 알림 결과를 남기지 못했습니다");
        }
    }

    /// (10차 10b 0번 나) 보유 종목들(손절 감시 자리가 있는 매매의 종목, 겹치지 않게 매매 번호 순서).
    /// 시세 흐름이 지켜보는 목록에 없어도 함께 받는다(보유의 손절 감시가 시세 없이 죽지 않게).
    pub(crate) fn held_symbols(&self) -> Vec<pi_market::Symbol> {
        let mut symbols: Vec<pi_market::Symbol> = Vec::new();
        for guard in self.executor.guards() {
            if !symbols.contains(&guard.symbol) {
                symbols.push(guard.symbol);
            }
        }
        symbols
    }

    /// (10차 10a 4번) 그 보유 매매에 걸려 있는(또는 다시 내기를 기다리는) 파는 주문이 있는가.
    pub(crate) async fn open_sell(&self, trade: TradeId) -> Result<bool, TradingError> {
        if self.executor.pending_sell_key(trade).is_some() {
            return Ok(true);
        }
        let now = self.clock.now();
        let open = Broker::orders(self.paper(), OrderGroup::Open, None, now)
            .await
            .map_err(exec_text)?;
        Ok(open
            .iter()
            .any(|o| o.side == Side::Sell && trade_of(o) == Some(trade.0)))
    }

    /// (10차 10a 4번) 그 매매에 남긴 수량(산 수량). 없으면 없음.
    pub(crate) fn held_quantity(&self, trade: TradeId) -> Result<Option<u64>, TradingError> {
        Ok(self.store.trade_quantity(trade)?)
    }

    /// 모의계좌의 끝난 주문 전부.
    async fn closed_orders(&self) -> Result<Vec<Order>, TradingError> {
        let now = self.clock.now();
        Broker::orders(self.paper(), OrderGroup::Closed, None, now)
            .await
            .map_err(exec_text)
    }

    /// 그 시장 날짜에 낸 주문 가운데 다 체결된 수.
    pub(crate) async fn filled_on(&self, date: NaiveDate) -> Result<usize, TradingError> {
        let orders = self.closed_orders().await?;
        Ok(orders
            .iter()
            .filter(|o| o.status == OrderStatus::Filled && self.date_of(o.ordered_at) == date)
            .count())
    }

    /// 원화 장부의 그날 손익(지금 평가액 − 그날 시작 평가액).
    pub(crate) async fn day_pnl(&self) -> Result<Decimal, TradingError> {
        let books = self.books(self.clock.now()).await?;
        Ok(books
            .iter()
            .find(|b| b.currency == pi_market::Currency::Krw)
            .map_or(Decimal::ZERO, |b| b.equity - b.day_start_equity))
    }

    /// 팔린(복기됨 포함) 매매의 요약(조건 검색, 기록 키는 매매 번호 글자). 손익은 그 매매 주문들의 돈 흐름 합이다.
    pub(crate) async fn sold_records(&self) -> Result<Vec<TradeRecord>, TradingError> {
        let orders = self.closed_orders().await?;
        let mut trades = self.store.trades_in_state(TradeState::Sold)?;
        trades.extend(self.store.trades_in_state(TradeState::Reviewed)?);
        Ok(trades
            .into_iter()
            .map(|trade| {
                let mine: Vec<&Order> = orders
                    .iter()
                    .filter(|o| trade_of(o) == Some(trade.id.0))
                    .collect();
                let pnl = (!mine.is_empty()).then(|| mine.iter().map(|o| cash_flow(o)).sum());
                TradeRecord {
                    record_ref: trade.id.0.to_string(),
                    symbol: trade.symbol,
                    closed_at: trade.updated_at,
                    pnl,
                }
            })
            .collect())
    }
}
