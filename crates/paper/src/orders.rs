//! 주문 접수 · 정정 · 취소. 확인([`crate::accept`])을 통과한 것만 장부에 적는다.
//! 받지 않은 요청은 장부를 건드리지 않는다.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::accept::{self, Checked, Draft};
use crate::book::Book;
use crate::ledger::{Content, KeyRecord, MarketCode, OrderRecord};
use crate::{
    AccountKind, BrokerError, CloseReason, Fault, IDEMPOTENCY_WINDOW, InvalidReason, ModifyRequest,
    OrderAck, OrderEventKind, OrderId, OrderRequest, OrderStatus, PaperConfig, UnfilledTimeout,
};

/// 장애 흉내가 켜져 있으면 그 오류 (E13 · E30).
fn check_fault(book: &Book) -> Result<(), BrokerError> {
    match book.fault {
        Some(Fault::RejectOrders) => Err(BrokerError::Unavailable),
        Some(Fault::NoResponse) => Err(BrokerError::NoResponse),
        None => Ok(()),
    }
}

/// 열린 주문의 자리. 없으면 `OrderNotFound`, 끝났으면 왜 끝났는지에 맞는 오류.
fn open_index(book: &Book, order_id: &OrderId) -> Result<usize, BrokerError> {
    let index = book
        .ledger
        .order_index(order_id)
        .ok_or(BrokerError::OrderNotFound)?;
    let status = book
        .ledger
        .orders
        .get(index)
        .map(|order| order.status)
        .ok_or(BrokerError::OrderNotFound)?;
    match status {
        status if status.is_open() => Ok(index),
        OrderStatus::Filled => Err(BrokerError::AlreadyFilled),
        OrderStatus::Canceled => Err(BrokerError::AlreadyCanceled),
        OrderStatus::Replaced => Err(BrokerError::AlreadyModified),
        _ => Err(BrokerError::AlreadyRejected),
    }
}

/// 같은 키가 [`IDEMPOTENCY_WINDOW`] 안에 있었으면 앞의 결과(같은 내용) 또는 충돌(다른 내용).
fn replay(
    book: &Book,
    key: Option<&String>,
    content: Option<&Content>,
    now: DateTime<Utc>,
) -> Option<Result<OrderAck, BrokerError>> {
    let (key, content) = (key?, content?);
    let found = book.ledger.idempotency.get(key)?;
    // 시각이 접수 때보다 앞이면(시계가 어긋남) 아직 살아 있는 것으로 본다.
    let alive = now
        .signed_duration_since(found.accepted_at)
        .to_std()
        .map_or(true, |age| age < IDEMPOTENCY_WINDOW);
    if !alive {
        return None;
    }
    Some(if &found.content == content {
        Ok(OrderAck {
            order_id: found.order_id.clone(),
            client_order_id: Some(key.clone()),
        })
    } else {
        Err(BrokerError::IdempotencyKeyConflict)
    })
}

/// 미체결 취소 시각.
fn deadline(timeout: UnfilledTimeout, checked: &Checked, now: DateTime<Utc>) -> DateTime<Utc> {
    match timeout {
        UnfilledTimeout::SessionClose => checked.session.end,
        UnfilledTimeout::After(wait) => chrono::Duration::from_std(wait)
            .ok()
            .and_then(|wait| now.checked_add_signed(wait))
            .unwrap_or(DateTime::<Utc>::MAX_UTC),
    }
}

/// 확인을 통과한 주문을 장부에 적고 돈을 묶는다. 적은 자리를 돌려준다.
fn open_order(
    book: &mut Book,
    draft: &Draft,
    checked: &Checked,
    client_order_id: Option<String>,
    deadline: DateTime<Utc>,
    now: DateTime<Utc>,
) -> usize {
    let order_id = book.ledger.issue_id();
    book.ledger.cash_mut(checked.currency).available -= checked.hold;
    book.ledger.orders.push(OrderRecord {
        order_id,
        client_order_id,
        market: checked.market,
        symbol: draft.symbol.as_str().to_owned(),
        side: draft.side,
        order_type: draft.order_type,
        status: OrderStatus::Pending,
        price: draft.price,
        quantity: draft.quantity,
        currency: checked.currency,
        ordered_at: now,
        canceled_at: None,
        filled_quantity: Decimal::ZERO,
        filled_amount: Decimal::ZERO,
        commission: Decimal::ZERO,
        tax: Decimal::ZERO,
        filled_at: None,
        settlement_date: None,
        close_reason: None,
        hold_price: checked.hold_price,
        held_cash: checked.hold,
        deadline,
    });
    book.ledger.orders.len() - 1
}

