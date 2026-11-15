//! 계좌의 속: 장부([`Ledger`])와 저장하지 않는 것(달력 · 기준가 · 장애 흉내 · 구독자).
//! 주문을 끝내기 · 미체결 취소 · 결제 · 이벤트 내기처럼 여러 길이 함께 쓰는 동작을 둔다.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::MarketCalendar;
use rust_decimal::Decimal;
use tokio::sync::mpsc::Sender;

use crate::calendar;
use crate::ledger::{CashCode, Ledger, Lot, MarketCode, OrderRecord};
use crate::{
    Archived, CloseReason, Fault, Fill, IDEMPOTENCY_WINDOW, OrderEvent, OrderEventKind, OrderStatus,
};

/// 종목 하나에 기억하는 기준가의 날짜 수. 오늘 것과 미리 넣은 다음 영업일 것이면 충분하다.
const REFERENCE_DAYS: usize = 3;

/// 멱등성 키가 아직 살아 있는가. 접수한 지 딱 [`IDEMPOTENCY_WINDOW`]가 되면 죽는다.
/// 시각이 접수 때보다 앞이면(시계가 어긋남) 아직 살아 있는 것으로 본다.
pub(crate) fn key_alive(accepted_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now.signed_duration_since(accepted_at)
        .to_std()
        .map_or(true, |age| age < IDEMPOTENCY_WINDOW)
}

/// 덜어 낼 주문인가: 끝났고 접수 시각이 `before`보다 앞이다.
fn archivable(order: &OrderRecord, before: DateTime<Utc>) -> bool {
    !order.is_open() && order.ordered_at < before
}

/// 계좌의 속. 잠금 안에 있다.
#[derive(Debug)]
pub(crate) struct Book {
    pub ledger: Ledger,
    pub kr_calendar: Option<MarketCalendar>,
    pub us_calendar: Option<MarketCalendar>,
    /// 종목 글자 → 그 기준가를 쓰는 영업일과 기준가(전일 종가). 날짜 순서, 최근 몇 날만.
    reference: BTreeMap<String, Vec<(NaiveDate, Decimal)>>,
    pub fault: Option<Fault>,
    /// 체결 알림을 받는 쪽들. 저마다 정한 개수까지만 쥔다.
    pub subscribers: Vec<Sender<OrderEvent>>,
}

impl Book {
    pub(crate) fn new(ledger: Ledger) -> Self {
        Self {
            ledger,
            kr_calendar: None,
            us_calendar: None,
            reference: BTreeMap::new(),
            fault: None,
            subscribers: Vec::new(),
        }
    }

    pub(crate) fn calendar(&self, market: MarketCode) -> Option<&MarketCalendar> {
        match market {
            MarketCode::Kr => self.kr_calendar.as_ref(),
            MarketCode::Us => self.us_calendar.as_ref(),
        }
    }

    /// 그 시장의 달력. 넣은 적이 없거나 `now`를 덮지 않으면 `None`(달력이 낡음).
    pub(crate) fn covering(
        &self,
        market: MarketCode,
        now: DateTime<Utc>,
    ) -> Option<&MarketCalendar> {
        self.calendar(market)
            .filter(|calendar| calendar::covers(calendar, now))
    }

    /// 종목의 기준가를 그 영업일의 것으로 기억한다. 같은 날짜의 것은 바꾼다.
    pub(crate) fn set_reference(&mut self, symbol: &str, price: Decimal, date: NaiveDate) {
        let days = self.reference.entry(symbol.to_owned()).or_default();
        days.retain(|(day, _)| *day != date);
        days.push((date, price));
        days.sort_by_key(|(day, _)| *day);
        let extra = days.len().saturating_sub(REFERENCE_DAYS);
        days.drain(..extra);
    }

    /// 그 영업일의 기준가. 다른 날짜의 것뿐이면 모르는 것이다.
    pub(crate) fn reference_on(&self, symbol: &str, date: NaiveDate) -> Option<Decimal> {
        self.reference
            .get(symbol)?
            .iter()
            .find(|(day, _)| *day == date)
            .map(|(_, price)| *price)
    }

