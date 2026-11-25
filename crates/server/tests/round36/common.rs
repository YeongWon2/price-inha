//! round36 시험이 함께 쓰는 도우미: 시계 · 달력 · 모의계좌 설정 · 자료 가짜 서버 · 서버 띄우기 · HTTP · 승인 요청.
//! round19의 공용 장치를 본떠 두고(그 폴더는 고치지 않는다) 34차(기다리는 승인 칸 · 승인 이벤트의 담당 · 상태)에
//! 맞게 줄이고 넓혔다.
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0` · 시연 모드로 띄우고 투자 팀과 조립을 잇는다
//! ([`price_inha::App::launch_with`]). AI는 시연 모드의 가짜 공급자뿐이고, 자료(DART · ECOS)는 로컬 가짜
//! 서버(wiremock)뿐이다. 토스 · Anthropic 자리는 아무도 듣지 않는 로컬 주소다.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_approval::{
    Answer, ApprovalNumber, ApprovalView, Approvals, Ask, Decision, Fingerprint, OrderSubject,
    Side, SignalKind, Subject, WaitTimes,
};
use pi_data::{CorpCode, DataConfig};
use pi_market::{
    Clock, Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol,
    TossConfig, TradingDay,
};
use pi_paper::{
    InitialCash, LimitFillPrice, MarketRules, MaxFill, MoneyRounding, OrderType, PaperConfig,
    ProceedsUsable, RoundingMode, SessionRules, Settlement, TickBand,
};
use pi_rules::{
    Funding, OrderDraft, ProductKind, Purpose, Rationale, RulesConfig, SourceStamp, SymbolStatus,
    TradeStage,
};
use pi_secrets::{Credentials, KeyKind, KeySlot, SecretStore, Verifier, VerifyFailure};
use pi_store::{
    AccountKind, Amount, ApprovalStatus, Judgment, NewTrade, ProposalId, RoleScore, SourceRef,
    Store, TeamState, TradeId, UnixMillis,
};
use price_inha::{
    App, CalendarFn, DayPlan, Launch, OFFICE_PATH, OrderRequest, Placed, Reach, Settings,
    TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TeamWiring, TradingWiring, Upstreams, Wiring,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 지켜볼 종목(시연 시세가 흐른다).
const WATCHED: &str = "005930";
/// 매매하는 종목. 시험이 시세를 넣는다.
const HYNIX: &str = "000660";
const CORP_WATCHED: &str = "00126380";
const CORP_HYNIX: &str = "00164779";
/// 시험용 자료 키(진짜 키처럼 보이지 않는다). 가림 시험이 사유 글에 넣는다.
pub const DART_KEY: &str = "test-dart-key-round36-zz";
const ECOS_KEY: &str = "test-ecos-key-round36-zz";
/// 가짜 DART가 주는 공시의 이름 · 접수 번호.
const DISCLOSURE_NAME: &str = "주요사항보고서(자기주식취득결정)";
const DISCLOSURE_RECEIPT: &str = "20261116000123";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
const ENTRY: i64 = 100_000;
const STOP: i64 = 95_000;
const TARGET: i64 = 110_000;
const EDGE: i64 = 5_000;

/// 글자를 십진수로.
fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

// ───────────── 시계 · 달력 ─────────────

/// 테스트가 정하는 시계.
pub struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    fn set(&self, time: DateTime<Utc>) {
        self.0.store(time.timestamp_millis(), Ordering::SeqCst);
    }
}

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

/// 2026-11-16 월요일.
pub fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
}

/// 그 날의 한국 시각.
pub fn kst(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    let offset = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    offset
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

fn ms(at: DateTime<Utc>) -> UnixMillis {
    UnixMillis(at.timestamp_millis())
}

fn business_day(date: NaiveDate, forward: bool) -> NaiveDate {
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
            start: kst(date, 9, 0),
            end: kst(date, 15, 30),
        }],
    }
}

fn kr_calendar(date: NaiveDate) -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        today: kr_day(date),
        previous_business_day: kr_day(business_day(date, false)),
        next_business_day: kr_day(business_day(date, true)),
    }
}

// ───────────── 자료 가짜 서버 ─────────────

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn disclosure_body(corp: &str, stock: &str) -> Value {
    json!({
        "status": "000", "message": "정상", "page_no": 1, "page_count": 10,
        "total_count": 1, "total_page": 1,
        "list": [{
            "corp_cls": "Y", "corp_name": "시험회사", "corp_code": corp, "stock_code": stock,
            "report_nm": DISCLOSURE_NAME, "rcept_no": DISCLOSURE_RECEIPT,
            "flr_nm": "시험회사", "rcept_dt": "20261116", "rm": "유"
        }]
    })
}

fn financials_body(corp: &str) -> Value {
    json!({
        "status": "000", "message": "정상",
        "list": [{
            "rcept_no": "20260310000789", "reprt_code": "11011", "bsns_year": "2025",
            "corp_code": corp, "stock_code": "000000", "fs_div": "CFS", "fs_nm": "연결재무제표",
            "sj_div": "IS", "sj_nm": "손익계산서", "account_nm": "매출액", "thstrm_nm": "제 57 기",
            "thstrm_dt": "2025.01.01 ~ 2025.12.31", "thstrm_amount": "300,870,903,000,000",
            "frmtrm_nm": "제 56 기", "frmtrm_dt": "2024.01.01 ~ 2024.12.31",
            "frmtrm_amount": "1,000", "ord": "21", "currency": "KRW"
        }]
    })
}

fn ecos_body(stat: &str, item: &str, name: &str, unit: &str, time: &str, value: &str) -> Value {
    json!({
        "StatisticSearch": {
            "list_total_count": 1,
            "row": [{
                "STAT_CODE": stat, "STAT_NAME": name, "ITEM_CODE1": item, "ITEM_NAME1": name,
                "ITEM_CODE2": null, "ITEM_NAME2": null, "ITEM_CODE3": null, "ITEM_NAME3": null,
                "ITEM_CODE4": null, "ITEM_NAME4": null, "UNIT_NAME": unit, "WGT": null,
                "TIME": time, "DATA_VALUE": value
            }]
        }
    })
}

/// 자료 가짜 서버(DART · ECOS 한 곳). 두 종목 모두 [`DISCLOSURE_NAME`] 공시 하나와 재무 · 금리 · 환율이 있다.
struct DataServer {
    server: MockServer,
}

impl DataServer {
    async fn start() -> DataServer {
        let server = MockServer::start().await;
        for (corp, stock) in [(CORP_WATCHED, WATCHED), (CORP_HYNIX, HYNIX)] {
            Mock::given(method("GET"))
                .and(path("/api/list.json"))
                .and(query_param("corp_code", corp))
                .respond_with(ok(disclosure_body(corp, stock)))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/fnlttSinglAcnt.json"))
                .and(query_param("corp_code", corp))
                .respond_with(ok(financials_body(corp)))
                .mount(&server)
                .await;
        }
        let rate = ecos_body(
            "722Y001",
            "0101000",
            "한국은행 기준금리",
            "연%",
            "202610",
            "2.25",
        );
        Mock::given(method("GET"))
            .and(path_regex("^/api/StatisticSearch/.*/722Y001/"))
            .respond_with(ok(rate))
            .mount(&server)
            .await;
        let fx = ecos_body(
            "731Y001",
            "0000001",
            "원/미국달러",
            "원",
            "20261113",
            "1391.5",
        );
        Mock::given(method("GET"))
            .and(path_regex("^/api/StatisticSearch/.*/731Y001/"))
            .respond_with(ok(fx))
            .mount(&server)
            .await;
        DataServer { server }
    }

    /// 이 가짜 서버를 쓰는 팀 조립 값.
    fn team(&self) -> TeamWiring {
        let mut team = TeamWiring::official();
        let uri = self.server.uri();
        team.data = DataConfig::new(uri.clone(), uri.clone(), uri);
        team.data.timeout = Duration::from_secs(3);
        team.corp_codes = HashMap::from([
            (Symbol::new(WATCHED), CorpCode::parse(CORP_WATCHED).unwrap()),
            (Symbol::new(HYNIX), CorpCode::parse(CORP_HYNIX).unwrap()),
        ]);
        team
    }
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

// ───────────── 설정 · 띄우기 ─────────────

fn market_rules(currency: Currency, scale: u32, ticks: Vec<TickBand>) -> MarketRules {
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

fn trading_wiring() -> TradingWiring {
    let tick = |below: Option<&str>, tick: &str| TickBand {
        below: below.map(d),
        tick: d(tick),
    };
    let paper = PaperConfig {
        initial_cash: InitialCash {
            krw: d("10000000"),
            usd: d("10000"),
        },
        kr: market_rules(
            Currency::Krw,
            0,
            vec![
                tick(Some("50000"), "50"),
                tick(Some("200000"), "100"),
                tick(None, "1000"),
            ],
        ),
        us: market_rules(Currency::Usd, 2, vec![tick(None, "0.01")]),
        stale: StalePolicy {
            max_age: Duration::from_secs(30),
        },
        event_buffer: 64,
    };
    let calendar: CalendarFn = Arc::new(kr_calendar);
    TradingWiring {
        paper,
        rules: RulesConfig {
            banned_symbols: Vec::new(),
            sectors: HashMap::from([(Symbol::new(HYNIX), "반도체".to_string())]),
        },
        calendar: Some(calendar),
    }
}

fn no_env(_name: &str) -> Option<String> {
    None
}

/// 설정: 임시 보관 폴더 · 빈 포트 · 짧은 시세 간격 · `--demo`.
fn demo_settings(home: &Path) -> Settings {
    let home = home.to_str().unwrap();
    let list = [
        "--home",
        home,
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--symbols",
        WATCHED,
        "--demo",
    ];
    let args: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

/// 바깥 자리: 토스 · Anthropic은 아무도 듣지 않는 로컬 주소다.
fn wiring(clock: Arc<TestClock>) -> Wiring {
    let dead = dead_base_url();
    Wiring {
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
        clock,
        peer_override: None,
        cli_locations: Vec::new(),
    }
}

/// 무엇이든 받는 시험용 확인자(바깥으로 나가지 않는다).
struct AcceptAll;

impl Verifier for AcceptAll {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_slot(
        &self,
        _slot: KeySlot,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

/// 보관 폴더에 칸 하나의 키를 등록한다.
async fn register_slot(home: &Path, slot: KeySlot, value: &str) {
    let secrets = SecretStore::open(home).unwrap();
    let values = Credentials::new().with(slot.fields()[0], value);
    secrets
        .register_slot(slot, values, &AcceptAll)
        .await
        .unwrap();
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 가짜 자료 서버.
pub struct Bench {
    _dir: TempDir,
    clock: Arc<TestClock>,
    app: App,
    _data: DataServer,
}

impl Bench {
    /// 새 보관 폴더로 그 시각에 시연 모드로 띄운다. 자료 키(DART · ECOS)를 등록해 둔다.
    pub async fn start(at: DateTime<Utc>) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        register_slot(dir.path(), KeySlot::Dart, DART_KEY).await;
        register_slot(dir.path(), KeySlot::Ecos, ECOS_KEY).await;
        let data = DataServer::start().await;
        let clock = TestClock::at(at);
        let launch = Launch {
            trading: trading_wiring(),
            team: Some(data.team()),
            secrets: None,
        };
        let reach = Reach {
            push: None,
            backup: None,
        };
        let start = App::launch_with(
            demo_settings(dir.path()),
            wiring(Arc::clone(&clock)),
            launch,
            reach,
        );
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            _dir: dir,
            clock,
            app,
            _data: data,
        }
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn set(&self, at: DateTime<Utc>) {
        self.clock.set(at);
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// `seconds`초 뒤로 20초씩 나눠 간다(걸음마다 뒤에서 도는 일이 따라오게 잠깐 쉰다).
    pub async fn advance(&self, seconds: i64) {
        let from = self.now();
        let mut gone = 0;
        while gone < seconds {
            gone = (gone + 20).min(seconds);
            self.set(from + chrono::Duration::seconds(gone));
            settle().await;
        }
    }

    /// 시계를 그 시각으로 옮기고 그 시각에 받은 시세 하나를 넣는다.
    async fn quote(&self, symbol: &str, price: i64, at: DateTime<Utc>) {
        self.set(at);
        let quote = Quote {
            symbol: Symbol::new(symbol),
            price: Decimal::from(price),
            currency: Currency::Krw,
            exchange_time: Some(at),
            received_at: at,
        };
        self.app
            .trading()
            .on_quote(quote)
            .await
            .expect("시세를 받아야 한다");
    }

    /// 금액 한도를 낮춘 스윙 템플릿으로 오늘 매매하기로 한다(사는 주문이 승인을 기다리게).
    pub async fn ready_for_approval(&self) {
        let reply = post(self.addr(), TEMPLATE_PATH, &json!({"style": "swing"})).await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let changes = json!({"symbol_weight_pct": "10", "auto_order_limit_pct": "2"});
        let body = json!({"id": id, "changes": changes});
        let reply = post(self.addr(), TEMPLATE_ADJUST_PATH, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
        let store = self.store();
        if store.team_state().unwrap() != TeamState::Operating {
            let _ = self.app.trading().decide_day(DayPlan::Trade);
        }
        wait_for("운용 중이 된다", || {
            store.team_state().unwrap() == TeamState::Operating
        })
        .await;
    }

    /// 금액 한도를 넘는 사기 주문 안을 내 승인 대기로 만든다([`Bench::ready_for_approval`] 뒤에).
    /// 매매 담당이 낸 주문이 서버의 승인 흐름을 그대로 탄다(사무실 승인 이벤트가 난다).
    pub async fn awaiting(&self) -> (ApprovalNumber, Fingerprint) {
        self.quote(HYNIX, ENTRY, self.now()).await;
        let trade = self.judged(HYNIX);
        let at = self.now();
        let placed = self
            .app
            .trading()
            .place(order(trade, HYNIX, Purpose::NewEntry, ENTRY, at))
            .await
            .expect("주문 안을 처리해야 한다");
        let Placed::AwaitingApproval {
            number,
            fingerprint,
        } = placed
        else {
            panic!("금액 한도를 넘으면 승인을 기다린다: {placed:?}");
        };
        (number, fingerprint)
    }

    /// 판단됨인 매매 하나(근거 6칸이 다 참). 판단 시각은 지금이다.
    fn judged(&self, symbol: &str) -> TradeId {
        let store = self.store();
        let at = ms(self.now());
        let trade = store
            .create_trade(NewTrade {
                symbol: symbol.to_string(),
                account: AccountKind::Paper,
                at,
            })
            .unwrap();
        let judgment = Judgment {
            decided_at: at,
            rationale: pi_store::Rationale {
                hypothesis: Some("실적 개선".to_string()),
                trigger: Some("거래량 증가".to_string()),
                entry: Some(Amount(ENTRY)),
                stop: Some(Amount(STOP)),
                target: Some(Amount(TARGET)),
                expected_value: Some(Amount(EDGE)),
            },
            sources: vec![SourceRef {
                name: "시세".to_string(),
                known_at: at,
            }],
            scores: vec![RoleScore {
                role: "risk".to_string(),
                score: 70,
                reason: "근거 충분".to_string(),
            }],
            consensus_score: 70,
        };
        store.judge_trade(trade.id, judgment).unwrap();
        trade.id
    }

    /// 승인 모듈의 눈으로 같은 저장소를 본다(시각은 시험 시계).
    pub fn approvals(&self) -> Approvals {
        let clock: Arc<dyn pi_approval::Clock> = Arc::new(ApprovalClock(Arc::clone(&self.clock)));
        Approvals::new(self.store(), clock, WaitTimes::default())
    }

    /// 지금 시각에 그 대상의 승인 요청 하나를 만들고 승인 번호를 돌려준다.
    pub fn ask(&self, subject: Subject, reason: &str) -> String {
        let requested = self
            .approvals()
            .request(Ask {
                subject,
                reason: reason.to_string(),
            })
            .expect("승인 요청을 만든다");
        requested.number.as_str().to_string()
    }

    /// 그 승인 요청의 지금 모습.
    pub fn view(&self, number: &str) -> ApprovalView {
        self.approvals()
            .get(&ApprovalNumber::new(number))
            .expect("승인 요청을 읽는다")
            .expect("그 번호의 승인 요청이 있다")
    }

    /// 그 승인 요청을 거절로 닫는다.
    pub fn refuse(&self, number: &str) {
        let answer = self
            .approvals()
            .answer(&ApprovalNumber::new(number), "", Decision::Reject)
            .expect("답을 받는다");
        assert!(
            matches!(answer, Answer::Rejected(_)),
            "거절로 닫힌다: {answer:?}"
        );
    }

    /// 그 승인 요청이 기다림에서 바뀔 때까지 기다린다(뒤에서 도는 기한 처리).
    pub async fn wait_closed(&self, number: &str) {
        wait_for("승인 요청이 닫힌다", || {
            self.view(number).status != ApprovalStatus::Pending
        })
        .await;
    }

    /// 사무실 첫 화면 자료(`GET /api/office`, 200이어야 한다).
    pub async fn office(&self) -> Value {
        let reply = get(self.addr(), OFFICE_PATH).await;
        assert_eq!(reply.status, 200, "{}", reply.text);
        reply.body
    }
}

/// 승인 모듈이 읽는 시계: 시험 시계 그대로.
struct ApprovalClock(Arc<TestClock>);

impl pi_approval::Clock for ApprovalClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.now().timestamp_millis())
    }
}

/// 규칙 개선안 적용 대상(개선안 번호가 다르면 다른 요청이다).
pub fn rule(proposal: i64) -> Subject {
    Subject::RuleImprovement {
        proposal: ProposalId(proposal),
    }
}

/// 사는 지정가 주문 대상. 신호는 `valid_seconds`초 뒤 끝난다(그때부터 만료). 매매 번호가 없는 신호 id다.
pub fn buy_order(bench: &Bench, signal: &str, valid_seconds: i64) -> Subject {
    let until = bench.now() + chrono::Duration::seconds(valid_seconds);
    Subject::Order(OrderSubject {
        account: AccountKind::Paper,
        symbol: HYNIX.to_string(),
        side: Side::Buy,
        quantity: 5,
        price: Decimal::from(ENTRY),
        order_type: pi_approval::OrderType::Limit,
        signal_id: signal.to_string(),
        signal_kind: SignalKind::Buy,
        signal_valid_until: ms(until),
    })
}

/// 지정가 · 수량 5주의 주문 안.
fn order(
    trade: TradeId,
    symbol: &str,
    purpose: Purpose,
    price: i64,
    at: DateTime<Utc>,
) -> OrderRequest {
    let stage = if purpose.is_buy() {
        TradeStage::Judged
    } else {
        TradeStage::Holding
    };
    let draft = OrderDraft {
        market: Market::Kr,
        symbol: Symbol::new(symbol),
        purpose,
        order_type: OrderType::Limit,
        limit_price: Some(Decimal::from(price)),
        ai_quantity: Some(d("5")),
        funding: Funding::Cash,
        product: ProductKind::Stock,
        stage,
        decided_at: at,
        rationale: Rationale {
            hypothesis: Some("실적 개선".to_string()),
            trigger: Some("거래량 증가".to_string()),
            entry: Some(Decimal::from(ENTRY)),
            stop: Some(Decimal::from(STOP)),
            target: Some(Decimal::from(TARGET)),
            expected_value: Some(Decimal::from(EDGE)),
        },
        sources: vec![SourceStamp {
            name: "시세".to_string(),
            known_at: at,
        }],
    };
    OrderRequest {
        trade_id: trade,
        draft,
        reference_price: Decimal::from(ENTRY),
        status: SymbolStatus {
            vi: Some(false),
            halted: Some(false),
        },
    }
}

// ───────────── 기다리기 · 이벤트 ─────────────

/// 뒤에서 도는 일이 몇 바퀴 돌 만큼 잠깐 쉰다.
pub async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// 조건이 될 때까지 기다린다(가장 길게 [`STEP`]).
pub async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + STEP;
    while !done() {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 저장된 그 종류 이벤트들의 내용.
pub fn events(store: &Store, kind: &str) -> Vec<Value> {
    store
        .read_after(0, 100_000)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == kind)
        .map(|event| event.payload)
        .collect()
}

// ───────────── HTTP ─────────────

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태 · JSON 본문(JSON이 아니면 `Null`) · 본문 글.
pub struct Reply {
    pub status: u16,
    pub body: Value,
    pub text: String,
}

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문을 읽어야 한다");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply { status, body, text }
}

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
async fn login(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let reply = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    assert!(reply.status().is_success(), "세션을 받아야 한다");
    let raw = reply
        .headers()
        .get("set-cookie")
        .expect("세션 쿠키가 있어야 한다")
        .to_str()
        .unwrap();
    raw.split(';').next().unwrap().trim().to_string()
}

/// 세션을 받아 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    send(request).await
}

/// 세션을 받아 같은 출처에서 보낸다.
pub async fn post(addr: SocketAddr, path: &str, body: &Value) -> Reply {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie)
        .header("Origin", origin_of(addr))
        .header("Content-Type", "application/json")
        .body(body.to_string());
    send(request).await
}
