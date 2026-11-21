//! 주문 안 하나: 쉬는 날(E19) → 규칙 점검(C9) → 실행(C10) 또는 승인 요청(C12).

use chrono::{DateTime, Duration as TimeDelta, Utc};
use pi_approval::{Ask, OrderSubject, SignalKind, Subject};
use pi_exec::{SellPurpose, SellSubmission, Submission};
use pi_market::MarketCalendar;
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, Order, OrderGroup, OrderType, Side, UnfilledTimeout,
};
use pi_rules::{
    ApiBudget, BlockKind, Blocked, Book, CheckContext, Outcome, Passed, PendingApproval, Purpose,
    Reason, TeamStatus, TradeStage, Violation,
};
use pi_store::{BlockReason, Trade, TradeId, TradeState};
use pi_template::{RiskParams, UnfilledCancel};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use super::answer::Waiting;
use super::base::{exec_text, lock, millis};
use super::day::status;
use super::{OrderRequest, Placed, Trading};
use crate::TradingError;

/// 승인 요청의 신호가 유효한 가장 짧은 시간(오늘 장 끝을 모를 때).
const SIGNAL_FALLBACK_MINUTES: i64 = 5;

/// 점검에 넣는 바깥 값(빌려 쓰는 칸의 주인).
pub(super) struct Inputs {
    now: DateTime<Utc>,
    pub(super) calendar: MarketCalendar,
    quote: pi_market::Quote,
    book: Book,
    open: Vec<ApprovedOrder>,
    trades_today: u32,
    team: TeamStatus,
}

impl Inputs {
    /// 규칙 엔진의 점검 입력.
    pub(super) fn context<'a>(
        &'a self,
        trading: &'a Trading,
        params: &'a RiskParams,
        request: &OrderRequest,
    ) -> CheckContext<'a> {
        CheckContext {
            now: self.now,
            params,
            config: &trading.rules,
            market_rules: trading.market_rules(request.draft.market),
            calendar: &self.calendar,
            quote: &self.quote,
            stale: trading.paper_config.stale,
            reference_price: request.reference_price,
            status: request.status,
            team: self.team,
            book: &self.book,
            // 시세 공급자 오류 · 자료 받기의 연속 실패는 실제 신호다(10차 10a 2번).
            health: trading.health(self.now),
            open_orders: &self.open,
            trades_today: self.trades_today,
            // 모의계좌에는 바깥 API 요청 한도가 없다.
            api: ApiBudget {
                used: 0,
                limit: u32::MAX,
            },
        }
    }
}

/// 규칙 점검에 넘길 매매 상태. 저장소의 상태를 쓰되, 이 길이 받는 상태(사기: 판단됨, 팔기: 보유중인
/// 매매의 팔기 판단)는 판단됨으로 본다(`draft.stage`는 믿지 않는다).
pub(super) fn stage_for(purpose: Purpose, state: TradeState) -> TradeStage {
    match (purpose.is_buy(), state) {
        (true, TradeState::Judged) | (false, TradeState::Holding) => TradeStage::Judged,
        (_, TradeState::Discussing) => TradeStage::Discussing,
        (_, TradeState::Judged) => TradeStage::Judged,
        (_, TradeState::AwaitingApproval) => TradeStage::AwaitingApproval,
        (_, TradeState::Blocked) => TradeStage::Blocked,
        (_, TradeState::Ordered) => TradeStage::Ordered,
        (_, TradeState::Holding) => TradeStage::Holding,
        (_, TradeState::Sold) => TradeStage::Sold,
        (_, TradeState::Reviewed) => TradeStage::Reviewed,
    }
}

/// 점검 입력이 모자라 막는다(시세나 달력을 모른다).
fn missing(violation: Violation) -> Blocked {
    Blocked {
        kind: BlockKind::PrincipleOrRationale,
        reason: Reason::Violations(vec![violation]),
    }
}

fn approved_of(order: &Order) -> ApprovedOrder {
    ApprovedOrder {
        market: order.market,
        symbol: order.symbol.clone(),
        side: order.side,
        order_type: order.order_type,
        quantity: order.quantity,
        price: order.price,
        account_kind: AccountKind::Paper,
    }
}

pub(super) fn timeout_of(cancel: UnfilledCancel) -> UnfilledTimeout {
    match cancel {
        UnfilledCancel::After(after) => UnfilledTimeout::After(after),
        UnfilledCancel::AtClose => UnfilledTimeout::SessionClose,
    }
}

fn sell_purpose(purpose: Purpose) -> SellPurpose {
    match purpose {
        Purpose::StopLoss => SellPurpose::StopLoss,
        Purpose::TakeProfit => SellPurpose::TakeProfit,
        // 팀장 팔기는 같은 종목의 다른 열린 파는 주문을 취소하지 않는다(보유가 여럿이면 매도가 모두 남는다, exec 7차).
        Purpose::TeamExit => SellPurpose::TeamExit,
        _ => SellPurpose::Liquidation,
    }
}

