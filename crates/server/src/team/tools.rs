//! 실행기의 도구 포트: 주문 안을 주문 길로 넘긴다.
//!
//! 판단 자료는 논의 요청에 미리 담아 넘긴다. 담당이 부를 도구는 지식 검색([`crate::KNOWLEDGE_TOOL`], T16)
//! 하나다(모르는 도구는 실행기가 거른다). 진입가 · 손절값 · 근거 · 자료는 저장소의 판단에서(금액은 통화의
//! 최소 단위라 미국은 센트 ÷ 100), 기준가는 그 종목의 마지막 시세에서 읽어 규칙 점검으로 넘긴다. 수량 ·
//! 가격은 규칙 엔진이 정하고, 매매 담당이 적은 수량 · 가격이 더 작으면 작은 쪽이다(P7).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use pi_agent::{ActionCall, OrderDraft, Role, Side, ToolError, Tools};
use pi_market::{Market, Symbol};
use pi_paper::OrderType;
use pi_rules::{Funding, ProductKind, Purpose, Rationale, SourceStamp, SymbolStatus, TradeStage};
use pi_store::{Amount, Trade, TradeId, UnixMillis};
use rust_decimal::Decimal;

use super::Crew;
use crate::{KNOWLEDGE_TOOL, OrderRequest, Placed, Trading};

/// 도구 포트.
pub(super) struct CrewTools {
    pub(super) trading: Arc<Trading>,
    /// 시연 모드면 VI · 거래정지가 없다고 안다(가짜 시세에는 그런 일이 없다). 아니면 모른다(사는 주문은 막힌다).
    pub(super) demo: bool,
    /// 매매마다 주문 길에 넣은 결과.
    pub(super) placed: Mutex<HashMap<TradeId, Placed>>,
    /// 지식 검색 도구가 부르는 팀(만든 뒤 붙인다).
    pub(super) crew: OnceLock<Weak<Crew>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn time_of(at: UnixMillis) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(at.0).single().unwrap_or_default()
}

/// 저장소 금액(통화의 최소 단위)을 그 시장 통화의 값으로: 국내는 원, 미국은 센트 ÷ 100.
fn money(market: Market, amount: Option<Amount>) -> Option<Decimal> {
    amount.map(|amount| match market {
        Market::Us => Decimal::new(amount.0, 2),
        _ => Decimal::from(amount.0),
    })
}

/// 둘 다 있으면 작은 쪽(P7).
fn smaller(rule: Option<Decimal>, ai: Option<Decimal>) -> Option<Decimal> {
    match (rule, ai) {
        (Some(rule), Some(ai)) => Some(rule.min(ai)),
        (rule, ai) => rule.or(ai),
    }
}

impl CrewTools {
    /// 넘긴 주문 안의 결과를 가져간다.
    pub(super) fn take(&self, trade: TradeId) -> Option<Placed> {
        lock(&self.placed).remove(&trade)
    }

    /// 판단된 매매로 주문 길의 주문 안을 만든다.
    fn request(
        &self,
        market: Market,
        trade: &Trade,
        draft: &OrderDraft,
    ) -> Result<OrderRequest, ToolError> {
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
            limit_price: smaller(money(market, rationale.entry), draft.ai_price),
            ai_quantity: draft.ai_quantity,
            funding: Funding::Cash,
            product: ProductKind::Stock,
            stage: TradeStage::Judged,
            decided_at: time_of(decided),
            rationale: Rationale {
                hypothesis: rationale.hypothesis.clone(),
                trigger: rationale.trigger.clone(),
                entry: money(market, rationale.entry),
                stop: money(market, rationale.stop),
                target: money(market, rationale.target),
                expected_value: money(market, rationale.expected_value),
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
        vec![KNOWLEDGE_TOOL.to_string()]
    }

    async fn call(&self, _role: Role, call: &ActionCall) -> Result<String, ToolError> {
        if call.name != KNOWLEDGE_TOOL {
            return Err(ToolError(format!("그런 도구가 없습니다: {}", call.name)));
        }
        let crew = self
            .crew
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| ToolError("팀을 아직 잇지 않았습니다".to_string()))?;
        let found = crew
            .search_knowledge(&call.args)
            .await
            .map_err(|error| ToolError(error.to_string()))?;
        serde_json::to_string(&found).map_err(|error| ToolError(error.to_string()))
    }

    async fn submit_order_draft(&self, draft: &OrderDraft) -> Result<(), ToolError> {
        if draft.side != Side::Buy {
            return Err(ToolError("사는 주문 안만 받습니다".to_string()));
        }
        let trade = self
            .trading
            .trade(draft.trade)
            .map_err(|error| ToolError(error.to_string()))?;
        let request = self.request(self.trading.market(), &trade, draft)?;
        let placed = self
            .trading
            .place(request)
            .await
            .map_err(|error| ToolError(error.to_string()))?;
        lock(&self.placed).insert(draft.trade, placed);
        Ok(())
    }
}
