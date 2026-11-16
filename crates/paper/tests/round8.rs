//! 8차 작업(후보 일괄 처리, `docs/dev/modules/paper.md` "다음 차수 후보 (exec 3차에서 넘어옴)").
//!
//! 점검 통과 표시를 종목 · 방향만이 아니라 매매 번호에 묶는다(E33). 표시를 내는 권한
//! ([`CheckAuthority`])이 매매 번호를 받아 표시에 남기고, 주문을 내는 쪽(exec)이
//! [`CheckPermit::trade`]로 그 매매의 표시인지 대조한다. 증권사(모의계좌)는 매매를 모르므로
//! 묶인 표시도 묶이지 않은 표시와 같이 받는다(계좌 표식 · 내용 대조는 그대로).
//!
//! 이 파일은 기존 묶음(`suite.rs`)과 따로 도는 테스트 실행 파일이라 도우미를 안에 둔다.

use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{
    Currency, Market, MarketCalendar, Session, SessionKind, StalePolicy, Symbol, TradingDay,
};
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, BrokerError, CheckPermit, InitialCash, LimitFillPrice,
    MarketRules, MaxFill, MoneyRounding, OrderAck, OrderRequest, OrderType, PaperBroker,
    PaperConfig, ProceedsUsable, RoundingMode, SessionRules, Settlement, Side, TickBand,
    UnfilledTimeout,
};
use rust_decimal::Decimal;
use tokio::runtime::Builder;

/// 삼성전자. 기준가 70,000원(호가 단위 100원).
const SAM: &str = "005930";

fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// 기준 영업일 2026-11-16 월요일.
fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
}

fn utc(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
}

/// 정규장(09:00~15:30 KST = 00:00~06:30 UTC)만 있는 날.
fn kr_day(date: NaiveDate) -> TradingDay {
    TradingDay {
        date,
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start: utc(date, 0, 0),
            end: utc(date, 6, 30),
        }],
    }
}

fn rules(currency: Currency, scale: u32, ticks: Vec<TickBand>) -> MarketRules {
    MarketRules {
        currency,
        commission_rate: d("0.00015"),
        sell_tax_rate: d("0.0020"),
        rounding: MoneyRounding {
            scale,
            mode: RoundingMode::Down,
        },
        tick_table: ticks,
        price_limit_rate: None,
        settlement: Settlement {
            business_days: 2,
            proceeds_usable: ProceedsUsable::AfterSettlement,
        },
        market_slippage_rate: d("0.001"),
        limit_fill_price: LimitFillPrice::Limit,
        max_fill: MaxFill::Unlimited,
        sessions: SessionRules {
            limit: vec![SessionKind::Regular],
            market: vec![SessionKind::Regular],
        },
        high_value_confirm_from: None,
        max_order_amount: None,
    }
}

/// 원화 10,000,000 · 달러 10,000. 기본값에 기대지 않게 숫자를 모두 적는다.
fn config() -> PaperConfig {
    let band = |below: Option<&str>, tick: &str| TickBand {
        below: below.map(d),
        tick: d(tick),
    };
    PaperConfig {
        initial_cash: InitialCash {
            krw: d("10000000"),
            usd: d("10000"),
        },
        kr: rules(
            Currency::Krw,
            0,
            vec![band(Some("200000"), "100"), band(None, "1000")],
        ),
        us: rules(Currency::Usd, 2, vec![band(None, "0.01")]),
        stale: StalePolicy {
            max_age: Duration::from_secs(30),
        },
        event_buffer: 64,
    }
}

/// 달력과 기준가를 넣은 새 계좌.
fn open() -> (PaperBroker, pi_paper::CheckAuthority) {
    let (broker, authority) = PaperBroker::open(config()).unwrap();
    broker.set_calendar(MarketCalendar {
        market: Market::Kr,
        today: kr_day(monday()),
        previous_business_day: kr_day(NaiveDate::from_ymd_opt(2026, 11, 13).unwrap()),
        next_business_day: kr_day(NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()),
    });
    broker.set_reference_price(Symbol::new(SAM), d("70000"), monday());
    (broker, authority)
}

/// 삼성전자 10주를 69,000원에 사는 지정가 주문 내용.
fn buy() -> ApprovedOrder {
    ApprovedOrder {
        market: Market::Kr,
        symbol: Symbol::new(SAM),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: d("10"),
        price: Some(d("69000")),
        account_kind: AccountKind::Paper,
    }
}

/// 그 내용으로 표시를 붙여 정규장 안(10:00 KST)에 낸다.
fn place(broker: &PaperBroker, permit: CheckPermit) -> Result<OrderAck, BrokerError> {
    let order = buy();
    let request = OrderRequest {
        client_order_id: None,
        market: order.market,
        symbol: order.symbol,
        side: order.side,
        order_type: order.order_type,
        quantity: order.quantity,
        price: order.price,
        account_kind: AccountKind::Paper,
        timeout: UnfilledTimeout::LONG_TERM,
        confirm_high_value: false,
        permit: Some(permit),
    };
    let runtime = Builder::new_current_thread().build().unwrap();
    let broker: &dyn Broker = broker;
    runtime.block_on(broker.place_order(request, utc(monday(), 1, 0)))
}

// 검증: E33
#[test]
fn trade_bound_permit_carries_its_trade_number() {
    let (_broker, authority) = open();
    let permit = authority.approve_for_trade(7, buy());
    assert_eq!(permit.trade(), Some(7));
    assert_eq!(permit.approved(), &buy());
}

// 검증: E33
#[test]
fn plain_permit_has_no_trade_number() {
    let (_broker, authority) = open();
    let permit = authority.approve(buy());
    assert_eq!(permit.trade(), None);
}

// 검증: E33
#[test]
fn permits_for_different_trades_keep_their_own_numbers() {
    let (_broker, authority) = open();
    let first = authority.approve_for_trade(1, buy());
    let second = authority.approve_for_trade(2, buy());
    assert_eq!(first.approved(), second.approved());
    assert_eq!(first.trade(), Some(1));
    assert_eq!(second.trade(), Some(2));
}

// 검증: E33
#[test]
fn broker_accepts_a_trade_bound_permit_like_a_plain_one() {
    let (broker, authority) = open();
    let ack = place(&broker, authority.approve_for_trade(7, buy()));
    assert!(ack.is_ok(), "묶인 표시도 받는다: {ack:?}");
}

// 검증: E33
#[test]
fn trade_bound_permit_from_another_account_is_still_rejected() {
    let (broker, _authority) = open();
    let (_other, other_authority) = open();
    let placed = place(&broker, other_authority.approve_for_trade(7, buy()));
    assert_eq!(placed.unwrap_err(), BrokerError::CheckInvalid);
}