impl Trading {
    /// [`Trading::place`]의 몸통.
    pub(super) async fn place_order(&self, request: OrderRequest) -> Result<Placed, TradingError> {
        self.require_setup()?;
        let purpose = request.draft.purpose;
        // 파는 주문은 마지막으로 쓰던 템플릿의 값으로(멈춤이어도), 사는 주문은 쓸 수 있는 템플릿만(`chosen`).
        let params = self.params_for(purpose)?.ok_or(TradingError::NoTemplate)?;
        let trade = self.trade(request.trade_id)?;
        let now = self.clock.now();
        if purpose == Purpose::StopLoss
            && let Some(id) = self.stop_sent(trade.id).await
        {
            return Ok(Placed::Ordered(id));
        }
        if let Some(reason) = self.rest_reason(now)
            && !matches!(purpose, Purpose::StopLoss | Purpose::Liquidation)
        {
            return Ok(Placed::RestDay(reason));
        }
        let mut request = request;
        request.draft.stage = stage_for(purpose, trade.state);
        let inputs = match self.inputs(&request, now).await? {
            Ok(inputs) => inputs,
            Err(blocked) => return self.blocked(&trade, blocked),
        };
        let outcome = pi_rules::check(&request.draft, &inputs.context(self, &params, &request));
        match outcome {
            Outcome::Blocked(blocked) => self.blocked(&trade, blocked),
            Outcome::NeedsApproval(pending) => {
                self.ask_approval(&trade, request, pending, &inputs.calendar)
            }
            Outcome::Pass(passed) => {
                let placed = self.execute(trade.id, purpose, passed, &params).await?;
                if purpose == Purpose::StopLoss {
                    self.note_stop(trade.id, &placed);
                }
                Ok(placed)
            }
        }
    }

    pub(crate) fn trade(&self, id: TradeId) -> Result<Trade, TradingError> {
        match self.store.get_trade(id) {
            Ok(trade) => Ok(trade),
            Err(pi_store::StoreError::TradeNotFound(..)) => Err(TradingError::TradeNotFound),
            Err(error) => Err(error.into()),
        }
    }

    /// 점검 입력을 모은다. 시세나 달력을 모르면 막힘을 돌려준다.
    pub(super) async fn inputs(
        &self,
        request: &OrderRequest,
        now: DateTime<Utc>,
    ) -> Result<Result<Inputs, Blocked>, TradingError> {
        let draft = &request.draft;
        let Some(calendar) = self.calendar(now) else {
            return Ok(Err(missing(Violation::MarketClosed)));
        };
        let Some(quote) = self.last_quote(draft.symbol.as_str()) else {
            return Ok(Err(missing(Violation::StaleQuote)));
        };
        let paper = self.executor.broker();
        let reference = request.reference_price;
        paper.set_reference_price(draft.symbol.clone(), reference, calendar.today.date);
        let currency = self.market_rules(draft.market).currency.clone();
        let books = self.books(now).await?;
        let book = books.into_iter().find(|book| book.currency == currency);
        let book = book.ok_or_else(|| exec_text("그 통화의 장부가 없습니다"))?;
        let mut orders = Broker::orders(paper, OrderGroup::Open, None, now)
            .await
            .map_err(exec_text)?;
        let open = orders.iter().map(approved_of).collect();
        orders.extend(
            Broker::orders(paper, OrderGroup::Closed, None, now)
                .await
                .map_err(exec_text)?,
        );
        let today = calendar.today.date;
        let bought = orders
            .iter()
            .filter(|o| o.side == Side::Buy && self.date_of(o.ordered_at) == today)
            .count();
        let team = status(self.store.team_state()?);
        Ok(Ok(Inputs {
            now,
            calendar,
            quote,
            book,
            open,
            trades_today: u32::try_from(bought).unwrap_or(u32::MAX),
            team,
        }))
    }

    /// 규칙 엔진이 막았다(V26): 판단됨인 매매는 막힘으로 옮기고 이유를 남긴다.
    fn blocked(&self, trade: &Trade, blocked: Blocked) -> Result<Placed, TradingError> {
        if trade.state == TradeState::Judged {
            let reason = BlockReason {
                kind: pi_store::BlockKind::PrincipleOrRationale,
                detail: blocked.detail(),
            };
            self.store
                .block_trade(trade.id, reason, millis(self.clock.now()))?;
        }
        Ok(Placed::Blocked(blocked))
    }

