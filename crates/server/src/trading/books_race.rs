//! (13차 1번) 체결이 도는 동안 장부를 여러 번 셈해도 섞인 평가액이 나오지 않는다.
//!
//! 장부 셈([`Trading::books`])이 보유와 현금을 따로 읽으면, 그 사이에 든 체결 때문에 "산 체결 전 보유 + 산 뒤
//! 현금"(평가액이 낮게 보여 거짓 하루 한도 · 최대 손실 멈춤) 또는 "판 체결 전 보유 + 판 뒤 현금"(최고 평가액이
//! 부풀어 저장)이 나온다. 하루 한도 · 최대 손실 판단은 이 장부의 평가액 · 그날 시작 평가액 · 최고 평가액으로만
//! 하므로, 섞인 값이 한 번도 나오지 않으면 섞인 평가액으로 판단이 나지 않는다.
//!
//! 수수료 · 세금을 0으로, 판 돈을 바로 쓰게 둔 모의계좌에서 같은 값(지정가 = 시세 = 평균 단가)으로 사고팔기를
//! 되풀이한다. 그러면 체결 전후 어느 때든 원화 평가액은 처음 현금 그대로다. 공개 길로는 하루 매매 횟수 상한 ·
//! 규칙 점검 때문에 체결을 충분히 많이 일으킬 수 없어, 주문 길 안쪽(점검 통과 표시를 내는 권한)으로 모의계좌에
//! 바로 주문을 낸다. 부하 없이도 거의 반드시 섞이게, 체결(수천 번)과 장부 셈을 여러 작업에서 겹친다.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_ai::AnthropicConfig;
use pi_market::{
    Clock, Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol,
    TossConfig, TradingDay,
};
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, InitialCash, LimitFillPrice, MarketRules, MaxFill,
    MoneyRounding, OrderRequest, OrderType, PaperConfig, ProceedsUsable, RoundingMode,
    SessionRules, Settlement, Side, TickBand, UnfilledTimeout,
};
use pi_rules::RulesConfig;
use rust_decimal::Decimal;

use crate::{App, CalendarFn, Settings, Trading, TradingWiring, Upstreams, Wiring};

/// 사고파는 종목.
const SYMBOL: &str = "000660";
/// 지정가 = 시세(원).
const PRICE: i64 = 100_000;
/// 한 번에 사고파는 수량(평가액의 10%라 섞이면 크게 어긋난다).
const QUANTITY: i64 = 10;
/// 처음 원화 현금.
const CASH: i64 = 10_000_000;
/// 사고팔기를 되풀이하는 횟수(체결은 그 두 배).
const ROUNDS: usize = 2_000;
/// 장부를 셈하는 작업 수.
const COUNTERS: usize = 3;

struct TestClock(AtomicI64);

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0.load(Ordering::SeqCst))
            .unwrap()
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration.min(Duration::from_millis(10))).await;
    }
}

fn kst(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    let offset = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    offset
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

fn day(date: NaiveDate) -> TradingDay {
    TradingDay {
        date,
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start: kst(date, 9, 0),
            end: kst(date, 15, 30),
        }],
    }
}

/// 2026-11-16 월요일 하나만 보는 달력(앞뒤 영업일은 금요일 · 화요일).
fn calendar(date: NaiveDate) -> MarketCalendar {
    let shift = |days: i64| date + chrono::Duration::days(days);
    MarketCalendar {
        market: Market::Kr,
        today: day(date),
        previous_business_day: day(shift(-3)),
        next_business_day: day(shift(1)),
    }
}

