//! round57 시험(담당별 AI 모델 고르기)이 함께 쓰는 도우미. round42의 발판(가짜 증권사 · 가짜 AI · 가짜 자료 ·
//! 시험 시계 · 다른 기기로 보이기)을 본떠 두고(그 폴더는 고치지 않는다) 메신저 · 푸시 · 백업 자리는 뺐다.
//!
//! - [`Bench`]: 임시 보관 폴더 · `127.0.0.1:0` · **시연 아님**(시연이면 가짜 AI가 모델 이름을 보지 않는다). 필수 키는
//!   시험용 값으로 미리 등록한다. 토스 자리는 [`HYNIX`] 시세를 주는 로컬 가짜 증권사, Anthropic 자리는 [`FakeAi`]
//!   (받은 요청의 역할 · **모델 이름** · 마지막 글을 남긴다), 자료 자리는 round42와 같은 로컬 가짜 서버다. 팀
//!   (`TeamWiring::official()` 바탕 — 모델은 기본값)을 이어 [`price_inha::App::launch`]로 띄운다.
//! - 켜면 깨우기 엔진의 첫 tick이 뒤에서 하루 판단 AI를 부른다. 그래서 "바꾼 뒤의 요청"은 [`FakeAi::settled`]로 잦아든
//!   뒤의 수를 기준으로 가른다.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_data::{CorpCode, DataConfig, NaverNewsConfig};
use pi_market::{
    Clock, Currency, Market, MarketCalendar, Session, SessionKind, StalePolicy, Symbol, TossConfig,
    TradingDay,
};
use pi_paper::{
    InitialCash, LimitFillPrice, MarketRules, MaxFill, MoneyRounding, PaperConfig, ProceedsUsable,
    RoundingMode, SessionRules, Settlement, TickBand,
};
use pi_rules::RulesConfig;
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::{AccessKeyHash, UnixMillis};
use price_inha::{
    App, CalendarFn, Crew, Launch, Settings, TeamWiring, TradingWiring, Upstreams, Wiring,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(20);
/// 매매하는 종목(국내).
pub const HYNIX: &str = "000660";
/// 시험용 증권사 · AI 키(진짜 키처럼 보이지 않는다). 어느 응답에도 나오면 안 된다.
pub const CLIENT_ID: &str = "zz-round57-client-id-0001";
pub const CLIENT_SECRET: &str = "zz-round57-client-secret-0002";
pub const AI_KEY: &str = "zz-round57-ai-key-qwmv-0003";
/// 등록한 키 값 모두.
pub const KEY_VALUES: [&str; 3] = [CLIENT_ID, CLIENT_SECRET, AI_KEY];

fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// 조립의 기본 담당 모델(`TeamWiring::official()`의 값).
pub fn default_model() -> String {
    TeamWiring::official().model
}

// ───────────── 시계 · 달력 ─────────────

/// 시험이 정하는 시계. 자는 것은 짧게만 잔다.
pub struct TestClock(AtomicI64);

impl TestClock {
    fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
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
fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
}

/// 그 날의 한국 시각.
fn kst(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    seoul()
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

fn seoul() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(9 * 3600).unwrap()
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

// ───────────── 가짜 AI ─────────────

/// 가짜 AI가 받은 요청 하나: 역할 id(시스템 글의 `담당 id: …`) · 요청의 `model` · 마지막 사용자 글.
#[derive(Debug, Clone)]
pub struct Asked {
    pub role: String,
    pub model: String,
    pub last: String,
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn last_user_text(body: &Value) -> String {
    body["messages"]
        .as_array()
        .and_then(|messages| messages.iter().rev().find(|m| m["role"] == json!("user")))
        .map(|message| text_of(&message["content"]))
        .unwrap_or_default()
}

/// round42 · 시연용 가짜 AI와 같은 규칙의 답: 하루 판단 trade · 점수 70 · 팀장은 지켜보기 · scout가 strategy · risk를 부른다.
fn ai_reply(role: &str, last: &str) -> String {
    if last.contains("[하루 판단]") {
        json!({"plan": "trade"}).to_string()
    } else if last.contains("[복기]") {
        json!({"mistakes": [], "note": "시험 복기"}).to_string()
    } else if last.contains("[점수]") {
        json!({"score": 70, "reason": "시험 점수"}).to_string()
    } else if last.contains("판단을 JSON 객체 하나로") {
        json!({"decision": "watch", "reason": "거래량이 적다", "revisit": "거래량이 늘면"})
            .to_string()
    } else if last.contains("[주문 안]") || last.contains("[팔기 주문 안]") {
        json!({"side": "buy", "note": "시험 주문 안"}).to_string()
    } else {
        match role {
            "scout" => "새 자료를 확인했다. @strategy @risk 의견을 부탁한다".to_string(),
            "strategy" => "전략상 지켜볼 만하다".to_string(),
            "risk" => "손절 기준 안에 있다".to_string(),
            _ => "(pass)".to_string(),
        }
    }
}

/// 로컬 가짜 AI 서버(Anthropic 대화 요청). 받은 요청(역할 · 모델 · 마지막 글)을 남기고 정한 뜸 뒤에 답한다.
pub struct FakeAi {
    server: MockServer,
    asked: Arc<Mutex<Vec<Asked>>>,
    delay_ms: Arc<AtomicU64>,
}

impl FakeAi {
    async fn start() -> FakeAi {
        let server = MockServer::start().await;
        let asked: Arc<Mutex<Vec<Asked>>> = Arc::new(Mutex::new(Vec::new()));
        let delay_ms = Arc::new(AtomicU64::new(0));
        let log = Arc::clone(&asked);
        let delay = Arc::clone(&delay_ms);
        Mock::given(method("POST"))
            .respond_with(move |request: &Request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                let system = text_of(&body["system"]);
                let role = system
                    .split("담당 id: ")
                    .nth(1)
                    .and_then(|rest| rest.split_whitespace().next())
                    .unwrap_or("leader")
                    .to_string();
                let last = last_user_text(&body);
                let model = body["model"].as_str().unwrap_or_default().to_string();
                let text = ai_reply(&role, &last);
                log.lock().unwrap().push(Asked { role, model, last });
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(delay.load(Ordering::SeqCst)))
                    .set_body_json(json!({
                        "id": "msg_round57", "type": "message", "role": "assistant",
                        "model": "zz-answer-model",
                        "content": [{"type": "text", "text": text}],
                        "stop_reason": "end_turn",
                        "usage": {"input_tokens": 1, "output_tokens": 1}
                    }))
            })
            .mount(&server)
            .await;
        FakeAi {
            server,
            asked,
            delay_ms,
        }
    }

    fn uri(&self) -> String {
        self.server.uri()
    }

    /// 답마다 이만큼 뜸을 둔다(논의가 그만큼 오래 열려 있다).
    pub fn set_delay(&self, delay: Duration) {
        self.delay_ms
            .store(u64::try_from(delay.as_millis()).unwrap(), Ordering::SeqCst);
    }

    /// 받은 요청들(받은 순서).
    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    /// `from`번째 요청부터의 요청들.
    pub fn asked_from(&self, from: usize) -> Vec<Asked> {
        self.asked().into_iter().skip(from).collect()
    }

    /// 받은 요청 수가 300ms 동안 그대로일 때까지 기다려(가장 길게 6초) 그 수를 돌려준다.
    pub async fn settled(&self) -> usize {
        let mut seen = self.asked().len();
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let now = self.asked().len();
            if now == seen {
                break;
            }
            seen = now;
        }
        seen
    }
}