    /// 그 주문의 지금 모습으로 이벤트를 하나 낸다. 구독자에게 보내고 `out`에도 담는다.
    pub(crate) fn emit(&mut self, kind: OrderEventKind, index: usize, out: &mut Vec<OrderEvent>) {
        let Some(order) = self.ledger.orders.get(index) else {
            return;
        };
        let event = OrderEvent {
            kind,
            order: order.view(),
        };
        // 받는 쪽이 사라졌거나 읽지 않아 쥘 수 있는 개수가 찬 구독은 끊는다(그 구독만).
        // 이미 쥔 것은 받는 쪽이 읽을 수 있고, 그 뒤로는 닫힌 것으로 보인다.
        self.subscribers
            .retain(|subscriber| subscriber.try_send(event.clone()).is_ok());
        out.push(event);
    }

    /// 주문을 끝낸다: 묶은 돈을 풀고 상태와 이유를 적는다. 파는 주문이 묶은 수량은 열린
    /// 주문의 남은 수량으로 셈하므로 상태가 바뀌면 함께 풀린다.
    pub(crate) fn close(
        &mut self,
        index: usize,
        status: OrderStatus,
        reason: CloseReason,
        canceled_at: Option<DateTime<Utc>>,
    ) {
        let Some(order) = self.ledger.orders.get_mut(index) else {
            return;
        };
        let released = order.held_cash;
        let currency = order.currency;
        order.held_cash = Decimal::ZERO;
        order.status = status;
        order.close_reason = Some(reason);
        order.canceled_at = canceled_at;
        self.ledger.cash_mut(currency).available += released;
    }

    /// `now`를 받는 모든 호출이 먼저 하는 일: 지난 멱등성 키를 버리고, 마감이 된 주문을 취소한다.
    /// 받은 `now`는 상태 값의 내보낸 시각으로 기억한다(가장 늦은 것).
    pub(crate) fn advance(&mut self, now: DateTime<Utc>, out: &mut Vec<OrderEvent>) {
        self.ledger.last_now = Some(self.ledger.last_now.map_or(now, |last| last.max(now)));
        self.ledger
            .idempotency
            .retain(|_, key| key_alive(key.accepted_at, now));
        self.expire(now, out);
    }

    /// 덜어 낼 것: 끝난 주문 가운데 접수 시각이 `before`보다 앞인 것과, 체결 기록 가운데 체결
    /// 시각이 `before`보다 앞인 것 (N1). 장부는 그대로이고 순서도 그대로다.
    pub(crate) fn pending_archive(&self, before: DateTime<Utc>) -> Archived {
        Archived {
            orders: self
                .ledger
                .orders
                .iter()
                .filter(|order| archivable(order, before))
                .map(OrderRecord::view)
                .collect(),
            fills: self
                .ledger
                .fills
                .iter()
                .filter(|fill| fill.filled_at < before)
                .map(|fill| fill.view())
                .collect(),
        }
    }

    /// 저장을 마친 것을 장부에서 지운다 (3차 규칙 7): `archived`에 든 끝난 주문(식별자로)과 체결
    /// 기록(같은 값, 든 개수만큼). 열린 주문 · 장부에 없는 것은 넘어간다.
    pub(crate) fn confirm_archived(&mut self, archived: &Archived) {
        self.ledger.orders.retain(|order| {
            order.is_open()
                || !archived
                    .orders
                    .iter()
                    .any(|gone| gone.order_id == order.order_id)
        });
        let mut saved: Vec<&Fill> = archived.fills.iter().collect();
        self.ledger.fills.retain(|fill| {
            let view = fill.view();
            match saved.iter().position(|gone| **gone == view) {
                Some(found) => {
                    saved.swap_remove(found);
                    false
                }
                None => true,
            }
        });
    }

