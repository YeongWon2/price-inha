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
use pi_agent::{ActionCall, OrderDraft, Role, SellDraft, Side, ToolError, Tools};
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
    /// 시연 모드면 VI · 거래정지가 없다고 안다(가짜 시세에는 그런 일이 없다). 아니면 최근 시세로 추정한다.
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

    /// 판단된 매매(`judged`)의 근거 · 자료로 규칙 점검에 넘길 주문 안을 만든다. 목적은 부르는 쪽(server 코드)이
    /// 정한다: AI의 글에서 온 주문 안은 언제나 사기([`Purpose::NewEntry`]) 또는 팀장 팔기([`Purpose::TeamExit`])다.
    /// 손절 · 청산 목적은 주문 길의 손절 · 청산 감시만 붙인다(10차 10a 3번, 라벨 위조 금지).
    fn draft_of(
        market: Market,
        judged: &Trade,
        purpose: Purpose,
        limit_price: Option<Decimal>,
        ai_quantity: Option<Decimal>,
    ) -> Result<pi_rules::OrderDraft, ToolError> {
        let decided = judged
            .decided_at
            .ok_or_else(|| ToolError("판단되지 않은 매매입니다".to_string()))?;
        let rationale = &judged.rationale;
        Ok(pi_rules::OrderDraft {
            market,
            symbol: Symbol::new(&judged.symbol),
            purpose,
            order_type: OrderType::Limit,
            limit_price,
            ai_quantity,
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
            sources: judged
                .sources
                .iter()
                .map(|source| SourceStamp {
                    name: source.name.clone(),
                    known_at: time_of(source.known_at),
                })
                .collect(),
        })
    }

    fn quote_of(&self, symbol: &str) -> Result<pi_market::Quote, ToolError> {
        self.trading
            .last_quote(symbol)
            .ok_or_else(|| ToolError("그 종목의 시세가 없습니다".to_string()))
    }

    /// 판단된 매매로 주문 길의 사는 주문 안을 만든다.
    fn request(
        &self,
        market: Market,
        trade: &Trade,
        draft: &OrderDraft,
    ) -> Result<OrderRequest, ToolError> {
        let quote = self.quote_of(&trade.symbol)?;
        let limit = smaller(money(market, trade.rationale.entry), draft.ai_price);
        let draft = Self::draft_of(market, trade, Purpose::NewEntry, limit, draft.ai_quantity)?;
        // 시연 모드는 VI · 거래정지가 없다고 안다. 아니면 최근 시세로 추정한다(결정 2026-11-21 5 · 11 · 14번):
        // 추정이 정상일 때만 지나고, 기준가(지난 정규장 마지막 시세)가 없으면 '모름'으로 막힌다.
        let (status, reference) = if self.demo {
            let known = SymbolStatus {
                vi: Some(false),
                halted: Some(false),
            };
            (known, None)
        } else {
            let now = self.trading.now();
            (
                self.trading.estimated_status(&trade.symbol, now),
                self.trading.reference_price(&trade.symbol, now),
            )
        };
        Ok(OrderRequest {
            trade_id: trade.id,
            draft,
            reference_price: reference.unwrap_or(quote.price),
            status,
        })
    }

    /// 팔기로 판단된 매매(`judged`)와 팔 보유 매매(`holding`)로 팀장 팔기 주문 안을 만든다(10차 10a 4번).
    /// 수량은 그 보유 매매가 가진 수량을 넘지 않는다(매매 담당이 더 많이 적어도). 값은 규칙 엔진이 정한다
    /// (매매 담당의 값이 없으면 시세).
    fn sell_request(
        &self,
        market: Market,
        judged: &Trade,
        holding: &Trade,
        draft: &SellDraft,
    ) -> Result<OrderRequest, ToolError> {
        let quote = self.quote_of(&holding.symbol)?;
        let held = self
            .trading
            .held_quantity(holding.id)
            .map_err(|error| ToolError(error.to_string()))?
            .map(Decimal::from);
        let quantity = smaller(held, draft.ai_quantity);
        let purpose = Purpose::TeamExit;
        let draft = Self::draft_of(market, judged, purpose, draft.ai_price, quantity)?;
        Ok(OrderRequest {
            trade_id: holding.id,
            draft,
            reference_price: self
                .trading
                .reference_price(&holding.symbol, self.trading.now())
                .unwrap_or(quote.price),
            // 파는 주문은 VI · 거래정지를 모른다고 막지 않는다.
            status: SymbolStatus {
                vi: None,
                halted: None,
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

    /// 팔기 주문 안(agent 9차)을 `Purpose::TeamExit` 매도로 점검 · 실행에 넘긴다. 같은 보유에 열린 매도가
    /// 있으면 다시 내지 않는다. 규칙이 막으면(전체 정지 · 쉬는 날 등) 내지 않고 기록만 남긴다.
    async fn submit_sell_draft(&self, draft: &SellDraft) -> Result<(), ToolError> {
        let tool = |error: crate::TradingError| ToolError(error.to_string());
        let judged = self.trading.trade(draft.judged).map_err(tool)?;
        let holding = self.trading.trade(draft.holding).map_err(tool)?;
        if holding.symbol != judged.symbol {
            return Err(ToolError("판단한 종목과 보유 종목이 다릅니다".to_string()));
        }
        if self.trading.open_sell(holding.id).await.map_err(tool)? {
            tracing::info!(
                trade = holding.id.0,
                "열린 매도가 있어 팔기 주문을 다시 내지 않는다"
            );
            return Ok(());
        }
        let request = self.sell_request(self.trading.market(), &judged, &holding, draft)?;
        let placed = self.trading.place(request).await.map_err(tool)?;
        if !matches!(placed, Placed::Ordered(_)) {
            tracing::info!(
                trade = holding.id.0,
                ?placed,
                "팀장 팔기 주문이 나가지 않았다"
            );
        }
        Ok(())
    }
}
