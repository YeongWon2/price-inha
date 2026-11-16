//! 실행기 안쪽 상태와 체결 알림을 매매 전이로 옮기는 일.
//!
//! 매매와 증권사 주문은 멱등성 키(`client_order_id`)로 잇는다: 사기는 `pi-exec-<매매>-buy`,
//! 팔기는 `pi-exec-<매매>-sell-<시각>-<순번>`. 키는 계좌 스냅샷에 함께 남아 다시 켠 뒤에도 잇는다.
//! 같은 알림을 두 번 옮겨도 결과가 같다(수량은 더하지 않고 주문의 누적 체결 수량으로 둔다).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use pi_market::{Market, Symbol};
use pi_paper::{CloseReason, Order, OrderEvent, OrderStatus, Side, Snapshot, UnfilledTimeout};
use pi_store::{
    Amount, BlockKind, BlockReason, Store, StoreError, Trade, TradeId, TradeState, UnixMillis,
};
use rust_decimal::Decimal;
use tokio::sync::mpsc::Receiver;

use crate::error::Error;
use crate::executor::{Guard, SellRetry, TradeChange};

/// 멱등성 키의 앞머리.
const KEY_PREFIX: &str = "pi-exec-";

/// 보유중인 매매 하나의 수량 장부.
#[derive(Debug)]
pub(crate) struct Position {
    /// 시장.
    market: Market,
    /// 종목.
    symbol: Symbol,
    /// 매매 근거의 손절값.
    stop: Option<Amount>,
    /// 사는 주문의 누적 체결 수량.
    bought: Decimal,
    /// 파는 주문마다의 누적 체결 수량(주문 식별자 글자로).
    sells: BTreeMap<String, Decimal>,
}

impl Position {
    /// 체결된(아직 팔지 않은) 수량.
    fn remaining(&self) -> Decimal {
        self.bought - self.sells.values().copied().sum::<Decimal>()
    }
}

/// 실행기 안쪽 상태. 한 잠금 안에서만 다룬다(잠근 채로 기다리지 않는다).
#[derive(Debug)]
pub(crate) struct State {
    /// 맞춰 보기를 마쳤는가(E16).
    pub(crate) ready: bool,
    /// 보유중인 매매의 수량 장부.
    pub(crate) positions: BTreeMap<TradeId, Position>,
    /// 다시 내기를 기다리는 파는 주문과 그 미체결 취소 시간(E30).
    pub(crate) pending: BTreeMap<TradeId, (SellRetry, UnfilledTimeout)>,
    /// 체결 알림 구독.
    pub(crate) events: Receiver<OrderEvent>,
    /// 마지막으로 저장한 계좌 스냅샷.
    pub(crate) saved: Option<Snapshot>,
    /// 파는 주문 키의 순번.
    pub(crate) seq: u64,
}

impl State {
    /// 새 상태.
    pub(crate) fn new(events: Receiver<OrderEvent>, saved: Option<Snapshot>) -> Self {
        Self {
            ready: false,
            positions: BTreeMap::new(),
            pending: BTreeMap::new(),
            events,
            saved,
            seq: 0,
        }
    }

    /// 손절 감시 자리들(남은 수량이 있는 것만, 매매 번호 순서).
    pub(crate) fn guards(&self) -> Vec<Guard> {
        self.positions
            .iter()
            .filter_map(|(id, position)| {
                let stop = position.stop?;
                let quantity = position.remaining();
                (quantity > Decimal::ZERO).then(|| Guard {
                    trade_id: *id,
                    market: position.market,
                    symbol: position.symbol.clone(),
                    quantity,
                    stop,
                })
            })
            .collect()
    }

    /// 다음 파는 주문 키.
    pub(crate) fn sell_key(&mut self, trade_id: TradeId, now: DateTime<Utc>) -> String {
        self.seq += 1;
        format!(
            "{KEY_PREFIX}{}-sell-{}-{}",
            trade_id.0,
            now.timestamp_millis(),
            self.seq
        )
    }
}

/// 사는 주문 키.
pub(crate) fn buy_key(trade_id: TradeId) -> String {
    format!("{KEY_PREFIX}{}-buy", trade_id.0)
}

/// 키에서 매매 번호와 방향을 읽는다. 이 실행기의 키가 아니면 없음.
pub(crate) fn parse_key(key: &str) -> Option<(TradeId, Side)> {
    let rest = key.strip_prefix(KEY_PREFIX)?;
    let (id, tail) = rest.split_once('-')?;
    let id = TradeId(id.parse().ok()?);
    if tail == "buy" {
        Some((id, Side::Buy))
    } else if tail.starts_with("sell-") {
        Some((id, Side::Sell))
    } else {
        None
    }
}

/// 시각을 저장소 시각으로.
pub(crate) fn millis(now: DateTime<Utc>) -> UnixMillis {
    UnixMillis(now.timestamp_millis())
}

/// 매매 한 건. 저장소에 없으면 없음(이 실행기가 모르는 매매의 알림).
fn find_trade(store: &Store, id: TradeId) -> Result<Option<Trade>, Error> {
    match store.get_trade(id) {
        Ok(trade) => Ok(Some(trade)),
        Err(StoreError::TradeNotFound(_)) => Ok(None),
        Err(other) => Err(other.into()),
    }
}