/// 요청들 가운데 그 역할의 모델 이름들(받은 순서).
pub fn models_of(asked: &[Asked], role: &str) -> Vec<String> {
    asked
        .iter()
        .filter(|a| a.role == role)
        .map(|a| a.model.clone())
        .collect()
}

// ───────────── 가짜 자료 ─────────────

/// [`HYNIX`]의 DART 고유번호.
const CORP_HYNIX: &str = "00164779";

fn data_ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
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

/// round42의 가짜 자료 서버와 같은 답: 논의에 필요한 자료가 모두 있어 담당들이 AI 턴을 돈다.
fn data_reply(request: &Request) -> ResponseTemplate {
    let path = request.url.path();
    match path {
        "/api/list.json" => data_ok(json!({
            "status": "000", "message": "정상", "page_no": 1, "page_count": 100,
            "total_count": 1, "total_page": 1, "list": [{
                "corp_cls": "Y", "corp_name": "시험회사", "corp_code": CORP_HYNIX, "stock_code": HYNIX,
                "report_nm": "주요사항보고서(자기주식취득결정)", "rcept_no": "20261116000057",
                "flr_nm": "시험회사", "rcept_dt": "20261116", "rm": "유"
            }]
        })),
        "/api/fnlttSinglAcnt.json" => data_ok(json!({
            "status": "000", "message": "정상",
            "list": [{
                "rcept_no": "20260310000789", "reprt_code": "11011", "bsns_year": "2025",
                "corp_code": CORP_HYNIX, "stock_code": "000000", "fs_div": "CFS", "fs_nm": "연결재무제표",
                "sj_div": "IS", "sj_nm": "손익계산서", "account_nm": "매출액", "thstrm_nm": "제 57 기",
                "thstrm_dt": "2025.01.01 ~ 2025.12.31", "thstrm_amount": "300,870,903,000,000",
                "frmtrm_nm": "제 56 기", "frmtrm_dt": "2024.01.01 ~ 2024.12.31",
                "frmtrm_amount": "1,000", "ord": "21", "currency": "KRW"
            }]
        })),
        "/fred/series/observations" => data_ok(json!({
            "observations": [{"date": "2026-11-12", "value": "4.10"}]
        })),
        "/v1/search/news.json" => data_ok(json!({
            "lastBuildDate": "Mon, 16 Nov 2026 10:00:00 +0900",
            "total": 0, "start": 1, "display": 0, "items": []
        })),
        _ if path.contains("/722Y001/") => data_ok(ecos_body(
            "722Y001",
            "0101000",
            "한국은행 기준금리",
            "연%",
            "202610",
            "2.25",
        )),
        _ if path.contains("/731Y001/") => data_ok(ecos_body(
            "731Y001",
            "0000001",
            "원/미국달러",
            "원",
            "20261113",
            "1391.5",
        )),
        _ => ResponseTemplate::new(404),
    }
}

