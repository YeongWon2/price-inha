//! 4차 테스트 도우미. 시세 · 달력은 `pi-market`의 `FakeAdapter`가 준 값을 쓴다.
//! 시각은 모두 테스트가 정한다(시계를 읽지 않는다).

use std::collections::HashMap;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use pi_market::{
    Currency, FakeAdapter, Market, MarketAdapter, MarketCalendar, Quote, Session, SessionKind,
    StalePolicy, Symbol, TradingDay,
};
use pi_paper::{
    AccountKind, ApprovedOrder, MarketRules, MaxFill, OrderType, ProceedsUsable, RoundingMode,
    Side, UnverifiedRules,
};
use pi_rules::{
    ApiBudget, Blocked, Book, CheckContext, Funding, Health, OrderDraft, Outcome, Passed, Position,
    ProductKind, Purpose, Rationale, Reason, RulesConfig, SourceStamp, SymbolStatus, TeamStatus,
    TradeStage, Violation, check,
};
use pi_template::{RiskParams, Style, preset};
use rust_decimal::Decimal;

pub fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}

pub fn t(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

/// 국내 장중(10:00 KST). 정규장은 00:00~06:30 UTC.
pub const KR_NOW: &str = "2026-11-16T01:00:00Z";

pub const SAMSUNG: &str = "005930";
pub const HYNIX: &str = "000660";

/// 바로 끝나는 미래(가짜 어댑터)를 끝까지 돌린다.
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("가짜 어댑터는 바로 끝나야 한다"),
    }
}

/// 국내 규칙: 수수료 0.015% · 미끄러짐 0.1% · 거래세 0.20% · 가격제한 30%.
fn kr_rules() -> MarketRules {
    MarketRules::kr(UnverifiedRules {
        commission_rate: d("0.00015"),
        market_slippage_rate: d("0.001"),
        rounding_mode: RoundingMode::Down,
        max_fill: MaxFill::Unlimited,
        proceeds_usable: ProceedsUsable::Immediately,
    })
}

fn day(date: &str) -> TradingDay {
    let date: NaiveDate = date.parse().unwrap();
    TradingDay {
        date,
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start: t(&format!("{date}T00:00:00Z")),
            end: t(&format!("{date}T06:30:00Z")),
        }],
    }
}

/// 가짜 어댑터에 국내 달력(11-13 · 11-16 · 11-17)과 시세를 정하고, 어댑터가 돌려준 값을 받는다.
pub fn kr_fake(
    symbol: &str,
    price: Decimal,
    received_at: DateTime<Utc>,
) -> (Quote, MarketCalendar) {
    let fake = FakeAdapter::new(Market::Kr);
    fake.set_calendar(MarketCalendar {
        market: Market::Kr,
        today: day("2026-11-16"),
        previous_business_day: day("2026-11-13"),
        next_business_day: day("2026-11-17"),
    });
    fake.set_quote(Quote {
        symbol: Symbol::new(symbol),
        price,
        currency: Currency::Krw,
        exchange_time: Some(received_at),
        received_at,
    });
    let quotes = ready(fake.quotes(&[Symbol::new(symbol)])).unwrap();
    let calendar = ready(fake.calendar("2026-11-16".parse().unwrap())).unwrap();
    (quotes.into_iter().next().unwrap(), calendar)
}

/// 시험용 템플릿 값: R 1% · 종목 20% · 업종 40% · 하루 손실 3% · 최대 낙폭 15% · 자동 주문 20% ·
/// 하루 5회.
fn params() -> RiskParams {
    let mut p = preset(Style::Swing);
    p.risk_per_trade_pct = d("1");
    p.symbol_weight_pct = d("20");
    p.conviction_weight_pct = None;
    p.sector_weight_pct = d("40");
    p.daily_loss_limit_pct = d("3");
    p.max_drawdown_pct = d("15");
    p.auto_order_limit_pct = d("20");
    p.max_trades_per_day = 5;
    p.chase_limit_pct = None;
    p
}

/// 점검 하나에 필요한 값 전부.
pub struct Scenario {
    pub now: DateTime<Utc>,
    pub params: RiskParams,
    pub config: RulesConfig,
    pub rules: MarketRules,
    pub calendar: MarketCalendar,
    pub quote: Quote,
    pub stale: StalePolicy,
    pub reference_price: Decimal,
    pub status: SymbolStatus,
    pub team: TeamStatus,
    pub book: Book,
    pub health: Health,
    pub open_orders: Vec<ApprovedOrder>,
    pub trades_today: u32,
    pub api: ApiBudget,
}

