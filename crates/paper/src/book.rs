//! 계좌의 속: 장부([`Ledger`])와 저장하지 않는 것(달력 · 기준가 · 장애 흉내 · 구독자).
//! 주문을 끝내기 · 미체결 취소 · 결제 · 이벤트 내기처럼 여러 길이 함께 쓰는 동작을 둔다.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc, Weekday};
use pi_market::MarketCalendar;
use rust_decimal::Decimal;
use tokio::sync::mpsc::UnboundedSender;

use crate::ledger::{CashCode, Ledger, Lot, MarketCode};
use crate::{CloseReason, Fault, OrderEvent, OrderEventKind, OrderStatus};

/// 계좌의 속. 잠금 안에 있다.
#[derive(Debug)]
pub(crate) struct Book {
    pub ledger: Ledger,
    pub kr_calendar: Option<MarketCalendar>,
    pub us_calendar: Option<MarketCalendar>,
    /// 종목 글자 → 기준가(전일 종가).
    pub reference: BTreeMap<String, Decimal>,
    pub fault: Option<Fault>,
    pub subscribers: Vec<UnboundedSender<OrderEvent>>,
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

    /// 그 주문의 지금 모습으로 이벤트를 하나 낸다. 구독자에게 보내고 `out`에도 담는다.
    pub(crate) fn emit(&mut self, kind: OrderEventKind, index: usize, out: &mut Vec<OrderEvent>) {
        let Some(order) = self.ledger.orders.get(index) else {
            return;
        };
        let event = OrderEvent {
            kind,
            order: order.view(),
        };
        // 받는 쪽이 사라진 구독은 뺀다.
        self.subscribers
            .retain(|subscriber| subscriber.send(event.clone()).is_ok());
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

    /// 마감이 된(`now >= 마감`) 열린 주문을 접수한 순서대로 취소한다 (E12 · E29 · N4).
    pub(crate) fn expire(&mut self, now: DateTime<Utc>, out: &mut Vec<OrderEvent>) {
        let due: Vec<usize> = self
            .ledger
            .orders
            .iter()
            .enumerate()
            .filter(|(_, order)| order.is_open() && now >= order.deadline)
            .map(|(index, _)| index)
            .collect();
        for index in due {
            self.close(
                index,
                OrderStatus::Canceled,
                CloseReason::UnfilledTimeout,
                Some(now),
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

/// 체결한 영업일: 달력에서 `now`가 든 세션의 날.
fn trade_date(calendar: &MarketCalendar, now: DateTime<Utc>) -> Option<NaiveDate> {
    [
        &calendar.previous_business_day,
        &calendar.today,
        &calendar.next_business_day,
    ]
    .into_iter()
    .find(|day| {
        day.sessions
            .iter()
            .any(|session| session.start <= now && now < session.end)
    })
    .map(|day| day.date)
}

/// 결제 예정일 = 체결한 영업일 + `business_days` 영업일. 영업일은 평일(월~금)로만 센다.
pub(crate) fn settlement_date(
    calendar: &MarketCalendar,
    now: DateTime<Utc>,
    business_days: u32,
) -> Option<NaiveDate> {
    let mut date = trade_date(calendar, now)?;
    let mut left = business_days;
    while left > 0 {
        date = date.succ_opt()?;
        if !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
            left -= 1;
        }
    }
    Some(date)
}