async fn fake_data() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(data_reply)
        .mount(&server)
        .await;
    server
}

/// 가짜 자료 서버를 쓰는 팀 조립 값(공식 팀 — 모델은 기본값 · 자료 주기 1시간 · [`HYNIX`]의 고유번호).
fn team_wiring(data: &MockServer) -> TeamWiring {
    let mut team = TeamWiring::official();
    let uri = data.uri();
    team.data = DataConfig::new(uri.clone(), uri.clone(), uri.clone());
    team.data.timeout = Duration::from_secs(3);
    team.corp_codes = HashMap::from([(Symbol::new(HYNIX), CorpCode::parse(CORP_HYNIX).unwrap())]);
    team.news = NaverNewsConfig::new(uri).with_timeout(Duration::from_secs(3));
    team.data_poll = Duration::from_secs(3600);
    team
}

// ───────────── 조립 ─────────────

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

/// 주문 길 조립: 모의계좌 · 국내 달력, 업종표에 [`HYNIX`]만("반도체").
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
    let mut rules = RulesConfig::default();
    rules
        .sectors
        .insert(Symbol::new(HYNIX), "반도체".to_string());
    TradingWiring {
        paper,
        rules,
        calendar: Some(calendar),
    }
}

/// 설정: 그 보관 폴더 · 빈 포트 · 짧은 시세 간격 · 매매 종목(시연 아님).
pub fn settings(home: &Path, port: u16) -> Settings {
    let port = port.to_string();
    let args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--port",
        &port,
        "--interval-ms",
        "20",
        "--retry-initial-ms",
        "20",
        "--retry-max-ms",
        "40",
        "--symbols",
        HYNIX,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    Settings::parse(&args, &|_| None, None).expect("설정을 읽어야 한다")
}