impl Scenario {
    /// 국내 기본: 평가액 10,000,000원(보유 없음) · 삼성전자 시세 50,000원(5초 전) · 장중 · 운용 중.
    pub fn kr() -> Self {
        let now = t(KR_NOW);
        let (quote, calendar) = kr_fake(SAMSUNG, d("50000"), now - Duration::seconds(5));
        let equity = d("10000000");
        Self {
            now,
            params: params(),
            config: RulesConfig {
                banned_symbols: Vec::new(),
                sectors: HashMap::from([
                    (Symbol::new(SAMSUNG), "반도체".to_string()),
                    (Symbol::new(HYNIX), "반도체".to_string()),
                ]),
            },
            rules: kr_rules(),
            calendar,
            reference_price: quote.price,
            quote,
            stale: StalePolicy::default(),
            status: SymbolStatus {
                vi: Some(false),
                halted: Some(false),
            },
            team: TeamStatus::Operating,
            book: Book {
                currency: Currency::Krw,
                equity,
                day_start_equity: equity,
                peak_equity: equity,
                positions: Vec::new(),
            },
            health: healthy(),
            open_orders: Vec::new(),
            trades_today: 0,
            api: ApiBudget { used: 0, limit: 10 },
        }
    }

    pub fn ctx(&self) -> CheckContext<'_> {
        CheckContext {
            now: self.now,
            params: &self.params,
            config: &self.config,
            market_rules: &self.rules,
            calendar: &self.calendar,
            quote: &self.quote,
            stale: self.stale,
            reference_price: self.reference_price,
            status: self.status,
            team: self.team,
            book: &self.book,
            health: self.health,
            open_orders: &self.open_orders,
            trades_today: self.trades_today,
            api: self.api,
        }
    }

    pub fn check(&self, draft: &OrderDraft) -> Outcome {
        check(draft, &self.ctx())
    }

    /// 보유를 더한다. 평가액은 바꾸지 않는다(시험이 정한 값 그대로).
    pub fn hold(&mut self, symbol: &str, quantity: &str, avg: &str, last: &str) {
        self.book.positions.push(Position {
            market: Market::Kr,
            symbol: Symbol::new(symbol),
            quantity: d(quantity),
            avg_price: d(avg),
            last_price: d(last),
        });
    }
}

/// 모두 정상인 자료와 연결 상태.
pub fn healthy() -> Health {
    Health {
        account_in_sync: true,
        api_error: false,
        data_anomaly: false,
    }
}

/// 근거가 다 있는 국내 매수 안: 지정가 50,000 · 손절 47,000 · 목표 56,000 · 주당 기대값 1,000.
pub fn kr_buy() -> OrderDraft {
    let now = t(KR_NOW);
    OrderDraft {
        market: Market::Kr,
        symbol: Symbol::new(SAMSUNG),
        purpose: Purpose::NewEntry,
        order_type: OrderType::Limit,
        limit_price: Some(d("50000")),
        ai_quantity: None,
        funding: Funding::Cash,
        product: ProductKind::Stock,
        stage: TradeStage::Judged,
        decided_at: now - Duration::seconds(60),
        rationale: Rationale {
            hypothesis: Some("실적 개선".to_string()),
            trigger: Some("전고점 돌파".to_string()),
            entry: Some(d("50000")),
            stop: Some(d("47000")),
            target: Some(d("56000")),
            expected_value: Some(d("1000")),
        },
        sources: vec![SourceStamp {
            name: "시세".to_string(),
            known_at: now - Duration::seconds(70),
        }],
    }
}

/// 국내 시장가 팔기 안(가진 수량 전부). 목적은 받은 값.
pub fn kr_sell(purpose: Purpose) -> OrderDraft {
    OrderDraft {
        purpose,
        order_type: OrderType::Market,
        limit_price: None,
        ..kr_buy()
    }
}

/// 모의계좌 주문 내용.
pub fn approved(
    side: Side,
    order_type: OrderType,
    quantity: &str,
    price: Option<&str>,
) -> ApprovedOrder {
    ApprovedOrder {
        market: Market::Kr,
        symbol: Symbol::new(SAMSUNG),
        side,
        order_type,
        quantity: d(quantity),
        price: price.map(d),
        account_kind: AccountKind::Paper,
    }
}

/// 통과여야 한다.
pub fn expect_pass(outcome: Outcome) -> Passed {
    match outcome {
        Outcome::Pass(passed) => passed,
        other => panic!("통과여야 한다: {other:?}"),
    }
}

/// 막힘이어야 한다.
pub fn expect_blocked(outcome: Outcome) -> Blocked {
    match outcome {
        Outcome::Blocked(blocked) => blocked,
        other => panic!("막혀야 한다: {other:?}"),
    }
}

/// 막힘의 위반 목록(원칙 위반이 아니면 빈 목록).
pub fn violations(blocked: &Blocked) -> &[Violation] {
    match &blocked.reason {
        Reason::Violations(list) => list,
        _ => &[],
    }
}