    /// 끝난 주문 가운데 접수 시각이 `before`보다 앞인 것과, 체결 기록 가운데 체결 시각이
    /// `before`보다 앞인 것을 장부에서 떼어 돌려준다 (N1). 순서는 그대로다.
    pub(crate) fn archive(&mut self, before: DateTime<Utc>) -> Archived {
        let old = |order: &OrderRecord| archivable(order, before);
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.ledger.orders)
            .into_iter()
            .partition(old);
        self.ledger.orders = kept;
        let (done, left): (Vec<_>, Vec<_>) = std::mem::take(&mut self.ledger.fills)
            .into_iter()
            .partition(|fill| fill.filled_at < before);
        self.ledger.fills = left;
        Archived {
            orders: gone.iter().map(OrderRecord::view).collect(),
            fills: done.iter().map(|fill| fill.view()).collect(),
        }
    }

    /// 마감이 된(`now >= 마감`) 열린 주문을 접수한 순서대로 취소한다 (E12 · E29 · N4).
    /// 취소 시각은 알아챈 `now`가 아니라 그 주문의 마감 시각이다(3차 규칙 5).
    fn expire(&mut self, now: DateTime<Utc>, out: &mut Vec<OrderEvent>) {
        let due: Vec<(usize, DateTime<Utc>)> = self
            .ledger
            .orders
            .iter()
            .enumerate()
            .filter(|(_, order)| order.is_open() && now >= order.deadline)
            .map(|(index, order)| (index, order.deadline))
            .collect();
        for (index, deadline) in due {
            self.close(
                index,
                OrderStatus::Canceled,
                CloseReason::UnfilledTimeout,
                Some(deadline),
            );
            self.emit(OrderEventKind::Canceled, index, out);
        }
    }

    /// 그 시장에서 판 돈 가운데 결제 예정일이 `today` 이하인 것을 쓸 수 있는 돈으로 옮긴다.
    pub(crate) fn settle(&mut self, market: MarketCode, today: NaiveDate) {
        for code in [CashCode::Krw, CashCode::Usd] {
            let cash = self.ledger.cash_mut(code);
            let due = |lot: &Lot| lot.market == market && lot.settles_on <= today;
            let released: Decimal = cash
                .unsettled
                .iter()
                .filter(|lot| due(lot))
                .map(|lot| lot.amount)
                .sum();
            cash.unsettled.retain(|lot| !due(lot));
            cash.available += released;
        }
    }

    /// 판 돈을 결제일까지 쓸 수 없는 칸에 둔다. 같은 시장 · 같은 날의 묶음에 더한다.
    pub(crate) fn defer(
        &mut self,
        code: CashCode,
        market: MarketCode,
        settles_on: NaiveDate,
        amount: Decimal,
    ) {
        let lots = &mut self.ledger.cash_mut(code).unsettled;
        match lots
            .iter_mut()
            .find(|lot| lot.market == market && lot.settles_on == settles_on)
        {
            Some(lot) => lot.amount += amount,
            None => lots.push(Lot {
                market,
                settles_on,
                amount,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Book, key_alive};
    use crate::ledger::Ledger;
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    use rust_decimal::Decimal;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
    }

    #[test]
    fn 기준가는_넣은_날짜의_것만_주고_최근_세_날짜만_기억한다() {
        let mut book = Book::new(Ledger::new(Decimal::ZERO, Decimal::ZERO));
        let price = |value: i64| Decimal::from(value);
        book.set_reference("005930", price(70_000), date(16));
        // 다음 영업일 것을 미리 넣어도 오늘 것이 남는다.
        book.set_reference("005930", price(71_000), date(17));
        assert_eq!(book.reference_on("005930", date(16)), Some(price(70_000)));
        assert_eq!(book.reference_on("005930", date(17)), Some(price(71_000)));
        assert_eq!(book.reference_on("005930", date(18)), None);
        assert_eq!(book.reference_on("000660", date(16)), None);
        // 같은 날짜로 다시 넣으면 바꾼다.
        book.set_reference("005930", price(70_500), date(16));
        assert_eq!(book.reference_on("005930", date(16)), Some(price(70_500)));
        // 날이 가면 오래된 날짜의 것부터 버린다.
        book.set_reference("005930", price(72_000), date(18));
        book.set_reference("005930", price(73_000), date(19));
        assert_eq!(book.reference_on("005930", date(16)), None);
        assert_eq!(book.reference_on("005930", date(17)), Some(price(71_000)));
        assert_eq!(book.reference_on("005930", date(19)), Some(price(73_000)));
    }

    #[test]
    fn 멱등성_키는_딱_열_분이_되면_죽는다() {
        assert!(key_alive(at(0), at(599)));
        assert!(!key_alive(at(0), at(600)));
        assert!(
            key_alive(at(0), at(-5)),
            "시계가 뒤로 갔으면 살아 있는 것으로 본다"
        );
    }
}