/// 수수료 · 세금 0, 판 돈을 바로 쓰는 시장 규칙.
fn rules(currency: Currency, scale: u32, tick: &str) -> MarketRules {
    MarketRules {
        currency,
        commission_rate: Decimal::ZERO,
        sell_tax_rate: Decimal::ZERO,
        rounding: MoneyRounding {
            scale,
            mode: RoundingMode::Down,
        },
        tick_table: vec![TickBand {
            below: None,
            tick: tick.parse().unwrap(),
        }],
        price_limit_rate: None,
        settlement: Settlement {
            business_days: 2,
            proceeds_usable: ProceedsUsable::Immediately,
        },
        market_slippage_rate: Decimal::ZERO,
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

fn trading_wiring() -> TradingWiring {
    let calendar: CalendarFn = Arc::new(calendar);
    TradingWiring {
        paper: PaperConfig {
            initial_cash: InitialCash {
                krw: Decimal::from(CASH),
                usd: Decimal::from(10_000),
            },
            kr: rules(Currency::Krw, 0, "100"),
            us: rules(Currency::Usd, 2, "0.01"),
            stale: StalePolicy {
                max_age: Duration::from_secs(30),
            },
            event_buffer: 64,
        },
        rules: RulesConfig {
            banned_symbols: Vec::new(),
            sectors: HashMap::new(),
        },
        calendar: Some(calendar),
    }
}

/// 아무도 듣지 않는 로컬 주소(바깥으로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

async fn start(home: &std::path::Path, at: DateTime<Utc>) -> App {
    let home = home.to_str().unwrap();
    let args: Vec<String> = ["--home", home, "--port", "0", "--interval-ms", "20"]
        .iter()
        .chain(["--symbols", "005930", "--demo"].iter())
        .map(|s| (*s).to_string())
        .collect();
    let settings = Settings::parse(&args, &|_| None, None).expect("설정");
    let dead = dead_base_url();
    let wiring = Wiring {
        upstreams: Upstreams {
            toss: TossConfig {
                base_url: dead.clone(),
                timeout: Duration::from_secs(3),
                token_failure_cooldown: Duration::ZERO,
                ..TossConfig::default()
            },
            anthropic: AnthropicConfig {
                base_url: dead,
                timeout: Duration::from_secs(3),
                max_retries: 0,
                retry_base_delay: Duration::from_millis(1),
            },
        },
        clock: Arc::new(TestClock(AtomicI64::new(at.timestamp_millis()))),
        peer_override: None,
        cli_locations: Vec::new(),
    };
    let start = App::start_with(settings, wiring, trading_wiring());
    tokio::time::timeout(Duration::from_secs(10), start)
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

fn quote(at: DateTime<Utc>) -> Quote {
    Quote {
        symbol: Symbol::new(SYMBOL),
        price: Decimal::from(PRICE),
        currency: Currency::Krw,
        exchange_time: Some(at),
        received_at: at,
    }
}

/// 같은 값으로 사거나 파는 지정가 주문을 `at`에 내고, 1밀리초 뒤에 받은 그 값의 시세로 체결시킨다.
async fn fill(trading: &Trading, side: Side, at: DateTime<Utc>) {
    let approved = ApprovedOrder {
        market: Market::Kr,
        symbol: Symbol::new(SYMBOL),
        side,
        order_type: OrderType::Limit,
        quantity: Decimal::from(QUANTITY),
        price: Some(Decimal::from(PRICE)),
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
        account_kind: approved.account_kind,
        timeout: UnfilledTimeout::SessionClose,
        confirm_high_value: false,
        permit: Some(trading.authority.approve(approved)),
    };
    let paper = trading.paper();
    Broker::place_order(paper, request, at)
        .await
        .expect("모의계좌가 주문을 받아야 한다");
    let later = at + chrono::Duration::milliseconds(1);
    let events = paper.on_quote(&quote(later), later);
    assert!(!events.is_empty(), "같은 값의 시세가 오면 체결된다");
}

/// 장부를 셈할 때마다 원화 장부의 (평가액, 그날 시작 평가액, 최고 평가액)이 처음 현금 그대로인지 본다.
/// 어긋난 셈의 값들을 돌려준다.
async fn count_books(
    trading: Arc<Trading>,
    at: DateTime<Utc>,
    done: Arc<AtomicBool>,
) -> Vec<String> {
    let cash = Decimal::from(CASH);
    let mut mixed = Vec::new();
    while !done.load(Ordering::SeqCst) {
        let books = trading.books(at).await.expect("장부를 셈해야 한다");
        let krw = books
            .iter()
            .find(|book| book.currency == Currency::Krw)
            .expect("원화 장부");
        if (krw.equity, krw.day_start_equity, krw.peak_equity) != (cash, cash, cash) {
            mixed.push(format!(
                "평가액 {} · 그날 시작 {} · 최고 {}",
                krw.equity, krw.day_start_equity, krw.peak_equity
            ));
        }
        tokio::task::yield_now().await;
    }
    mixed
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 체결이_도는_동안_장부를_여러_번_셈해도_섞인_평가액으로_한도_판단이_나지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let at = kst(NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(), 10, 0);
    let app = start(dir.path(), at).await;
    let trading = app.trading();
    // 그 종목의 시세를 주문 길로 한 번 받아 둔다(평가 값 = 지정가, 모의계좌가 그날 달력을 안다).
    trading
        .on_quote(quote(at))
        .await
        .expect("시세를 받아야 한다");

    let done = Arc::new(AtomicBool::new(false));
    let counters: Vec<_> = (0..COUNTERS)
        .map(|_| tokio::spawn(count_books(Arc::clone(&trading), at, Arc::clone(&done))))
        .collect();
    let step = |n: usize| at + chrono::Duration::milliseconds(i64::try_from(n).unwrap() * 2 + 2);
    for round in 0..ROUNDS {
        fill(&trading, Side::Buy, step(round * 2)).await;
        fill(&trading, Side::Sell, step(round * 2 + 1)).await;
    }
    done.store(true, Ordering::SeqCst);
    let mut mixed = Vec::new();
    for counter in counters {
        mixed.extend(counter.await.expect("장부 셈 작업"));
    }
    let after = trading.books(at).await.expect("장부");
    let krw = after.iter().find(|b| b.currency == Currency::Krw).unwrap();
    assert_eq!(
        krw.equity,
        Decimal::from(CASH),
        "다 팔고 나면 처음 현금 그대로다"
    );
    assert!(
        mixed.is_empty(),
        "체결과 겹친 장부 셈이 섞인 값을 냈다({}번): {:?}",
        mixed.len(),
        mixed.iter().take(3).collect::<Vec<_>>()
    );
    tokio::time::timeout(Duration::from_secs(10), app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}