/// 주문 접수. 확인 순서는 [`crate::paper`]의 모듈 문서 1~9.
pub(crate) fn place(
    book: &mut Book,
    config: &PaperConfig,
    seal: &Arc<()>,
    request: OrderRequest,
    now: DateTime<Utc>,
) -> Result<OrderAck, BrokerError> {
    check_fault(book)?;
    if request.account_kind != AccountKind::Paper {
        return Err(BrokerError::AccountKindMismatch {
            order: request.account_kind,
            broker: AccountKind::Paper,
        });
    }
    let draft = Draft {
        market: request.market,
        symbol: request.symbol,
        side: request.side,
        order_type: request.order_type,
        quantity: request.quantity,
        price: request.price,
    };
    accept::check_permit(request.permit.as_ref(), seal, &draft)?;

    let content = MarketCode::of(draft.market).map(|market| Content {
        market,
        symbol: draft.symbol.as_str().to_owned(),
        side: draft.side,
        order_type: draft.order_type,
        quantity: draft.quantity,
        price: draft.price,
        timeout: request.timeout,
    });
    let key = request.client_order_id;
    if let Some(result) = replay(book, key.as_ref(), content.as_ref(), now) {
        return result;
    }

    let checked = accept::check_draft(book, config, &draft, now, None)?;
    let deadline = deadline(request.timeout, &checked, now);
    let index = open_order(book, &draft, &checked, key.clone(), deadline, now);
    let order_id = book
        .ledger
        .orders
        .get(index)
        .map(|order| order.order_id.clone())
        .ok_or(BrokerError::OrderNotFound)?;
    if let (Some(key), Some(content)) = (key.clone(), content) {
        let record = KeyRecord {
            order_id: order_id.clone(),
            accepted_at: now,
            content,
        };
        book.ledger.idempotency.insert(key, record);
    }
    book.emit(OrderEventKind::Pending, index, &mut Vec::new());
    Ok(OrderAck {
        order_id,
        client_order_id: key,
    })
}

/// 주문 정정. 원주문을 `Replaced`로 끝내고 새 주문이 그 자리를 잇는다.
pub(crate) fn modify(
    book: &mut Book,
    config: &PaperConfig,
    seal: &Arc<()>,
    order_id: &OrderId,
    request: ModifyRequest,
    now: DateTime<Utc>,
) -> Result<OrderId, BrokerError> {
    check_fault(book)?;
    let index = open_index(book, order_id)?;
    let original = book
        .ledger
        .orders
        .get(index)
        .cloned()
        .ok_or(BrokerError::OrderNotFound)?;
    // 명세: 국내는 수량이 꼭 있어야 하고, 미국은 수량을 줄 수 없다(값만 바꾼다).
    let quantity = match (original.market, request.quantity) {
        (MarketCode::Kr, Some(quantity)) => quantity,
        (MarketCode::Kr, None) => {
            return Err(BrokerError::InvalidRequest(
                InvalidReason::ModifyQuantityRequired,
            ));
        }
        (MarketCode::Us, Some(_)) => return Err(BrokerError::UsModifyQuantityNotSupported),
        (MarketCode::Us, None) => original.remaining(),
    };
    let draft = Draft {
        market: original.market.market(),
        symbol: pi_market::Symbol::new(original.symbol.clone()),
        side: original.side,
        order_type: request.order_type,
        quantity,
        price: request.price,
    };
    accept::check_permit(request.permit.as_ref(), seal, &draft)?;
    let checked = accept::check_draft(book, config, &draft, now, Some(&original))?;

    book.close(index, OrderStatus::Replaced, CloseReason::Replaced, None);
    let new_index = open_order(book, &draft, &checked, None, original.deadline, now);
    let mut events = Vec::new();
    book.emit(OrderEventKind::Replaced, index, &mut events);
    book.emit(OrderEventKind::Pending, new_index, &mut events);
    book.ledger
        .orders
        .get(new_index)
        .map(|order| order.order_id.clone())
        .ok_or(BrokerError::OrderNotFound)
}

/// 주문 취소. 묶인 것을 풀고, 취소 요청에 새로 낸 식별자를 돌려준다.
pub(crate) fn cancel(
    book: &mut Book,
    order_id: &OrderId,
    now: DateTime<Utc>,
) -> Result<OrderId, BrokerError> {
    check_fault(book)?;
    let index = open_index(book, order_id)?;
    book.close(
        index,
        OrderStatus::Canceled,
        CloseReason::CancelRequested,
        Some(now),
    );
    book.emit(OrderEventKind::Canceled, index, &mut Vec::new());
    Ok(book.ledger.issue_id())
}