/// 다른 기기로 볼 상대 주소(사설 대역).
const REMOTE_PEER: [u8; 4] = [100, 64, 0, 7];

fn peer_override(remote: Arc<AtomicBool>) -> pi_gate::PeerOverride {
    Arc::new(move |addr: SocketAddr| {
        if remote.load(Ordering::SeqCst) {
            SocketAddr::from((REMOTE_PEER, addr.port()))
        } else {
            addr
        }
    })
}

/// 가짜 증권사(토스): 토큰 · 현재가(시계의 지금 시각으로 [`HYNIX`] 한 종목) · 국내 달력.
async fn fake_broker(clock: Arc<TestClock>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "zz-round57-quote-token", "token_type": "Bearer", "expires_in": 86_400
        })))
        .mount(&server)
        .await;
    let price_clock = Arc::clone(&clock);
    Mock::given(method("GET"))
        .and(path("/api/v1/prices"))
        .respond_with(move |_: &Request| {
            let now = price_clock.now().with_timezone(&seoul());
            ResponseTemplate::new(200).set_body_json(json!({"result": [{
                "symbol": HYNIX, "timestamp": now.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string(),
                "lastPrice": "100000", "currency": "KRW"
            }]}))
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/market-calendar/KR"))
        .respond_with(move |_: &Request| {
            let today = clock.now().with_timezone(&seoul()).date_naive();
            ResponseTemplate::new(200).set_body_json(json!({"result": {
                "today": toss_day(today),
                "previousBusinessDay": toss_day(business_day(today, false)),
                "nextBusinessDay": toss_day(business_day(today, true)),
            }}))
        })
        .mount(&server)
        .await;
    server
}

/// 토스 달력 응답의 하루(round17과 같은 모양).
fn toss_day(date: NaiveDate) -> Value {
    let date = date.format("%Y-%m-%d").to_string();
    json!({
        "date": date,
        "integrated": {
            "preMarket": {
                "startTime": format!("{date}T08:00:00+09:00"),
                "singlePriceAuctionStartTime": format!("{date}T08:50:00+09:00"),
                "endTime": format!("{date}T09:00:00+09:00"),
            },
            "regularMarket": {
                "startTime": format!("{date}T09:00:00+09:00"),
                "singlePriceAuctionStartTime": format!("{date}T15:20:00+09:00"),
                "endTime": format!("{date}T15:30:00+09:00"),
            },
            "afterMarket": {
                "startTime": format!("{date}T15:30:00+09:00"),
                "singlePriceAuctionEndTime": format!("{date}T15:40:00+09:00"),
                "endTime": format!("{date}T20:00:00+09:00"),
            },
        }
    })
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

    async fn verify_provider(
        &self,
        _keys: ProviderKeys,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

/// 필수 키(증권사 · AI)와 자료 키를 시험용 값으로 등록한 새 보관 폴더.
pub async fn fresh_home() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let secrets = SecretStore::open(dir.path()).unwrap();
    let broker = Credentials::new()
        .with("client_id", CLIENT_ID)
        .with("client_secret", CLIENT_SECRET);
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", AI_KEY);
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    for (slot, value) in [
        (KeySlot::Dart, "zz-round57-dart-key-0004"),
        (KeySlot::Ecos, "zz-round57-ecos-key-0005"),
    ] {
        let values = Credentials::new().with(slot.fields()[0], value);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    dir
}

// ───────────── 띄운 서버 ─────────────

/// 다시 띄워도 그대로 쓰는 바깥 자리.
struct Parts {
    clock: Arc<TestClock>,
    remote: Arc<AtomicBool>,
    broker: MockServer,
    ai: FakeAi,
    data: MockServer,
}

fn wiring(parts: &Parts) -> Wiring {
    Wiring {
        upstreams: Upstreams {
            toss: TossConfig {
                base_url: parts.broker.uri(),
                timeout: Duration::from_secs(3),
                token_failure_cooldown: Duration::ZERO,
                ..TossConfig::default()
            },
            anthropic: AnthropicConfig {
                base_url: parts.ai.uri(),
                timeout: Duration::from_secs(5),
                max_retries: 0,
                retry_base_delay: Duration::from_millis(1),
            },
        },
        clock: Arc::clone(&parts.clock) as Arc<dyn Clock>,
        peer_override: Some(peer_override(Arc::clone(&parts.remote))),
        cli_locations: Vec::new(),
    }
}

async fn launch(home: &Path, parts: &Parts) -> App {
    let launch = Launch {
        trading: trading_wiring(),
        team: Some(team_wiring(&parts.data)),
        secrets: None,
    };
    tokio::time::timeout(STEP, App::launch(settings(home, 0), wiring(parts), launch))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 바깥 자리.
pub struct Bench {
    dir: TempDir,
    parts: Parts,
    app: App,
}

impl Bench {
    /// 새 보관 폴더(필수 키 등록)로 월요일 10시에 띄운다.
    pub async fn start() -> Bench {
        Bench::start_in(fresh_home().await).await
    }

    /// 그 보관 폴더로 월요일 10시에 띄운다(띄우기 전에 보관 폴더에 파일을 둘 때).
    pub async fn start_in(dir: TempDir) -> Bench {
        let clock = TestClock::at(kst(monday(), 10, 0));
        let parts = Parts {
            remote: Arc::new(AtomicBool::new(false)),
            broker: fake_broker(Arc::clone(&clock)).await,
            ai: FakeAi::start().await,
            data: fake_data().await,
            clock,
        };
        let app = launch(dir.path(), &parts).await;
        Bench { dir, parts, app }
    }

    /// 끄고, 같은 보관 폴더 · 같은 바깥 자리로 다시 띄운다(상대 주소는 그 컴퓨터로 돌린다).
    pub async fn restart(self) -> Bench {
        let Bench { dir, parts, app } = self;
        tokio::time::timeout(STEP, app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        parts.remote.store(false, Ordering::SeqCst);
        let app = launch(dir.path(), &parts).await;
        Bench { dir, parts, app }
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }

    pub fn home(&self) -> &Path {
        self.dir.path()
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    /// 가짜 AI 서버.
    pub fn ai(&self) -> &FakeAi {
        &self.parts.ai
    }

    /// 이어 띄운 투자 팀.
    pub fn crew(&self) -> Arc<Crew> {
        self.app.crew().expect("발판은 팀을 이어 띄운다")
    }

    /// [`HYNIX`] 종목 논의를 한 번 돌린다(끝날 때까지 기다린다).
    pub async fn discuss(&self, topic: &str) {
        let crew = self.crew();
        let done = tokio::time::timeout(STEP, crew.discuss(HYNIX, topic)).await;
        assert!(done.is_ok(), "논의가 시간 안에 끝나야 한다");
    }

    /// 다른 기기의 원격 세션을 받는다: 접속 키를 두고, 다른 기기로 보이게 한 뒤 그 키로 들어간다.
    /// `Cookie` 헤더 글자를 돌려준다. 돌려준 뒤에도 다른 기기로 보인다.
    pub async fn remote_login(&self) -> String {
        let salt = vec![7_u8; 16];
        let mut hasher = Sha256::new();
        hasher.update(&salt);
        hasher.update(ACCESS_KEY.as_bytes());
        let hash = AccessKeyHash {
            sha256: hasher.finalize().into(),
            salt,
        };
        self.app
            .store()
            .set_access_key(hash, UnixMillis(0))
            .expect("접속 키를 둔다");
        self.parts.remote.store(true, Ordering::SeqCst);
        let origin = origin_of(self.addr());
        let request = reqwest::Client::new()
            .post(format!("{origin}{}", pi_gate::SESSION_PATH))
            .header("Origin", &origin)
            .header("Content-Type", "application/json")
            .body(json!({ pi_gate::ACCESS_KEY_FIELD: ACCESS_KEY }).to_string());
        let reply = tokio::time::timeout(STEP, request.send())
            .await
            .expect("응답이 시간 안에 와야 한다")
            .expect("세션 요청이 닿아야 한다");
        assert!(reply.status().is_success(), "원격 세션을 받아야 한다");
        cookie_of(&reply)
    }
}

/// 다른 기기의 접속 키(시험용, 글자 수 · 글자 모음은 `pi_gate`의 것).
const ACCESS_KEY: &str = "0123456789ABCDEFGHJK";

// ───────────── 기다리기 ─────────────

/// 조건이 될 때까지 기다린다(가장 길게 [`STEP`]).
pub async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + STEP;
    while !done() {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
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

fn cookie_of(reply: &reqwest::Response) -> String {
    let raw = reply
        .headers()
        .get("set-cookie")
        .expect("세션 쿠키가 있어야 한다")
        .to_str()
        .unwrap();
    raw.split(';').next().unwrap().trim().to_string()
}

/// 같은 출처에서 (그 컴퓨터의) 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
pub async fn login(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let reply = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    assert!(reply.status().is_success(), "세션을 받아야 한다");
    cookie_of(&reply)
}

/// 그 세션으로 읽는다.
pub async fn get_with(addr: SocketAddr, path: &str, cookie: &str) -> Reply {
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    send(request).await
}

/// 같은 출처(`Origin`)에서 그 세션으로 JSON 글자를 `POST`한다.
pub async fn post_raw_with(addr: SocketAddr, path: &str, cookie: &str, body: &str) -> Reply {
    let origin = origin_of(addr);
    let request = reqwest::Client::new()
        .post(format!("{origin}{path}"))
        .header("Origin", &origin)
        .header("Cookie", cookie)
        .header("Content-Type", "application/json")
        .body(body.to_string());
    send(request).await
}

/// 세션을 받아 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    get_with(addr, path, &cookie).await
}

/// 세션을 받아 같은 출처에서 JSON을 `POST`한다.
pub async fn post(addr: SocketAddr, path: &str, body: &Value) -> Reply {
    let cookie = login(addr).await;
    post_raw_with(addr, path, &cookie, &body.to_string()).await
}

/// 객체의 칸 이름들(정렬).
pub fn keys_of(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

/// 담당 id 여섯 → 모델 이름으로 바꾸기 몸통(`{"roles": [{"agent", "model"}]}`). `changed`에 없는 담당은 `rest`.
pub fn models_body(changed: &[(&str, &str)], rest: &str) -> Value {
    let roles: Vec<Value> = pi_agent::Role::ALL
        .iter()
        .map(|role| {
            let model = changed
                .iter()
                .find(|(agent, _)| *agent == role.id())
                .map_or(rest, |(_, model)| *model);
            json!({ "agent": role.id(), "model": model })
        })
        .collect();
    json!({ "roles": roles })
}

/// 읽기 답의 `roles`에서 담당 → 모델.
pub fn shown_models(body: &Value) -> Vec<(String, String)> {
    body["roles"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    (
                        row["agent"].as_str().unwrap_or_default().to_string(),
                        row["model"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}
