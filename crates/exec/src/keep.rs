//! 저장소에 남기는 계좌 스냅샷 문서(V6 · E30).
//!
//! 문서는 모의계좌의 [`Snapshot`] JSON에 이 실행기의 몫을 `"pi_exec"` 이름 하나로 더한 것이다:
//! 파는 주문 키의 순번과 다시 내기를 기다리는 파는 주문들(멱등성 키 포함), 보유 장부(3차). 되살릴
//! 때 이 이름을 떼어 내고 나머지를 모의계좌 스냅샷으로 읽는다. 이름이 없으면(1차 문서) 기다리는
//! 것이 없고, 장부가 없으면(2차 문서) 맞춰 볼 때 증권사 보유로 만든다.

use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use pi_market::{Market, Symbol};
use pi_paper::{AccountKind, ApprovedOrder, OrderType, Side, Snapshot, UnfilledTimeout};
use pi_store::{Amount, TradeId};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::error::Error;
use crate::executor::{SellPurpose, SellRetry};
use crate::track::Position;

/// 문서 안에서 이 실행기 몫의 이름.
const EXEC_KEY: &str = "pi_exec";

/// 다시 내기를 기다리는 파는 주문 하나와 그 미체결 취소 시간 · 멱등성 키.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pending {
    /// 공개하는 모습.
    pub(crate) retry: SellRetry,
    /// 미체결 취소 시간.
    pub(crate) timeout: UnfilledTimeout,
    /// 이 시도 묶음의 멱등성 키(다시 낼 때 같은 것을 쓴다).
    pub(crate) key: String,
}

/// 되살린 실행기 몫.
pub(crate) struct Restored {
    /// 모의계좌 스냅샷.
    pub(crate) snapshot: Snapshot,
    /// 파는 주문 키의 순번.
    pub(crate) seq: u64,
    /// 다시 내기를 기다리는 파는 주문들.
    pub(crate) pending: BTreeMap<TradeId, Pending>,
    /// 보유 장부.
    pub(crate) positions: BTreeMap<TradeId, Position>,
}

fn broken(what: &str) -> Error {
    Error::Snapshot(format!("실행기 몫을 읽을 수 없다: {what}"))
}

/// 모의계좌 스냅샷과 실행기 몫으로 문서를 만든다.
pub(crate) fn document(
    snapshot: &Snapshot,
    seq: u64,
    pending: &BTreeMap<TradeId, Pending>,
    positions: &BTreeMap<TradeId, Position>,
) -> Result<Value, Error> {
    let mut doc = serde_json::to_value(snapshot).map_err(|e| Error::Snapshot(e.to_string()))?;
    let Value::Object(fields) = &mut doc else {
        return Err(Error::Snapshot(
            "계좌 스냅샷이 JSON 객체가 아니다".to_string(),
        ));
    };
    let mut list = Vec::new();
    for item in pending.values() {
        list.push(encode(item)?);
    }
    let mut ledger = Vec::new();
    for (id, position) in positions {
        ledger.push(encode_position(*id, position)?);
    }
    fields.insert(
        EXEC_KEY.to_string(),
        json!({"seq": seq, "pending": list, "positions": ledger}),
    );
    Ok(doc)
}

/// 문서를 모의계좌 스냅샷과 실행기 몫으로 나눈다.
pub(crate) fn split(mut doc: Value) -> Result<Restored, Error> {
    let mine = match &mut doc {
        Value::Object(fields) => fields.remove(EXEC_KEY),
        _ => None,
    };
    let snapshot: Snapshot =
        serde_json::from_value(doc).map_err(|e| Error::Snapshot(e.to_string()))?;
    let mut restored = Restored {
        snapshot,
        seq: 0,
        pending: BTreeMap::new(),
        positions: BTreeMap::new(),
    };
    let Some(mine) = mine else {
        return Ok(restored);
    };
    restored.seq = mine
        .get("seq")
        .and_then(Value::as_u64)
        .ok_or_else(|| broken("seq"))?;
    let list = mine
        .get("pending")
        .and_then(Value::as_array)
        .ok_or_else(|| broken("pending"))?;
    for item in list {
        let pending = decode(item).ok_or_else(|| broken("pending"))?;
        restored.pending.insert(pending.retry.trade_id, pending);
    }
    if let Some(ledger) = mine.get("positions") {
        for item in ledger.as_array().ok_or_else(|| broken("positions"))? {
            let (id, position) = decode_position(item).ok_or_else(|| broken("positions"))?;
            restored.positions.insert(id, position);
        }
    }
    Ok(restored)
}

fn market_code(market: Market) -> Result<&'static str, Error> {
    match market {
        Market::Kr => Ok("kr"),
        Market::Us => Ok("us"),
        other => Err(Error::Snapshot(format!("모르는 시장: {other:?}"))),
    }
}

fn market_from(code: &str) -> Option<Market> {
    match code {
        "kr" => Some(Market::Kr),
        "us" => Some(Market::Us),
        _ => None,
    }
}

fn encode_position(id: TradeId, position: &Position) -> Result<Value, Error> {
    let sells: serde_json::Map<String, Value> = position
        .sells
        .iter()
        .map(|(order, filled)| (order.clone(), Value::String(filled.to_string())))
        .collect();
    Ok(json!({
        "trade_id": id.0,
        "market": market_code(position.market)?,
        "symbol": position.symbol.as_str(),
        "stop": position.stop.map(|a| a.0),
        "bought": position.bought.to_string(),
        "sells": sells,
    }))
}

