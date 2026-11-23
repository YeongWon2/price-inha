//! 9차 작업(장부 한 번에 읽기, `docs/dev/modules/paper.md` "9차 작업").
//!
//! 보유 전체와 통화별 현금을 같은 잠금 안에서 함께 읽는 [`PaperBroker::account_snapshot`]을 본다.
//! 한 스레드가 `on_quote`로 체결을 계속 일으키는 동안 다른 스레드가 여러 번 읽어도, 매번
//! "현금 + 보유 수량 × 체결가"가 체결 전후의 일관된 값 가운데 하나여야 한다(섞인 값이 없다).
//!
//! 이 파일은 기존 묶음(`suite.rs`)과 따로 도는 테스트 실행 파일이라 도우미를 안에 둔다.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{
    Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol, TradingDay,
};
use pi_paper::{
    AccountKind, AccountSnapshot, ApprovedOrder, Broker, CheckAuthority, InitialCash,
    LimitFillPrice, MarketRules, MaxFill, MoneyRounding, OrderRequest, OrderType, PaperBroker,
    PaperConfig, ProceedsUsable, RoundingMode, SessionRules, Settlement, Side, TickBand,
    UnfilledTimeout,
};
use rust_decimal::Decimal;
use tokio::runtime::{Builder, Runtime};

/// 삼성전자. 기준가 70,000원(호가 단위 100원).
const SAM: &str = "005930";

/// 체결을 일으키는 횟수. 한 번에 1주씩 70,000원에 산다(묶는 돈 70,011원 × 140 < 처음 돈).
const FILLS: i64 = 140;

/// 체결이 일어나는 동안 장부를 함께 읽는 스레드 수.
const READERS: usize = 4;

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

/// 삼성전자 1주를 70,000원에 사는 지정가 주문을 `now`에 낸다(받아야 한다).
fn place_buy(broker: &PaperBroker, authority: &CheckAuthority, rt: &Runtime, now: DateTime<Utc>) {
    let approved = ApprovedOrder {
        market: Market::Kr,
        symbol: Symbol::new(SAM),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: d("1"),
        price: Some(d("70000")),
        account_kind: AccountKind::Paper,
    };
    let request = OrderRequest {
        client_order_id: None,
        market: approved.market,
        symbol: approved.symbol.clone(),
        side: approved.side,
        order_type: approved.order_type,
        quantity: approved.quantity,
        price: approved.price,
        account_kind: AccountKind::Paper,
        timeout: UnfilledTimeout::LONG_TERM,
        confirm_high_value: false,
        permit: Some(authority.approve(approved)),
    };
    let broker: &dyn Broker = broker;
    rt.block_on(broker.place_order(request, now))
        .expect("1주 사는 지정가 주문은 받아야 한다");
}

/// 70,000원 시세. 거래소 시각 · 받은 시각이 모두 `now`다(그때 막 체결된 값).
fn quote(now: DateTime<Utc>) -> Quote {
    Quote {
        symbol: Symbol::new(SAM),
        price: d("70000"),
        currency: Currency::Krw,
        exchange_time: Some(now),
        received_at: now,
    }
}

/// 한 번에 읽은 장부의 원화 평가액: 원화 현금(세 칸 합) + 삼성전자 보유 수량 × 70,000원.
/// 삼성전자 보유 수량도 함께 돌려준다.
fn valuation(snapshot: &AccountSnapshot) -> (Decimal, Decimal) {
    let quantity: Decimal = snapshot
        .holdings
        .iter()
        .filter(|holding| holding.symbol == Symbol::new(SAM))
        .map(|holding| holding.quantity)
        .sum();
    (snapshot.krw.total() + quantity * d("70000"), quantity)
}

/// k주를 산 뒤의 일관된 평가액: 한 번 살 때마다 수수료 버림(70,000 × 0.00015 = 10.5 → 10원)만큼
/// 준다. 묶인 돈(held)도 현금에 들어 있으니 접수 뒤 · 체결 전에도 값이 같다.
fn consistent(bought: Decimal) -> Decimal {
    d("10000000") - bought * d("10")
}

// 검증: C10
#[test]
fn account_snapshot_matches_separate_reads_when_nothing_moves() {
    let (broker, authority) = open();
    let rt = runtime();
    place_buy(&broker, &authority, &rt, at(0));
    broker.on_quote(&quote(at(1)), at(1));

    let now = at(2);
    let snapshot = broker.account_snapshot(now);
    let as_broker: &dyn Broker = &broker;
    let holdings = rt.block_on(as_broker.holdings(None, now)).unwrap();

    assert_eq!(snapshot.holdings, holdings, "보유는 따로 읽은 것과 같다");
    assert_eq!(snapshot.krw, broker.cash(&Currency::Krw, now), "원화 현금");
    assert_eq!(snapshot.usd, broker.cash(&Currency::Usd, now), "달러 현금");
    assert_eq!(valuation(&snapshot), (consistent(d("1")), d("1")));
}

// 검증: C10
#[test]
fn account_snapshot_never_mixes_holdings_and_cash_while_fills_happen() {
    let (broker, authority) = open();
    let done = AtomicBool::new(false);
    // 쓰는 쪽이 마지막으로 쓴 시각(정규장 시작 뒤 초). 읽는 쪽 `now`가 이것을 따라가
    // 마감 취소 경로도 체결과 함께 돈다.
    let clock = AtomicI64::new(0);

    std::thread::scope(|scope| {
        let writer = scope.spawn(|| {
            let rt = runtime();
            for i in 0..FILLS {
                clock.store(2 * i, Ordering::SeqCst);
                place_buy(&broker, &authority, &rt, at(2 * i));
                clock.store(2 * i + 1, Ordering::SeqCst);
                let filled = at(2 * i + 1);
                broker.on_quote(&quote(filled), filled);
            }
            done.store(true, Ordering::SeqCst);
        });

        let readers: Vec<_> = (0..READERS)
            .map(|reader| {
                let (broker, done, clock) = (&broker, &done, &clock);
                scope.spawn(move || {
                    let mut reads = 0_u64;
                    loop {
                        let finished = done.load(Ordering::SeqCst);
                        let now = at(clock.load(Ordering::SeqCst));
                        let snapshot = broker.account_snapshot(now);
                        let (value, quantity) = valuation(&snapshot);
                        assert!(
                            quantity >= Decimal::ZERO && quantity <= Decimal::from(FILLS),
                            "읽는 스레드 {reader}: 보유 수량은 0 ~ {FILLS}주: {quantity} \
                             ({snapshot:?})"
                        );
                        assert_eq!(
                            value,
                            consistent(quantity),
                            "읽는 스레드 {reader}의 {reads}번째 읽기({now}): 보유 {quantity}주와 \
                             현금이 섞이지 않은 평가액이어야 한다 ({snapshot:?})"
                        );
                        reads += 1;
                        if finished {
                            break;
                        }
                    }
                    reads
                })
            })
            .collect();

        writer
            .join()
            .expect("체결을 일으키는 스레드가 멈추지 않아야 한다");
        for reader in readers {
            let reads = reader
                .join()
                .expect("읽는 스레드의 확인이 모두 맞아야 한다");
            assert!(reads >= 1, "읽는 스레드마다 적어도 한 번은 읽어야 한다");
        }
    });

    let last = broker.account_snapshot(at(2 * FILLS));
    let fills = Decimal::from(FILLS);
    assert_eq!(
        valuation(&last),
        (consistent(fills), fills),
        "모두 체결된 뒤"
    );
}
