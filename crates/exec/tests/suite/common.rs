//! 테스트가 함께 쓰는 도우미: 시계 · 설정 · 달력 · 매매 · 표시와 그것을 다루는 책상(`Bench`).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_approval::{
    Approvals, Ask, Decision, OrderSubject, OrderType as SubjectOrderType, Side as SubjectSide,
    SignalKind, Subject, WaitTimes,
};
use pi_exec::{Error, Executor, Guard, SellPurpose, SellSubmission, Submission};
use pi_market::{
    Clock, Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol,
    TradingDay,
};
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, CheckAuthority, CheckPermit, Holding, InitialCash,
    LimitFillPrice, MarketRules, MaxFill, MoneyRounding, Order, OrderGroup, OrderType, PaperConfig,
    ProceedsUsable, RoundingMode, SessionRules, Settlement, Side, Snapshot, TickBand,
    UnfilledTimeout,
};
use pi_rules::ApprovalGrant;
use pi_store::{
    AccountKind as StoreAccount, Amount, Judgment, NewTrade, Rationale, RoleScore, SourceRef,
    Store, Trade, TradeId, UnixMillis,
};
use rust_decimal::Decimal;
use tempfile::TempDir;

/// 삼성전자. 기준가 70,000원(호가 단위 100원).
pub const SAM: &str = "005930";
/// 매매 근거의 손절값(원).
pub const STOP: i64 = 65_000;

/// 글자를 십진수로.
pub fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// 기준 영업일: 2026-11-16 월요일.
pub fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
}

fn utc(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
}

/// 국내 정규장 안의 기준 시각(월요일 10:00 KST = 01:00 UTC)에서 `seconds`초 뒤.
pub fn at(seconds: i64) -> DateTime<Utc> {
    utc(monday(), 1, 0) + chrono::Duration::seconds(seconds)
}

/// std 간격을 chrono 간격으로.
pub fn span(duration: Duration) -> chrono::Duration {
    chrono::Duration::from_std(duration).unwrap()
}

fn weekday_step(date: NaiveDate, forward: bool) -> NaiveDate {
    let mut next = date;
    loop {
        next = if forward {
            next.succ_opt().unwrap()
        } else {
            next.pred_opt().unwrap()
        };
        if !matches!(next.weekday(), Weekday::Sat | Weekday::Sun) {
            return next;
        }
    }
}

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

fn kr_calendar(date: NaiveDate) -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        today: kr_day(date),
        previous_business_day: kr_day(weekday_step(date, false)),
        next_business_day: kr_day(weekday_step(date, true)),
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
        price_limit_rate: Some(d("0.30")),
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

/// 테스트 설정. 원화 10,000,000 · 달러 10,000. 체결 한도 없음.
pub fn config() -> PaperConfig {
    let tick = |below: Option<&str>, tick: &str| TickBand {
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
            vec![
                tick(Some("50000"), "50"),
                tick(Some("200000"), "100"),
                tick(None, "1000"),
            ],
        ),
        us: rules(Currency::Usd, 2, vec![tick(None, "0.01")]),
        stale: StalePolicy {
            max_age: Duration::from_secs(30),
        },
        event_buffer: 64,
    }
}

/// 국내 주문이 시세 한 번에 `quantity`주까지만 체결되는 설정(일부 체결을 만든다).
pub fn config_partial(quantity: &str) -> PaperConfig {
    let mut config = config();
    config.kr.max_fill = MaxFill::PerQuote(d(quantity));
    config
}

/// 테스트가 정하는 시계. 기다리지 않는다.
#[derive(Debug)]
pub struct TestClock {
    now: Mutex<DateTime<Utc>>,
}

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().unwrap()
    }

    async fn sleep(&self, _duration: Duration) {}
}

/// 승인 모듈에 줄 멈춘 시계.
struct FixedMillis(UnixMillis);

impl pi_approval::Clock for FixedMillis {
    fn now(&self) -> UnixMillis {
        self.0
    }
}

/// 실행기 · 저장소 · 시계 · 권한을 함께 쥔 책상.
pub struct Bench {
    _dir: TempDir,
    pub store: Store,
    clock: Arc<TestClock>,
    pub exec: Executor,
    pub authority: CheckAuthority,
}

impl Bench {
    /// 새 계좌로 열고 달력 · 기준가를 넣고 맞춰 보기까지 마친 책상.
    pub async fn new() -> Self {
        Self::with(config()).await
    }