fn decode_position(item: &Value) -> Option<(TradeId, Position)> {
    let stop = match item.get("stop")? {
        Value::Null => None,
        other => Some(Amount(other.as_i64()?)),
    };
    let mut sells = BTreeMap::new();
    for (order, filled) in item.get("sells")?.as_object()? {
        sells.insert(order.clone(), filled.as_str()?.parse::<Decimal>().ok()?);
    }
    let position = Position {
        market: market_from(item.get("market")?.as_str()?)?,
        symbol: Symbol::new(item.get("symbol")?.as_str()?),
        stop,
        bought: item.get("bought")?.as_str()?.parse().ok()?,
        sells,
    };
    Some((TradeId(item.get("trade_id")?.as_i64()?), position))
}

fn encode(item: &Pending) -> Result<Value, Error> {
    let retry = &item.retry;
    let order = &retry.order;
    let timeout = serde_json::to_value(item.timeout).map_err(|e| Error::Snapshot(e.to_string()))?;
    let market = market_code(order.market)?;
    Ok(json!({
        "trade_id": retry.trade_id.0,
        "purpose": purpose_code(retry.purpose),
        "attempts": retry.attempts,
        "retry_at": retry.retry_at.to_rfc3339_opts(SecondsFormat::AutoSi, true),
        "timeout": timeout,
        "key": item.key,
        "order": {
            "market": market,
            "symbol": order.symbol.as_str(),
            "side": match order.side { Side::Buy => "buy", Side::Sell => "sell" },
            "order_type": match order.order_type { OrderType::Limit => "limit", OrderType::Market => "market" },
            "quantity": order.quantity.to_string(),
            "price": order.price.map(|p| p.to_string()),
            "account": match order.account_kind { AccountKind::Paper => "paper", AccountKind::Live => "live" },
        },
    }))
}

fn decode(item: &Value) -> Option<Pending> {
    let text = |v: &Value, name: &str| v.get(name)?.as_str().map(str::to_string);
    let order = item.get("order")?;
    let price = match order.get("price")? {
        Value::Null => None,
        other => Some(other.as_str()?.parse::<Decimal>().ok()?),
    };
    let approved = ApprovedOrder {
        market: market_from(&text(order, "market")?)?,
        symbol: Symbol::new(text(order, "symbol")?),
        side: match text(order, "side")?.as_str() {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            _ => return None,
        },
        order_type: match text(order, "order_type")?.as_str() {
            "limit" => OrderType::Limit,
            "market" => OrderType::Market,
            _ => return None,
        },
        quantity: text(order, "quantity")?.parse().ok()?,
        price,
        account_kind: match text(order, "account")?.as_str() {
            "paper" => AccountKind::Paper,
            "live" => AccountKind::Live,
            _ => return None,
        },
    };
    let retry_at = DateTime::parse_from_rfc3339(&text(item, "retry_at")?)
        .ok()?
        .with_timezone(&Utc);
    Some(Pending {
        retry: SellRetry {
            trade_id: TradeId(item.get("trade_id")?.as_i64()?),
            purpose: purpose_from(&text(item, "purpose")?)?,
            order: approved,
            attempts: u32::try_from(item.get("attempts")?.as_u64()?).ok()?,
            retry_at,
        },
        timeout: serde_json::from_value(item.get("timeout")?.clone()).ok()?,
        key: text(item, "key")?,
    })
}

fn purpose_code(purpose: SellPurpose) -> &'static str {
    match purpose {
        SellPurpose::StopLoss => "stop_loss",
        SellPurpose::TakeProfit => "take_profit",
        SellPurpose::Liquidation => "liquidation",
        SellPurpose::TeamExit => todo!("7차: 팀장 팔기의 목적 글자"),
    }
}

fn purpose_from(code: &str) -> Option<SellPurpose> {
    match code {
        "stop_loss" => Some(SellPurpose::StopLoss),
        "take_profit" => Some(SellPurpose::TakeProfit),
        "liquidation" => Some(SellPurpose::Liquidation),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purposes_round_trip_and_unknown_is_none() {
        for purpose in [
            SellPurpose::StopLoss,
            SellPurpose::TakeProfit,
            SellPurpose::Liquidation,
        ] {
            assert_eq!(purpose_from(purpose_code(purpose)), Some(purpose));
        }
        assert_eq!(purpose_from("hold"), None);
        assert!(decode(&json!({"trade_id": 1})).is_none());
    }

    #[test]
    fn positions_round_trip() {
        let mut sells = BTreeMap::new();
        sells.insert("o-2".to_string(), Decimal::new(4, 0));
        let position = Position {
            market: Market::Us,
            symbol: Symbol::new("AAPL"),
            stop: Some(Amount(15_050)),
            bought: Decimal::new(105, 1),
            sells,
        };
        let encoded = encode_position(TradeId(3), &position).unwrap();
        assert_eq!(decode_position(&encoded), Some((TradeId(3), position)));
        assert!(decode_position(&json!({"trade_id": 3})).is_none());
    }
}
