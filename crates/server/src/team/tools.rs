//! 실행기의 도구 포트: 주문 안을 주문 길로 넘긴다.
//!
//! 판단 자료는 논의 요청에 미리 담아 넘기므로 담당이 부를 도구는 없다(모르는 도구는 실행기가 거른다).
//! 주문 안(종목 · 방향)에는 수량 · 가격이 없다: 진입가 · 손절값 · 근거 · 자료는 저장소의 판단에서,
//! 기준가는 그 종목의 마지막 시세에서 읽어 규칙 점검으로 넘긴다(수량은 규칙 엔진이 정한다, P7).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use pi_agent::{ActionCall, OrderDraft, Role, Side, ToolError, Tools};
use pi_market::{Market, Symbol};
use pi_paper::OrderType;
use pi_rules::{Funding, ProductKind, Purpose, Rationale, SourceStamp, SymbolStatus, TradeStage};
use pi_store::{Amount, Trade, TradeId, UnixMillis};
use rust_decimal::Decimal;

use crate::{OrderRequest, Placed, Trading};

/// 도구 포트.
pub(super) struct CrewTools {
    pub(super) trading: Arc<Trading>,
    /// 시연 모드면 VI · 거래정지가 없다고 안다(가짜 시세에는 그런 일이 없다). 아니면 모른다(사는 주문은 막힌다).
    pub(super) demo: bool,
    /// 매매마다 주문 길에 넣은 결과.
    pub(super) placed: Mutex<HashMap<TradeId, Placed>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn time_of(at: UnixMillis) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(at.0).single().unwrap_or_default()
}

fn money(amount: Option<Amount>) -> Option<Decimal> {
    amount.map(|amount| Decimal::from(amount.0))
}

impl CrewTools {
    /// 넘긴 주문 안의 결과를 가져간다.
    pub(super) fn take(&self, trade: TradeId) -> Option<Placed> {
        lock(&self.placed).remove(&trade)
    }

    /// 판단된 매매로 주문 길의 주문 안을 만든다.
    fn request(&self, market: Market, trade: &Trade) -> Result<OrderRequest, ToolError> {
        let quote = self
            .trading
            .last_quote(&trade.symbol)
            .ok_or_else(|| ToolError("그 종목의 시세가 없습니다".to_string()))?;
        let decided = trade
            .decided_at
            .ok_or_else(|| ToolError("판단되지 않은 매매입니다".to_string()))?;
        let rationale = &trade.rationale;
        let draft = pi_rules::OrderDraft {
            market,
            symbol: Symbol::new(&trade.symbol),
            purpose: Purpose::NewEntry,
            order_type: OrderType::Limit,
            limit_price: money(rationale.entry),
            ai_quantity: None,
            funding: Funding::Cash,
            product: ProductKind::Stock,
            stage: TradeStage::Judged,
            decided_at: time_of(decided),
            rationale: Rationale {
                hypothesis: rationale.hypothesis.clone(),
                trigger: rationale.trigger.clone(),
                entry: money(rationale.entry),
                stop: money(rationale.stop),
                target: money(rationale.target),
                expected_value: money(rationale.expected_value),
            },
            sources: trade
                .sources
                .iter()
                .map(|source| SourceStamp {
                    name: source.name.clone(),
                    known_at: time_of(source.known_at),
                })
                .collect(),
        };
        let known = self.demo.then_some(false);
        Ok(OrderRequest {
            trade_id: trade.id,
            draft,
            reference_price: quote.price,
            status: SymbolStatus {
                vi: known,
                halted: known,
            },
        })
    }
}

#[async_trait]
impl Tools for CrewTools {
    fn names(&self, _role: Role) -> Vec<String> {
        Vec::new()
    }

    async fn call(&self, _role: Role, call: &ActionCall) -> Result<String, ToolError> {
        Err(ToolError(format!("그런 도구가 없습니다: {}", call.name)))
    }

    async fn submit_order_draft(&self, draft: &OrderDraft) -> Result<(), ToolError> {
        if draft.side != Side::Buy {
            return Err(ToolError("사는 주문 안만 받습니다".to_string()));
        }
        let trade = self
            .trading
            .trade(draft.trade)
            .map_err(|error| ToolError(error.to_string()))?;
        let request = self.request(self.trading.market(), &trade)?;
        let placed = self
            .trading
            .place(request)
            .await
            .map_err(|error| ToolError(error.to_string()))?;
        lock(&self.placed).insert(draft.trade, placed);
        Ok(())
    }
}