    /// 설정을 정해 연 책상(맞춰 보기까지 마침).
    pub async fn with(config: PaperConfig) -> Self {
        let bench = Self::unready(config);
        bench.exec.reconcile().await.unwrap();
        bench
    }

    /// 새 계좌로 열고 달력 · 기준가만 넣은 책상(맞춰 보기 전).
    pub fn unready(config: PaperConfig) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("pi.db")).unwrap();
        let clock = Arc::new(TestClock {
            now: Mutex::new(at(0)),
        });
        let (exec, authority) =
            Executor::open(config, store.clone(), clock.clone() as Arc<dyn Clock>).unwrap();
        prepare(&exec);
        Self {
            _dir: dir,
            store,
            clock,
            exec,
            authority,
        }
    }

    /// 껐다 켠다: 같은 저장소로 새 실행기를 연다(맞춰 보기 전). 달력 · 기준가는 다시 넣는다.
    pub fn reopen(&mut self, config: PaperConfig) {
        let (exec, authority) = self.try_open(config).unwrap();
        prepare(&exec);
        self.exec = exec;
        self.authority = authority;
    }

    /// 같은 저장소로 실행기를 열어 본다(책상은 그대로).
    pub fn try_open(&self, config: PaperConfig) -> Result<(Executor, CheckAuthority), Error> {
        Executor::open(
            config,
            self.store.clone(),
            self.clock.clone() as Arc<dyn Clock>,
        )
    }

    /// 시계를 기준 시각에서 `seconds`초 뒤로.
    pub fn set(&self, seconds: i64) {
        self.set_at(at(seconds));
    }

    /// 시계를 그 시각으로.
    pub fn set_at(&self, now: DateTime<Utc>) {
        *self.clock.now.lock().unwrap() = now;
    }

    /// 지금 시각.
    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// 판단됨인 삼성전자 매매(모의 계좌, 근거 6칸이 다 참).
    pub fn judged(&self) -> TradeId {
        self.judged_with(StoreAccount::Paper, Some(Amount(STOP)))
    }

    /// 계좌 종류와 손절값을 정한 판단됨 매매. 손절값이 없으면 근거가 빈 것이다.
    pub fn judged_with(&self, account: StoreAccount, stop: Option<Amount>) -> TradeId {
        let ms = UnixMillis(self.now().timestamp_millis());
        let trade = self
            .store
            .create_trade(NewTrade {
                symbol: SAM.to_string(),
                account,
                at: ms,
            })
            .unwrap();
        let judgment = Judgment {
            decided_at: ms,
            rationale: Rationale {
                hypothesis: Some("실적 개선".to_string()),
                trigger: Some("거래량 증가".to_string()),
                entry: Some(Amount(70_000)),
                stop,
                target: Some(Amount(80_000)),
                expected_value: Some(Amount(1_000)),
            },
            sources: vec![SourceRef {
                name: "시세".to_string(),
                known_at: ms,
            }],
            scores: vec![RoleScore {
                role: "분석".to_string(),
                score: 70,
                reason: "근거 충분".to_string(),
            }],
            consensus_score: 70,
        };
        self.store.judge_trade(trade.id, judgment).unwrap();
        trade.id
    }

    /// 매매 한 건.
    pub fn trade(&self, id: TradeId) -> Trade {
        self.store.get_trade(id).unwrap()
    }

    /// 이 계좌의 점검 통과 표시(삼성전자 지정가).
    pub fn permit(&self, side: Side, quantity: &str, price: &str) -> CheckPermit {
        self.authority.approve(approved(side, quantity, price))
    }

    /// 사는 주문 하나(모의 템플릿, 승인 없음, 60초 미체결 취소).
    pub fn buy(&self, trade_id: TradeId, quantity: &str, price: &str) -> Submission {
        Submission {
            trade_id,
            permit: self.permit(Side::Buy, quantity, price),
            template_account: AccountKind::Paper,
            approval: None,
            timeout: UnfilledTimeout::SCALP,
        }
    }

    /// 파는 주문 하나(60초 미체결 취소).
    pub fn sell(
        &self,
        trade_id: TradeId,
        purpose: SellPurpose,
        quantity: &str,
        price: &str,
    ) -> SellSubmission {
        SellSubmission {
            trade_id,
            permit: self.permit(Side::Sell, quantity, price),
            purpose,
            timeout: UnfilledTimeout::SCALP,
        }
    }

    /// 지금 시각에 받은 삼성전자 시세.
    pub fn quote(&self, price: &str) -> Quote {
        Quote {
            symbol: Symbol::new(SAM),
            price: d(price),
            currency: Currency::Krw,
            exchange_time: Some(self.now()),
            received_at: self.now(),
        }
    }

    /// 보유중인 매매 하나를 만든다: 10주를 70,000원에 사서 다 체결시킨다(1초 뒤).
    pub async fn holding(&self) -> TradeId {
        let id = self.judged();
        self.exec.submit(self.buy(id, "10", "70000")).await.unwrap();
        self.set_at(self.now() + chrono::Duration::seconds(1));
        self.exec.on_quote(&self.quote("70000")).unwrap();
        id
    }

    /// 그 매매의 삼성전자 지정가 사기를 승인 모듈로 요청 · 승인하고(`consume`이면 소비까지) 만든 증표.
    /// 승인한 내용의 신호 id는 `trade-<매매 번호>`다(조립하는 쪽의 약속).
    pub fn grant(
        &self,
        trade_id: TradeId,
        quantity: u64,
        price: &str,
        consume: bool,
    ) -> ApprovalGrant {
        let now = UnixMillis(self.now().timestamp_millis());
        let approvals = Approvals::new(
            self.store.clone(),
            Arc::new(FixedMillis(now)),
            WaitTimes::default(),
        );
        let subject = Subject::Order(OrderSubject {
            account: StoreAccount::Paper,
            symbol: SAM.to_string(),
            side: SubjectSide::Buy,
            quantity,
            price: d(price),
            order_type: SubjectOrderType::Limit,
            signal_id: format!("trade-{}", trade_id.0),
            signal_kind: SignalKind::Buy,
            signal_valid_until: UnixMillis(now.0 + 3_600_000),
        });
        let requested = approvals
            .request(Ask {
                subject: subject.clone(),
                reason: "큰 금액".to_string(),
            })
            .unwrap();
        approvals
            .answer(
                &requested.number,
                requested.fingerprint.as_str(),
                Decision::Approve,
            )
            .unwrap();
        if consume {
            approvals.consume(&requested.number, &subject).unwrap();
        }
        ApprovalGrant::new(requested.number.as_str(), requested.fingerprint.as_str())
    }

    /// 증권사의 주문 목록.
    pub async fn orders(&self, group: OrderGroup) -> Vec<Order> {
        let broker: &dyn Broker = self.exec.broker();
        broker.orders(group, None, self.now()).await.unwrap()
    }

    /// 증권사의 보유.
    pub async fn holdings(&self) -> Vec<Holding> {
        let broker: &dyn Broker = self.exec.broker();
        broker.holdings(None, self.now()).await.unwrap()
    }

    /// 저장소의 가장 나중 계좌 스냅샷이 지금 계좌와 같은가.
    pub fn snapshot_saved(&self) -> bool {
        let Some(saved) = self.store.latest_paper_snapshot().unwrap() else {
            return false;
        };
        let saved: Snapshot = serde_json::from_value(saved.state).unwrap();
        saved == self.exec.broker().snapshot()
    }
}

/// 삼성전자 매매의 손절 감시 자리(손절값은 근거의 것).
pub fn guard(trade_id: TradeId, quantity: &str) -> Guard {
    Guard {
        trade_id,
        market: Market::Kr,
        symbol: Symbol::new(SAM),
        quantity: d(quantity),
        stop: Amount(STOP),
    }
}

/// 삼성전자 지정가 주문 내용(모의 계좌).
pub fn approved(side: Side, quantity: &str, price: &str) -> ApprovedOrder {
    ApprovedOrder {
        market: Market::Kr,
        symbol: Symbol::new(SAM),
        side,
        order_type: OrderType::Limit,
        quantity: d(quantity),
        price: Some(d(price)),
        account_kind: AccountKind::Paper,
    }
}

/// 달력과 기준가를 넣는다(열 때마다 다시 넣어야 한다).
fn prepare(exec: &Executor) {
    exec.broker().set_calendar(kr_calendar(monday()));
    exec.broker()
        .set_reference_price(Symbol::new(SAM), d("70000"), monday());
}
