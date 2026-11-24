//! 10차 작업(승인 증표 표시, `docs/dev/modules/paper.md` "10차 작업", 결정 14).
//!
//! `CheckPermit::granted()`가 승인 증표로 낸 표시인지를 담는다. `approve` · `approve_for_trade`는
//! 거짓, `approve_granted_for_trade`는 참이고 매매에 묶인다. 모의계좌는 세 표시 모두로 주문을 받는다.
//!
//! 이 파일은 기존 묶음(`suite.rs`)과 따로 도는 테스트 실행 파일이라 도우미를 안에 둔다.

use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{
    Currency, Market, MarketCalendar, Session, SessionKind, StalePolicy, Symbol, TradingDay,
};
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, CheckAuthority, InitialCash, LimitFillPrice, MarketRules,
    MaxFill, MoneyRounding, OrderRequest, OrderType, PaperBroker, PaperConfig, ProceedsUsable,
    RoundingMode, SessionRules, Settlement, Side, TickBand, UnfilledTimeout,
};
use rust_decimal::Decimal;
use tokio::runtime::{Builder, Runtime};

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

/// 정규장 안(10:00 KST = 01:00 UTC)에서 `seconds`초 뒤.
fn at(seconds: i64) -> DateTime<Utc> {
    utc(monday(), 1, 0) + chrono::Duration::seconds(seconds)
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

/// 수수료 0.015%(원 미만 버림), 판 세금 0.2%, 지정가 체결은 지정가로.
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
fn open() -> (PaperBroker, CheckAuthority) {
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

fn runtime() -> Runtime {
    Builder::new_current_thread().build().unwrap()
}

/// 삼성전자 1주를 70,000원에 사는 지정가 승인 내용.
fn approved() -> ApprovedOrder {
    ApprovedOrder {
        market: Market::Kr,
        symbol: Symbol::new(SAM),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: d("1"),
        price: Some(d("70000")),
        account_kind: AccountKind::Paper,
    }
}

/// `approved()`와 같은 내용의 주문 요청에 `permit`을 달아 낸다. 결과는 호출한 쪽이 본다.
fn place_with(
    broker: &PaperBroker,
    rt: &Runtime,
    permit: pi_paper::CheckPermit,
) -> Result<pi_paper::OrderAck, pi_paper::BrokerError> {
    let a = approved();
    let request = OrderRequest {
        client_order_id: None,
        market: a.market,
        symbol: a.symbol,
        side: a.side,
        order_type: a.order_type,
        quantity: a.quantity,
        price: a.price,
        account_kind: AccountKind::Paper,
        timeout: UnfilledTimeout::LONG_TERM,
        confirm_high_value: false,
        permit: Some(permit),
    };
    let broker: &dyn Broker = broker;
    rt.block_on(broker.place_order(request, at(0)))
}

// 검증: E33
#[test]
fn 승인으로_낸_표시는_granted가_참이고_묶인_매매_번호를_돌려준다() {
    let (_broker, authority) = open();
    let permit = authority.approve_granted_for_trade(42, approved());
    assert!(permit.granted(), "승인 증표 표시는 granted가 참");
    assert_eq!(permit.trade(), Some(42), "매매 번호에 묶인다");
    assert_eq!(permit.approved(), &approved(), "승인한 내용이 그대로");
}

// 검증: E33
#[test]
fn 그냥_낸_표시는_granted가_거짓이고_매매에_묶이지_않는다() {
    let (_broker, authority) = open();
    let permit = authority.approve(approved());
    assert!(!permit.granted(), "approve는 granted가 거짓");
    assert_eq!(permit.trade(), None);
}

// 검증: E33
#[test]
fn 매매에_묶어_낸_표시도_granted가_거짓이다() {
    let (_broker, authority) = open();
    let permit = authority.approve_for_trade(7, approved());
    assert!(
        !permit.granted(),
        "approve_for_trade는 granted가 거짓(지금 동작 그대로)"
    );
    assert_eq!(permit.trade(), Some(7));
}

// 검증: E33
#[test]
fn 승인_표시의_매매_번호는_넘긴_번호_그대로다() {
    let (_broker, authority) = open();
    let first = authority.approve_granted_for_trade(1, approved());
    let second = authority.approve_granted_for_trade(9_000_000_000, approved());
    assert_eq!(first.trade(), Some(1));
    assert_eq!(second.trade(), Some(9_000_000_000));
    assert!(first.granted() && second.granted());
}

// 검증: E21 E22 E33
#[test]
fn 모의계좌는_세_표시_모두로_주문을_받는다() {
    let rt = runtime();
    for kind in ["approve", "approve_for_trade", "approve_granted_for_trade"] {
        let (broker, authority) = open();
        let permit = match kind {
            "approve" => authority.approve(approved()),
            "approve_for_trade" => authority.approve_for_trade(5, approved()),
            _ => authority.approve_granted_for_trade(5, approved()),
        };
        let expected_granted = kind == "approve_granted_for_trade";
        assert_eq!(permit.granted(), expected_granted, "{kind}의 granted");
        place_with(&broker, &rt, permit)
            .unwrap_or_else(|error| panic!("{kind} 표시로 낸 주문은 받아야 한다: {error:?}"));
    }
}

// 검증: E33
#[test]
fn 승인_표시도_내용이_다르면_지금처럼_거절한다() {
    let (broker, authority) = open();
    let rt = runtime();
    let mut other = approved();
    other.quantity = d("2");
    let permit = authority.approve_granted_for_trade(5, other);
    assert!(permit.granted());
    assert!(
        place_with(&broker, &rt, permit).is_err(),
        "granted와 무관하게 내용 대조는 그대로여야 한다"
    );
}

// 검증: E33
#[test]
fn 승인_표시도_다른_계좌의_것이면_거절한다() {
    let (broker, _authority) = open();
    let (_other_broker, other_authority) = open();
    let rt = runtime();
    let permit = other_authority.approve_granted_for_trade(5, approved());
    assert!(permit.granted());
    assert!(
        place_with(&broker, &rt, permit).is_err(),
        "granted와 무관하게 계좌 표식 대조는 그대로여야 한다"
    );
}