    /// 점검을 지난 주문을 낸다: 점검 통과 표시는 규칙 엔진의 길(`pi_rules::issue_for_trade`)로만 받고
    /// 그 매매에 묶는다(E33). 사는 주문은 규칙 엔진이 정한 수량을 매매에 남긴다(실행기가 대조한다).
    pub(super) async fn execute(
        &self,
        trade: TradeId,
        purpose: Purpose,
        passed: Passed,
        params: &RiskParams,
    ) -> Result<Placed, TradingError> {
        let approval = passed.approval().cloned();
        let quantity = passed.order().quantity;
        let permit = pi_rules::issue_for_trade(&self.authority, passed, trade.0);
        let timeout = timeout_of(params.unfilled_cancel);
        let sent = if purpose.is_buy() {
            self.keep_quantity(trade, quantity)?;
            // 실전중 템플릿의 주문은 실전 계좌의 것이다. 실전 주문은 만들지 않으므로 실행기가 막는다(E25).
            let template_account = if self.live_in_use()? {
                AccountKind::Live
            } else {
                AccountKind::Paper
            };
            let submission = Submission {
                trade_id: trade,
                permit,
                template_account,
                approval,
                timeout,
            };
            self.executor.submit(submission).await
        } else {
            let submission = SellSubmission {
                trade_id: trade,
                permit,
                purpose: sell_purpose(purpose),
                timeout,
            };
            self.executor.sell(submission).await
        };
        match sent {
            Ok(id) => Ok(Placed::Ordered(id)),
            Err(pi_exec::Error::AnomalyStop(error)) => {
                self.anomaly()?;
                Err(TradingError::Broker(error.to_string()))
            }
            Err(error) => Err(exec_text(error)),
        }
    }

    /// 사는 수량을 매매에 남긴다. 승인이 붙어 바꿀 수 없으면 그대로 둔다(승인의 수량을 실행기가 대조한다).
    pub(super) fn keep_quantity(
        &self,
        trade: TradeId,
        quantity: Decimal,
    ) -> Result<(), TradingError> {
        let Some(quantity) = quantity.trunc().to_u64() else {
            return Ok(());
        };
        if self.store.trade_quantity(trade)? == Some(quantity) {
            return Ok(());
        }
        match self.store.set_trade_quantity(trade, quantity) {
            Ok(()) | Err(pi_store::StoreError::QuantityLocked { .. }) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// 금액 한도를 넘었다(C12): 승인 요청을 만들고 매매를 승인대기로 옮긴다. 다시 점검할 것은 기억한다.
    fn ask_approval(
        &self,
        trade: &Trade,
        request: OrderRequest,
        pending: PendingApproval,
        calendar: &MarketCalendar,
    ) -> Result<Placed, TradingError> {
        let now = self.clock.now();
        let close = calendar.today.sessions.iter().map(|s| s.end).max();
        let until = close
            .filter(|end| *end > now)
            .unwrap_or(now + TimeDelta::minutes(SIGNAL_FALLBACK_MINUTES));
        let subject = order_subject(trade.id, pending.order(), millis(until))?;
        let reason = format!(
            "주문 금액 {}이 자동 주문 금액 한도 {}를 넘습니다",
            pending.amount(),
            pending.limit()
        );
        let summary = format!("{} · {reason}", trade.symbol);
        let ask = Ask {
            subject: subject.clone(),
            reason,
        };
        let requested = self.approvals.request(ask).map_err(exec_text)?;
        let to = TradeState::AwaitingApproval;
        self.store.transition_trade(trade.id, to, millis(now))?;
        let waiting = Waiting {
            trade: trade.id,
            request,
            pending,
            subject,
        };
        lock(&self.waiting).insert(requested.number.as_str().to_string(), waiting);
        self.announce_approval(&requested.number, &summary);
        Ok(Placed::AwaitingApproval {
            number: requested.number,
            fingerprint: requested.fingerprint,
        })
    }
}

/// 승인할 주문 내용(지문에 들어가는 칸: 계좌 · 종목 · 방향 · 수량 · 값 · 호가 유형 · 신호 id).
fn order_subject(
    trade: TradeId,
    order: &ApprovedOrder,
    until: pi_store::UnixMillis,
) -> Result<Subject, TradingError> {
    let quantity = order
        .quantity
        .trunc()
        .to_u64()
        .ok_or_else(|| exec_text("주문 수량을 셀 수 없습니다"))?;
    let (side, signal_kind) = match order.side {
        Side::Buy => (pi_approval::Side::Buy, SignalKind::Buy),
        Side::Sell => (pi_approval::Side::Sell, SignalKind::StopLoss),
    };
    let order_type = match order.order_type {
        OrderType::Limit => pi_approval::OrderType::Limit,
        OrderType::Market => pi_approval::OrderType::Market,
    };
    Ok(Subject::Order(OrderSubject {
        account: pi_store::AccountKind::Paper,
        symbol: order.symbol.as_str().to_string(),
        side,
        quantity,
        price: order.price.unwrap_or_default(),
        order_type,
        signal_id: format!("trade-{}", trade.0),
        signal_kind,
        signal_valid_until: until,
    }))
}