/// 주문됨인 매매를 막힘(미체결 취소)으로 옮기고 이유를 남긴다(E12).
pub(crate) fn block_unfilled(
    store: &Store,
    trade_id: TradeId,
    detail: String,
    at: UnixMillis,
    changes: &mut Vec<TradeChange>,
) -> Result<(), Error> {
    store.block_trade(
        trade_id,
        BlockReason {
            kind: BlockKind::UnfilledCancelled,
            detail,
        },
        at,
    )?;
    changes.push(TradeChange {
        trade_id,
        to: TradeState::Blocked,
    });
    Ok(())
}

/// 하나도 체결되지 않고 끝난 주문의 이유 글.
fn unfilled_detail(order: &Order) -> String {
    let why = match order.close_reason {
        Some(CloseReason::UnfilledTimeout) => "정한 시간 안에 체결되지 않아 취소됨",
        Some(CloseReason::CancelRequested) => "취소 요청으로 취소됨",
        Some(CloseReason::InsufficientBuyingPower) => "체결할 돈이 모자라 취소됨",
        Some(CloseReason::Replaced) => "정정돼 다른 주문으로 바뀜",
        _ if order.status == OrderStatus::Rejected => "증권사가 거부함",
        _ => "체결 없이 끝남",
    };
    format!(
        "미체결 취소: {why} (주문 {}, 수량 {})",
        order.order_id.as_str(),
        order.quantity
    )
}

/// 주문 하나의 지금 모습을 매매 전이로 옮긴다. 여러 번 옮겨도 결과가 같다.
pub(crate) fn apply(
    store: &Store,
    state: &mut State,
    order: &Order,
    at: UnixMillis,
    changes: &mut Vec<TradeChange>,
) -> Result<(), Error> {
    let Some((trade_id, side)) = order.client_order_id.as_deref().and_then(parse_key) else {
        return Ok(());
    };
    match side {
        Side::Buy => apply_buy(store, state, trade_id, order, at, changes),
        Side::Sell => apply_sell(store, state, trade_id, order, at, changes),
    }
}

/// 사는 주문: 체결이 있으면 보유중(체결된 수량으로 감시), 체결 없이 끝났으면 막힘.
fn apply_buy(
    store: &Store,
    state: &mut State,
    trade_id: TradeId,
    order: &Order,
    at: UnixMillis,
    changes: &mut Vec<TradeChange>,
) -> Result<(), Error> {
    let filled = order.execution.filled_quantity;
    if filled <= Decimal::ZERO && order.status.is_open() {
        return Ok(());
    }
    let Some(trade) = find_trade(store, trade_id)? else {
        return Ok(());
    };
    if filled <= Decimal::ZERO {
        if trade.state == TradeState::Ordered {
            block_unfilled(store, trade_id, unfilled_detail(order), at, changes)?;
        }
        return Ok(());
    }
    match trade.state {
        TradeState::Ordered => {
            store.transition_trade(trade_id, TradeState::Holding, at)?;
            changes.push(TradeChange {
                trade_id,
                to: TradeState::Holding,
            });
        }
        TradeState::Holding => {}
        _ => return Ok(()),
    }
    let position = state.positions.entry(trade_id).or_insert_with(|| Position {
        market: order.market,
        symbol: order.symbol.clone(),
        stop: trade.rationale.stop,
        bought: Decimal::ZERO,
        sells: BTreeMap::new(),
    });
    position.bought = position.bought.max(filled);
    Ok(())
}

/// 파는 주문: 체결 수량을 적고, 다 팔렸으면 팔림.
fn apply_sell(
    store: &Store,
    state: &mut State,
    trade_id: TradeId,
    order: &Order,
    at: UnixMillis,
    changes: &mut Vec<TradeChange>,
) -> Result<(), Error> {
    let Some(position) = state.positions.get_mut(&trade_id) else {
        return Ok(());
    };
    let filled = order.execution.filled_quantity;
    let entry = position
        .sells
        .entry(order.order_id.as_str().to_string())
        .or_insert(Decimal::ZERO);
    *entry = (*entry).max(filled);
    if order.status != OrderStatus::Filled || position.remaining() > Decimal::ZERO {
        return Ok(());
    }
    if let Some(trade) = find_trade(store, trade_id)?
        && trade.state == TradeState::Holding
    {
        store.transition_trade(trade_id, TradeState::Sold, at)?;
        changes.push(TradeChange {
            trade_id,
            to: TradeState::Sold,
        });
    }
    state.positions.remove(&trade_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_carry_the_trade_and_the_side() {
        assert_eq!(
            parse_key(&buy_key(TradeId(7))),
            Some((TradeId(7), Side::Buy))
        );
        assert_eq!(
            parse_key("pi-exec-12-sell-1700000000000-3"),
            Some((TradeId(12), Side::Sell))
        );
        assert_eq!(parse_key("other-7-buy"), None);
        assert_eq!(parse_key("pi-exec-x-buy"), None);
        assert_eq!(parse_key("pi-exec-7-hold"), None);
    }
}
