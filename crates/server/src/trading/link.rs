//! (W5 3차) 주문 길이 3차 조립(알림 · 하루 정리 · 지식 검색)에 내주는 것.

use chrono::NaiveDate;
use pi_approval::{ApprovalNumber, ApprovalView, Notice};
use pi_knowledge::TradeRecord;
use pi_paper::{Broker, Order, OrderGroup, OrderStatus, Side};
use pi_store::{ApprovalStatus, TradeState};
use rust_decimal::Decimal;

use super::Trading;
use super::base::exec_text;
use crate::TradingError;

/// 승인 요청이 생겼을 때 부른다(승인 번호, 사람이 읽는 요약). 오래 붙잡지 않는다.
pub(crate) type ApprovalHook = Box<dyn Fn(ApprovalNumber, String) + Send + Sync>;
/// 하루 마감을 남긴 뒤 부른다(시장 날짜). 오래 붙잡지 않는다.
pub(crate) type CloseHook = Box<dyn Fn(NaiveDate) + Send + Sync>;

/// 주문 길이 알리는 자리.
pub(crate) struct Hooks {
    pub(crate) approval: ApprovalHook,
    pub(crate) closed: CloseHook,
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

    pub(super) fn announce_approval(&self, number: &ApprovalNumber, summary: &str) {
        if let Some(hooks) = self.hooks.get() {
            (hooks.approval)(number.clone(), summary.to_string());
        }
    }

    pub(super) fn announce_close(&self, date: NaiveDate) {
        if let Some(hooks) = self.hooks.get() {
            (hooks.closed)(date);
        }
    }

    /// 승인 요청 하나(없으면 `None`).
    pub(crate) fn approval_view(&self, number: &str) -> Result<Option<ApprovalView>, TradingError> {
        self.approvals
            .get(&ApprovalNumber::new(number))
            .map_err(exec_text)
    }

    /// 승인 요청 알림을 보낸 결과를 남긴다(P20).
    pub(crate) fn record_approval_notice(&self, number: &ApprovalNumber, sent: bool) {
        let notice = if sent { Notice::Sent } else { Notice::Failed };
        if let Err(error) = self.approvals.record_notice(number, notice) {
            tracing::warn!(%error, "승인 요청 알림 결과를 남기지 못했습니다");
        }
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
