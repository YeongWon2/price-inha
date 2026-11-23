//! round18 테스트가 함께 쓰는 도우미: 시계 · 달력 · 모의계좌 설정 · 가짜 서버(자료 · 뉴스 · 시세 · AI) ·
//! 서버 띄우기 · HTTP · 매매. round17의 공용 장치를 본떠 두고(그 폴더는 고치지 않는다) 10b에 맞게 넓혔다.
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0`으로 띄운다([`price_inha::App::launch_with`]). 시연 모드의 AI는
//! 시연용 가짜 공급자이고, 시연이 아닌 모드는 로컬 가짜 시세 서버(종목별 값을 테스트가 정한다)와 로컬 가짜
//! AI 서버(wiremock, 받은 요청을 센다)를 쓴다. 자료(DART · ECOS · FRED) · 네이버 뉴스도 로컬 가짜 서버 하나다.
//! 지켜볼 종목은 `--symbols`가 아니라 보관 폴더의 저장된 목록으로 준다(화면에서 바꿀 수 있게).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeZone, Utc, Weekday};
use pi_ai::AnthropicConfig;
use pi_approval::{Approvals, WaitTimes};
use pi_data::{CorpCode, DataConfig, NAVER_NEWS_KEYS, NaverNewsConfig};
use pi_market::{
    Clock, Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, Symbol,
    TossConfig, TradingDay,
};
use pi_paper::{
    Broker, InitialCash, LimitFillPrice, MarketRules, MaxFill, MoneyRounding, Order, OrderGroup,
    OrderType, PaperConfig, ProceedsUsable, RoundingMode, SessionRules, Settlement, Side, TickBand,
};
use pi_rules::{
    Funding, OrderDraft, ProductKind, Purpose, Rationale, RulesConfig, SourceStamp, SymbolStatus,
    TradeStage,
};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::{
    AccountKind, Amount, Judgment, NewTrade, RoleScore, SourceRef, Store, TeamCause, TeamChange,
    TeamState, TradeId, TradeState, UnixMillis,
};
use price_inha::{
    App, CalendarFn, Crew, DEFAULT_DART_DAILY_LIMIT, DayPlan, DemoTurn, Launch, OrderRequest,
    Placed, Reach, Settings, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TeamWiring, Trading,
    TradingWiring, Upstreams, Wiring, save_watchlist,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 지켜볼 종목.
pub const WATCHED: &str = "005930";
/// 논의 · 매매하는 종목.
pub const HYNIX: &str = "000660";
/// 가짜 시세 서버가 [`WATCHED`]에 처음 주는 값(원).
pub const WATCHED_PRICE: i64 = 72_000;
const CORP_WATCHED: &str = "00126380";
const CORP_HYNIX: &str = "00164779";
const DART_KEY: &str = "test-dart-key-round18-zz";
const ECOS_KEY: &str = "test-ecos-key-round18-zz";
/// 시험용 FRED 키 값.
pub const FRED_KEY: &str = "test-fred-key-round18-zz";
/// 시험용 네이버 키 값(진짜 키처럼 보이지 않는다).
pub const NAVER_ID: &str = "test-naver-id-round18";
pub const NAVER_SECRET: &str = "test-naver-secret-round18";
/// 매매 근거의 진입가 · 손절값 · 목표값 · 주당 기대값(원).
pub const ENTRY: i64 = 100_000;
pub const STOP: i64 = 95_000;
pub const TARGET: i64 = 110_000;
const EDGE: i64 = 5_000;
/// 자료 가짜 서버가 주는 공시의 이름(접수번호마다 같다).
pub const DISCLOSURE_NAME: &str = "주요사항보고서(자기주식취득결정)";
/// FRED 관측값 경로.
pub const FRED_PATH: &str = "/fred/series/observations";
/// 네이버 뉴스 검색 경로.
pub const NEWS_PATH: &str = "/v1/search/news.json";

/// 글자를 십진수로.
pub fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

/// 팀장의 판단(근거 칸이 다 찬 것). `decision`은 `"buy"` 또는 `"sell"`.
pub fn verdict(decision: &str) -> String {
    json!({
        "decision": decision, "hypothesis": "자사주 매입으로 수급 개선", "trigger": "자기주식취득결정 공시",
        "entry": ENTRY, "stop": STOP, "target": TARGET, "expected_value": EDGE
    })
    .to_string()
}

/// 잠금을 쥔 채 죽은 스레드가 있어도 값을 읽는다.
fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
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

    fn read(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0.load(Ordering::SeqCst))
            .unwrap()
    }
}

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        self.read()
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration.min(Duration::from_millis(10))).await;
    }
}

/// 승인 모듈이 읽는 같은 시계.
struct ApprovalClock(Arc<TestClock>);

impl pi_approval::Clock for ApprovalClock {
    fn now(&self) -> UnixMillis {
        ms(self.0.read())
    }
}

/// 2026-11-16 월요일.
pub fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
}

/// 2026-11-17 화요일.
pub fn tuesday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 17).unwrap()
}

fn seoul() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).unwrap()
}

/// 그 날의 한국 시각.
pub fn kst(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    seoul()
        .from_local_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

/// `seconds`초 뒤.
pub fn plus(at: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
    at + chrono::Duration::seconds(seconds)
}

/// 유닉스 밀리초.
pub fn ms(at: DateTime<Utc>) -> UnixMillis {
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

/// 그 날의 국내 세션(정규장 09:00~15:30). 주말은 휴장(세션 없음)이다.
fn kr_day(date: NaiveDate) -> TradingDay {
    let sessions = if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        Vec::new()
    } else {
        vec![Session {
            kind: SessionKind::Regular,
            start: kst(date, 9, 0),
            end: kst(date, 15, 30),
        }]
    };
    TradingDay { date, sessions }
}

fn kr_calendar(date: NaiveDate) -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        today: kr_day(date),
        previous_business_day: kr_day(business_day(date, false)),
        next_business_day: kr_day(business_day(date, true)),
    }
}

// ───────────── 자료 · 뉴스 가짜 서버 ─────────────

/// 자료 가짜 서버의 상태(테스트가 도중에 바꾼다).
struct DataState {
    /// 참이면 모든 요청에 500(출처 장애).
    failing: AtomicBool,
    /// 공시 목록의 항목들(`corp_code`로 거른다). 비면 DART `013`(없음).
    disclosures: Mutex<Vec<Value>>,
    /// 네이버 뉴스 검색의 기사들.
    news: Mutex<Vec<Value>>,
    /// 네이버 뉴스 검색의 상태 번호(200 또는 401).
    news_status: AtomicU16,
    /// FRED 미국 금리 관측값.
    fred: Mutex<String>,
    /// ECOS 한국은행 기준금리 값.
    korea_rate: Mutex<String>,
    /// DART 재무(매출액)의 이번 기 금액.
    revenue: Mutex<String>,
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn financials_body(corp: &str, revenue: &str) -> Value {
    json!({
        "status": "000", "message": "정상",
        "list": [{
            "rcept_no": "20260310000789", "reprt_code": "11011", "bsns_year": "2025",
            "corp_code": corp, "stock_code": "000000", "fs_div": "CFS", "fs_nm": "연결재무제표",
            "sj_div": "IS", "sj_nm": "손익계산서", "account_nm": "매출액", "thstrm_nm": "제 57 기",
            "thstrm_dt": "2025.01.01 ~ 2025.12.31", "thstrm_amount": revenue,
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

fn query_of(request: &Request, name: &str) -> Option<String> {
    request
        .url
        .query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

/// 경로로 나눠 답한다: DART 공시 · 재무, ECOS 금리 · 환율, FRED 관측값, 네이버 뉴스 검색.
fn data_reply(state: &DataState, request: &Request) -> ResponseTemplate {
    if state.failing.load(Ordering::SeqCst) {
        return ResponseTemplate::new(500);
    }
    let path = request.url.path();
    let corp = query_of(request, "corp_code").unwrap_or_default();
    match path {
        "/api/list.json" => {
            let list: Vec<Value> = locked(&state.disclosures)
                .iter()
                .filter(|item| item["corp_code"] == json!(corp))
                .cloned()
                .collect();
            if list.is_empty() {
                return ok(json!({"status": "013", "message": "조회된 데이타가 없습니다."}));
            }
            ok(json!({
                "status": "000", "message": "정상", "page_no": 1, "page_count": 100,
                "total_count": list.len(), "total_page": 1, "list": list
            }))
        }
        "/api/fnlttSinglAcnt.json" => ok(financials_body(&corp, &locked(&state.revenue))),
        FRED_PATH => ok(json!({
            "observations": [{"date": "2026-11-12", "value": *locked(&state.fred)}]
        })),
        NEWS_PATH => {
            let status = state.news_status.load(Ordering::SeqCst);
            if status != 200 {
                return ResponseTemplate::new(status).set_body_json(json!({
                    "errorMessage": "Authentication failed", "errorCode": "024"
                }));
            }
            let items = locked(&state.news).clone();
            ok(json!({
                "lastBuildDate": "Mon, 16 Nov 2026 10:00:00 +0900",
                "total": items.len(), "start": 1, "display": items.len(), "items": items
            }))
        }
        _ if path.contains("/722Y001/") => ok(ecos_body(
            "722Y001",
            "0101000",
            "한국은행 기준금리",
            "연%",
            "202610",
            &locked(&state.korea_rate),
        )),
        _ if path.contains("/731Y001/") => ok(ecos_body(
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

/// 자료 가짜 서버(DART · ECOS · FRED · 네이버 뉴스 한 곳). 처음에는 공시 · 뉴스가 없다.
pub struct DataServer {
    server: MockServer,
    state: Arc<DataState>,
}

impl DataServer {
    async fn start() -> DataServer {
        let server = MockServer::start().await;
        let state = Arc::new(DataState {
            failing: AtomicBool::new(false),
            disclosures: Mutex::new(Vec::new()),
            news: Mutex::new(Vec::new()),
            news_status: AtomicU16::new(200),
            fred: Mutex::new("4.10".to_string()),
            korea_rate: Mutex::new("2.25".to_string()),
            revenue: Mutex::new("300,870,903,000,000".to_string()),
        });
        let reply_state = Arc::clone(&state);
        Mock::given(method("GET"))
            .respond_with(move |request: &Request| data_reply(&reply_state, request))
            .mount(&server)
            .await;
        DataServer { server, state }
    }

    /// 그 종목의 공시 하나를 더한다(접수일은 월요일).
    pub fn disclose(&self, symbol: &str, receipt: &str) {
        let corp = if symbol == WATCHED {
            CORP_WATCHED
        } else {
            CORP_HYNIX
        };
        locked(&self.state.disclosures).push(json!({
            "corp_cls": "Y", "corp_name": "시험회사", "corp_code": corp, "stock_code": symbol,
            "report_nm": DISCLOSURE_NAME, "rcept_no": receipt,
            "flr_nm": "시험회사", "rcept_dt": "20261116", "rm": "유"
        }));
    }

    /// 네이버 뉴스 검색이 줄 기사들을 바꾼다.
    pub fn set_news(&self, items: Vec<Value>) {
        *locked(&self.state.news) = items;
    }

    /// 네이버가 키를 거절하게 한다(401).
    pub fn reject_news_keys(&self) {
        self.state.news_status.store(401, Ordering::SeqCst);
    }

    /// FRED 미국 금리 관측값을 바꾼다(처음 `4.10`).
    pub fn set_fred(&self, value: &str) {
        *locked(&self.state.fred) = value.to_string();
    }

    /// ECOS 한국은행 기준금리 값을 바꾼다(처음 `2.25`).
    pub fn set_korea_rate(&self, value: &str) {
        *locked(&self.state.korea_rate) = value.to_string();
    }

    /// DART 재무(매출액)의 이번 기 금액을 바꾼다(처음 `300,870,903,000,000`).
    pub fn set_revenue(&self, amount: &str) {
        *locked(&self.state.revenue) = amount.to_string();
    }

    /// 모든 자료 요청이 500으로 실패하게 바꾼다(출처 장애).
    pub fn fail(&self) {
        self.state.failing.store(true, Ordering::SeqCst);
    }

    /// 이 가짜 서버의 주소.
    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// 이 가짜 서버가 받은, 그 경로의 요청들.
    pub async fn requests(&self, path: &str) -> Vec<Request> {
        let received = self.server.received_requests().await.unwrap_or_default();
        received
            .into_iter()
            .filter(|request| request.url.path() == path)
            .collect()
    }

    /// 이 가짜 서버가 받은, 경로에 `part`가 든 요청들(ECOS처럼 경로에 키 · 기간이 실리는 것).
    pub async fn requests_like(&self, part: &str) -> Vec<Request> {
        let received = self.server.received_requests().await.unwrap_or_default();
        received
            .into_iter()
            .filter(|request| request.url.path().contains(part))
            .collect()
    }

    /// 이 가짜 서버를 쓰는 팀 조립 값.
    fn team(&self, plan: &Plan) -> TeamWiring {
        let mut team = TeamWiring::official();
        let uri = self.server.uri();
        team.data = DataConfig::new(uri.clone(), uri.clone(), uri.clone());
        team.data.timeout = Duration::from_secs(3);
        team.corp_codes = HashMap::from([
            (Symbol::new(WATCHED), CorpCode::parse(CORP_WATCHED).unwrap()),
            (Symbol::new(HYNIX), CorpCode::parse(CORP_HYNIX).unwrap()),
        ]);
        // 그 밖의 지켜볼 종목에도 고유번호를 준다(가짜 서버는 그 번호의 공시가 없다고 답한다).
        for symbol in &plan.symbols {
            let code = CorpCode::parse(&format!("90{symbol}")).unwrap();
            team.corp_codes.entry(Symbol::new(*symbol)).or_insert(code);
        }
        team.news = NaverNewsConfig::new(uri).with_timeout(Duration::from_secs(3));
        team.data_poll = plan.poll;
        team.news_daily_limit = plan.news_limit;
        team.dart_daily_limit = plan.dart_limit;
        team
    }
}

/// 네이버 뉴스 검색의 기사 하나(월요일 09:30에 난 것). `link`는 `originallink` · `link` 둘 다에 넣는다.
pub fn article(title: &str, summary: &str, link: &str) -> Value {
    json!({
        "title": title, "description": summary, "originallink": link, "link": link,
        "pubDate": "Mon, 16 Nov 2026 09:30:00 +0900"
    })
}

// ───────────── 시세 · AI 가짜 서버(시연이 아닌 모드) ─────────────

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

/// 종목별 값(원). 없는 종목은 물어도 답에 싣지 않는다(그 종목의 시세가 끊긴다).
type Prices = Arc<Mutex<HashMap<String, i64>>>;

/// 가짜 시세 서버: 토큰 · 현재가(물은 종목 가운데 값이 있는 것만, 시계의 지금 시각으로) · 국내 달력.
async fn fake_quotes(clock: Arc<TestClock>, prices: Prices) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "test-quote-token-not-real", "token_type": "Bearer", "expires_in": 86_400
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(move |request: &Request| {
            let now = clock.read().with_timezone(&seoul());
            if request.url.path() == "/api/v1/market-calendar/KR" {
                let today = now.date_naive();
                return ok(json!({"result": {
                    "today": toss_day(today),
                    "previousBusinessDay": toss_day(business_day(today, false)),
                    "nextBusinessDay": toss_day(business_day(today, true)),
                }}));
            }
            let asked = query_of(request, "symbols").unwrap_or_default();
            let known = locked(&prices).clone();
            let stamp = now.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string();
            let result: Vec<Value> = asked
                .split(',')
                .filter_map(|symbol| {
                    let price = known.get(symbol)?;
                    Some(json!({
                        "symbol": symbol, "timestamp": stamp,
                        "lastPrice": price.to_string(), "currency": "KRW"
                    }))
                })
                .collect();
            ok(json!({ "result": result }))
        })
        .mount(&server)
        .await;
    server
}

/// 뉴스 공시 시세 담당이 논의 턴이 아닌 턴(깨우기)에 할 답. 없으면 기본 답.
type ScoutReply = Arc<Mutex<Option<String>>>;

/// 요청 글로 턴을 골라 답한다(시연용 가짜 AI와 같은 규칙): 팀장은 언제나 사기로 판단한다.
/// 뉴스 공시 시세 담당의 논의 턴이 아닌 턴은 `scout`이 정해져 있으면 그 글로 답한다.
fn ai_reply(system: &str, last: &str, scout: Option<String>) -> String {
    let role = system
        .split("담당 id: ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or("leader");
    if last.contains("[하루 판단]") {
        json!({"plan": "trade"}).to_string()
    } else if last.contains("[복기]") {
        json!({"mistakes": [], "note": "시험 복기"}).to_string()
    } else if last.contains("[점수]") {
        json!({"score": 70, "reason": "시험 점수"}).to_string()
    } else if last.contains("판단을 JSON 객체 하나로") {
        verdict("buy")
    } else if last.contains("[주문 안]") {
        json!({"side": "buy", "note": "시험 주문 안"}).to_string()
    } else if let Some(reply) = scout.filter(|_| role == "scout" && !last.contains("라운드다"))
    {
        reply
    } else {
        match role {
            "scout" => "새 자료를 확인했다. @strategy @risk 의견을 부탁한다".to_string(),
            "strategy" => "전략상 진입할 만하다.".to_string(),
            "risk" => "손절 기준을 지키면 감당할 만하다.".to_string(),
            _ => "(pass)".to_string(),
        }
    }
}

/// 가짜 AI 서버: 대화 요청(`POST /v1/messages`)에 [`ai_reply`]의 글 하나로 답한다.
async fn fake_ai(scout: ScoutReply) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
            let system = body["system"].as_str().unwrap_or_default();
            let last = body["messages"]
                .as_array()
                .and_then(|messages| messages.iter().rev().find(|m| m["role"] == json!("user")))
                .and_then(|message| message["content"].as_str())
                .unwrap_or_default();
            ok(json!({
                "id": "msg_test", "type": "message", "role": "assistant", "model": "test-model",
                "content": [{"type": "text", "text": ai_reply(system, last, locked(&scout).clone())}],
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 1, "output_tokens": 1}
            }))
        })
        .mount(&server)
        .await;
    server
}

/// 시연이 아닌 모드의 가짜 바깥 서버들. 다시 켜도 그대로 쓴다.
struct Live {
    quotes: MockServer,
    ai: MockServer,
    prices: Prices,
    scout: ScoutReply,
}

// ───────────── 서버 띄우기 ─────────────

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

/// 설정: 임시 보관 폴더 · 빈 포트 · 짧은 시세 간격과 다시 시도 간격. 종목은 저장된 목록에서 읽는다.
fn settings(home: &Path, demo: bool) -> Settings {
    let home = home.to_str().unwrap();
    let mut list = vec![
        "--home",
        home,
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--retry-initial-ms",
        "20",
        "--retry-max-ms",
        "40",
    ];
    if demo {
        list.push("--demo");
    }
    let args: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// 바깥 자리를 정한 조립 값. 주소가 없으면 아무도 듣지 않는 로컬 주소다.
fn wiring(clock: Arc<TestClock>, live: Option<&Live>) -> Wiring {
    let quotes = live.map_or_else(dead_base_url, |live| live.quotes.uri());
    let ai = live.map_or_else(dead_base_url, |live| live.ai.uri());
    Wiring {
        upstreams: Upstreams {
            toss: TossConfig {
                base_url: quotes,
                timeout: Duration::from_secs(3),
                token_failure_cooldown: Duration::ZERO,
                ..TossConfig::default()
            },
            anthropic: AnthropicConfig {
                base_url: ai,
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

    async fn verify_provider(
        &self,
        _keys: ProviderKeys,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

/// 네이버 뉴스 키 칸 선언(보관소 쪽).
pub fn naver_keys() -> ProviderKeys {
    NAVER_NEWS_KEYS.provider_keys()
}

/// 띄우기 전 보관 폴더에 키를 등록하고 지켜볼 종목을 저장한다. 값은 진짜 키처럼 보이지 않는다.
async fn prepare(home: &Path, plan: &Plan) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", "test-client-id-round18")
        .with("client_secret", "test-client-secret-round18");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "test-ai-key-round18-zz");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    let mut slots = Vec::new();
    if plan.data_keys {
        slots.extend([(KeySlot::Dart, DART_KEY), (KeySlot::Ecos, ECOS_KEY)]);
    }
    if plan.fred {
        slots.push((KeySlot::Fred, FRED_KEY));
    }
    for (slot, value) in slots {
        let values = Credentials::new().with(slot.fields()[0], value);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    if plan.naver {
        let values = Credentials::new()
            .with("client_id", NAVER_ID)
            .with("client_secret", NAVER_SECRET);
        secrets
            .register_provider(naver_keys(), values, &AcceptAll)
            .await
            .unwrap();
    }
    let symbols: Vec<Symbol> = plan.symbols.iter().map(|s| Symbol::new(*s)).collect();
    save_watchlist(home, &symbols).unwrap();
}

/// 띄울 모양: 시연인가 · 지켜볼 종목 · 어떤 키를 등록하나 · 자료 간격 · 네이버 · DART 하루 상한.
pub struct Plan {
    live: bool,
    symbols: Vec<&'static str>,
    data_keys: bool,
    fred: bool,
    naver: bool,
    poll: Duration,
    news_limit: u32,
    dart_limit: u32,
    prices: Vec<(&'static str, i64)>,
}

impl Plan {
    /// 시연 모드, 지켜볼 종목 [`WATCHED`], 자료 키(DART · ECOS) 있음, 자료 간격 1시간.
    pub fn demo() -> Plan {
        Plan {
            live: false,
            symbols: vec![WATCHED],
            data_keys: true,
            fred: false,
            naver: false,
            poll: Duration::from_secs(3600),
            news_limit: 25_000,
            dart_limit: DEFAULT_DART_DAILY_LIMIT,
            prices: Vec::new(),
        }
    }

    /// 시연이 아닌 모드(로컬 가짜 시세 · AI 서버). 나머지는 [`Plan::demo`]와 같다.
    pub fn live() -> Plan {
        Plan {
            live: true,
            ..Plan::demo()
        }
    }

    /// 지켜볼 종목(저장된 목록).
    pub fn watching(self, symbols: &[&'static str]) -> Plan {
        Plan {
            symbols: symbols.to_vec(),
            ..self
        }
    }

    /// 띄우기 전에 가짜 시세 서버에 그 종목의 값을 넣는다([`WATCHED`] 말고도).
    pub fn priced(self, symbol: &'static str, price: i64) -> Plan {
        let mut prices = self.prices;
        prices.push((symbol, price));
        Plan { prices, ..self }
    }

    /// 자료 키(DART · ECOS)를 등록하지 않는다(자료 키는 선택이다).
    pub fn without_data_keys(self) -> Plan {
        Plan {
            data_keys: false,
            ..self
        }
    }

    /// FRED 키를 등록한다.
    pub fn with_fred(self) -> Plan {
        Plan { fred: true, ..self }
    }

    /// 네이버 뉴스 키를 등록한다.
    pub fn with_naver(self) -> Plan {
        Plan {
            naver: true,
            ..self
        }
    }

    /// 자료를 다시 보는 간격(시계 기준).
    pub fn polling(self, poll: Duration) -> Plan {
        Plan { poll, ..self }
    }

    /// 네이버 뉴스 검색의 하루 상한.
    pub fn news_limit(self, news_limit: u32) -> Plan {
        Plan { news_limit, ..self }
    }

    /// DART 요청의 하루 상한.
    pub fn dart_limit(self, dart_limit: u32) -> Plan {
        Plan { dart_limit, ..self }
    }

    /// 이 모양으로 그 시각에 띄운다.
    pub async fn start(self, at: DateTime<Utc>) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path(), &self).await;
        let clock = TestClock::at(at);
        let live = if self.live {
            let mut initial = HashMap::from([(WATCHED.to_string(), WATCHED_PRICE)]);
            for (symbol, price) in &self.prices {
                initial.insert((*symbol).to_string(), *price);
            }
            let prices: Prices = Arc::new(Mutex::new(initial));
            let quotes = fake_quotes(Arc::clone(&clock), Arc::clone(&prices)).await;
            let scout: ScoutReply = Arc::new(Mutex::new(None));
            Some(Live {
                quotes,
                ai: fake_ai(Arc::clone(&scout)).await,
                prices,
                scout,
            })
        } else {
            None
        };
        let data = DataServer::start().await;
        let app = boot(dir.path(), &self, &clock, live.as_ref(), &data).await;
        Bench {
            dir,
            clock,
            app,
            data,
            live,
            plan: self,
        }
    }
}

async fn boot(
    home: &Path,
    plan: &Plan,
    clock: &Arc<TestClock>,
    live: Option<&Live>,
    data: &DataServer,
) -> App {
    let launch = Launch {
        trading: trading_wiring(),
        team: Some(data.team(plan)),
        secrets: None,
    };
    let reach = Reach {
        push: None,
        backup: None,
    };
    let start = App::launch_with(
        settings(home, !plan.live),
        wiring(Arc::clone(clock), live),
        launch,
        reach,
    );
    tokio::time::timeout(STEP, start)
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 시계 · 가짜 서버들.
pub struct Bench {
    dir: TempDir,
    clock: Arc<TestClock>,
    pub app: App,
    pub data: DataServer,
    live: Option<Live>,
    plan: Plan,
}

impl Bench {
    /// 끈다.
    pub async fn close(self) {
        self.stop().await;
    }

    /// 끄고 보관 폴더를 돌려준다(꺼진 홈을 보는 시험).
    pub async fn stop(self) -> TempDir {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        self.dir
    }

    /// 끄고, 같은 보관 폴더 · 같은 가짜 서버들로 그 시각에 다시 띄운다.
    pub async fn reopen(self, at: DateTime<Utc>) -> Bench {
        let Bench {
            dir,
            clock,
            app,
            data,
            live,
            plan,
        } = self;
        tokio::time::timeout(STEP, app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        clock.set(at);
        let app = boot(dir.path(), &plan, &clock, live.as_ref(), &data).await;
        Bench {
            dir,
            clock,
            app,
            data,
            live,
            plan,
        }
    }

    pub fn home(&self) -> &Path {
        self.dir.path()
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    pub fn trading(&self) -> Arc<Trading> {
        self.app.trading()
    }

    pub fn crew(&self) -> Arc<Crew> {
        self.app.crew().expect("팀을 이어 띄웠다")
    }

    /// 시연용 가짜 AI의 그 턴의 답을 정한다.
    pub fn script(&self, turn: DemoTurn, reply: &str) {
        let ai = self.crew().demo_ai().expect("시연 모드다");
        ai.script(turn, reply);
    }

    /// 시연용 가짜 AI가 받은 글들.
    pub fn prompts(&self) -> Vec<String> {
        let ai = self.crew().demo_ai().expect("시연 모드다");
        ai.prompts()
    }

    fn live(&self) -> &Live {
        self.live.as_ref().expect("시연이 아닌 모드다")
    }

    /// 가짜 시세 서버가 그 종목에 줄 값을 정한다.
    pub fn set_price(&self, symbol: &str, price: i64) {
        locked(&self.live().prices).insert(symbol.to_string(), price);
    }

    /// 가짜 시세 서버가 그 종목을 더 주지 않는다(그 종목의 시세가 끊긴다).
    pub fn drop_price(&self, symbol: &str) {
        locked(&self.live().prices).remove(symbol);
    }

    /// 가짜 시세 서버가 받은 현재가 요청들의 `symbols` 값(받은 순서).
    pub async fn asked_symbols(&self) -> Vec<String> {
        let received = self.live().quotes.received_requests().await;
        received
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path() == "/api/v1/prices")
            .filter_map(|request| query_of(request, "symbols"))
            .collect()
    }

    /// 가짜 AI 서버가 받은 요청 가운데 뉴스 공시 시세 담당(scout)의 것이고 본문에 `needle`이 있는 것의 수.
    pub async fn scout_calls(&self, needle: &str) -> usize {
        let received = self.live().ai.received_requests().await;
        received
            .unwrap_or_default()
            .iter()
            .filter_map(|request| serde_json::from_slice::<Value>(&request.body).ok())
            .filter(|body| {
                let system = body["system"].as_str().unwrap_or_default();
                system.contains("담당 id: scout") && body.to_string().contains(needle)
            })
            .count()
    }

    /// 가짜 AI 서버가 받은 뉴스 공시 시세 담당(scout)의 요청 가운데 마지막 사용자 글에 `needle`이 (대소문자
    /// 없이) 든 것들의 그 글.
    pub async fn scout_inputs(&self, needle: &str) -> Vec<String> {
        let needle = needle.to_lowercase();
        let received = self.live().ai.received_requests().await;
        received
            .unwrap_or_default()
            .iter()
            .filter_map(|request| serde_json::from_slice::<Value>(&request.body).ok())
            .filter(|body| {
                let system = body["system"].as_str().unwrap_or_default();
                system.contains("담당 id: scout")
            })
            .filter_map(|body| {
                let messages = body["messages"].as_array()?;
                let last = messages.iter().rev().find(|m| m["role"] == json!("user"))?;
                last["content"].as_str().map(str::to_string)
            })
            .filter(|input| input.to_lowercase().contains(&needle))
            .collect()
    }

    /// 뉴스 공시 시세 담당이 논의 턴이 아닌 턴(깨우기)에 할 답을 정한다.
    pub fn scout_says(&self, reply: &str) {
        *locked(&self.live().scout) = Some(reply.to_string());
    }

    /// 가짜 AI 서버가 받은 요청에서 팀 논의(`[팀 논의] … · 종목 <코드>`)가 열린 종목들.
    pub async fn discussed_symbols(&self) -> std::collections::BTreeSet<String> {
        const MARK: &str = " · 종목 ";
        let received = self.live().ai.received_requests().await;
        let mut found = std::collections::BTreeSet::new();
        for request in received.unwrap_or_default() {
            let Ok(body) = serde_json::from_slice::<Value>(&request.body) else {
                continue;
            };
            for message in body["messages"].as_array().cloned().unwrap_or_default() {
                let text = message["content"].as_str().unwrap_or_default();
                let Some(start) = text.find("[팀 논의]") else {
                    continue;
                };
                let rest = &text[start..];
                let Some(at) = rest.find(MARK) else {
                    continue;
                };
                let code: String = rest[at + MARK.len()..].chars().take(6).collect();
                found.insert(code);
            }
        }
        found
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn set(&self, at: DateTime<Utc>) {
        self.clock.set(at);
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.read()
    }

    pub fn team(&self) -> TeamState {
        self.store().team_state().unwrap()
    }

    /// 승인 모듈의 눈으로 같은 저장소를 본다(같은 시계).
    pub fn approvals(&self) -> Approvals {
        let clock = Arc::new(ApprovalClock(Arc::clone(&self.clock)));
        Approvals::new(self.store(), clock, WaitTimes::default())
    }

    /// 시계를 그 시각으로 옮기고 그 시각에 받은 시세 하나를 넣는다.
    pub async fn quote(&self, symbol: &str, price: i64, at: DateTime<Utc>) {
        self.set(at);
        let quote = Quote {
            symbol: Symbol::new(symbol),
            price: Decimal::from(price),
            currency: Currency::Krw,
            exchange_time: Some(at),
            received_at: at,
        };
        self.trading()
            .on_quote(quote)
            .await
            .expect("시세를 받아야 한다");
    }

    /// `seconds`초 뒤로 20초씩 나눠 간다(걸음마다 뒤에서 도는 일이 따라오게 잠깐 쉰다).
    pub async fn advance(&self, seconds: i64) {
        let from = self.now();
        let mut gone = 0;
        while gone < seconds {
            gone = (gone + 20).min(seconds);
            self.set(plus(from, gone));
            settle().await;
        }
    }

    pub fn trade_state(&self, id: TradeId) -> TradeState {
        self.store().get_trade(id).unwrap().state
    }

    /// 모의계좌에 낸 그 종목의 파는 주문들(걸린 것과 끝난 것).
    pub async fn sells(&self, symbol: &str) -> Vec<Order> {
        let trading = self.trading();
        let mut orders = Vec::new();
        for group in [OrderGroup::Open, OrderGroup::Closed] {
            let found = Broker::orders(trading.paper(), group, None, self.now())
                .await
                .expect("모의계좌 주문을 읽어야 한다");
            orders.extend(found);
        }
        orders.retain(|order| order.side == Side::Sell && order.symbol.as_str() == symbol);
        orders
    }

    /// 실행기가 다시 내기를 기다리는 그 매매의 파는 주문들(계좌 스냅샷의 `pi_exec.pending`).
    pub fn pending_sells(&self, trade: TradeId) -> Vec<Value> {
        let snapshot = self.store().latest_paper_snapshot().unwrap();
        let Some(snapshot) = snapshot else {
            return Vec::new();
        };
        let pending = snapshot.state["pi_exec"]["pending"].as_array().cloned();
        pending
            .unwrap_or_default()
            .into_iter()
            .filter(|item| item["trade_id"] == json!(trade.0))
            .collect()
    }

    /// 매매하기 좋게 만든다: 스윙 템플릿을 고르고 종목 비중 10%로 맞춘 뒤 오늘 매매하기로 한다.
    pub async fn ready(&self) {
        let reply = post(self.addr(), TEMPLATE_PATH, &json!({"style": "swing"})).await;
        assert_eq!(reply.status, 200, "템플릿을 골라야 한다: {}", reply.body);
        let id = reply.body["id"].as_i64().expect("템플릿 번호");
        let body = json!({"id": id, "changes": {"symbol_weight_pct": "10"}});
        let reply = post(self.addr(), TEMPLATE_ADJUST_PATH, &body).await;
        assert_eq!(reply.status, 200, "상한 안의 값은 저장된다: {}", reply.body);
        self.trade_today();
    }

    /// 오늘 매매하기로 한다(운용 중).
    pub fn trade_today(&self) {
        let state = self.trading().decide_day(DayPlan::Trade).unwrap();
        assert_eq!(state, TeamState::Operating, "매매하기로 하면 운용 중이다");
    }

    /// 운용 중에서 최대 손실로 전체 정지가 된다.
    pub fn full_stop(&self) {
        self.store()
            .change_team_state(TeamChange {
                to: TeamState::FullStop,
                at: ms(self.now()),
                cause: TeamCause::MaxLoss,
                detail: "시험: 최대 손실 도달".to_string(),
                owner_approved: false,
            })
            .expect("운용 중에서 최대 손실로 전체 정지가 된다");
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

    pub async fn place(&self, request: OrderRequest) -> Placed {
        self.trading()
            .place(request)
            .await
            .expect("주문 안을 처리해야 한다")
    }

    /// 지금 시세 [`ENTRY`]로 5주를 사서 보유중까지 만든 매매(에이전트를 거치지 않는다). 시계는 1초 간다.
    pub async fn holding(&self, symbol: &str) -> TradeId {
        let at = self.now();
        self.quote(symbol, ENTRY, at).await;
        let trade = self.judged(symbol);
        let placed = self
            .place(order(trade, symbol, Purpose::NewEntry, ENTRY, at))
            .await;
        assert!(matches!(placed, Placed::Ordered(_)), "산다: {placed:?}");
        self.quote(symbol, ENTRY, plus(at, 1)).await;
        assert_eq!(self.trade_state(trade), TradeState::Holding);
        trade
    }
}

/// 지정가 · 수량 5주의 주문 안(VI · 거래정지는 없다고 안다).
pub fn order(
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

// ───────────── 기다리기 ─────────────

/// 조건이 될 때까지 기다린다(가장 길게 [`STEP`]).
pub async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + STEP;
    while !done() {
        assert!(Instant::now() < deadline, "시간 안에 되지 않았다: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 뒤에서 도는 일(시세 흐름 · 주기 점검 · 깨우기)이 몇 바퀴 돌 만큼 잠깐 쉰다.
pub async fn settle() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// 저장된 이벤트 가운데 그 종류이고 그 날짜인 것의 수.
pub fn on_date(store: &Store, kind: &str, date: NaiveDate) -> usize {
    let date = json!(date.format("%Y-%m-%d").to_string());
    store
        .read_after(0, 100_000)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == kind && event.payload["date"] == date)
        .count()
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

async fn reply_of(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let bytes = reply.bytes().await.expect("본문을 읽어야 한다");
    Reply {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        text: String::from_utf8_lossy(&bytes).into_owned(),
    }
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
    reply_of(request).await
}

/// 세션을 받아 같은 출처에서 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    reply_of(request).await
}
