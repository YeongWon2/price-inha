//! 바깥 주소 자리의 로컬 가짜 서버들: 가짜 증권사(토스 — 토큰 · 현재가 · **오늘 앞뒤 하루만 아는 달력**) · 가짜 AI
//! (Anthropic 대화 요청) · 가짜 자료 서버(DART · ECOS · FRED · 네이버). round42의 발판을 본떴다(그 폴더는 고치지
//! 않는다). 실제 증권사 · AI를 부르지 않는다.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeZone, Utc, Weekday};
use pi_market::Clock;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 매매하는 종목(국내 · 지켜볼 종목 · 시세 흐름이 받는 종목).
pub const HYNIX: &str = "000660";
/// 함께 지켜보는 다른 종목([`HYNIX`]의 시세를 받지 못한 날에도 시세 흐름이 그 날 달력을 받게 한다).
pub const SAMSUNG: &str = "005930";
/// [`HYNIX`]의 DART 고유번호.
pub const CORP_HYNIX: &str = "00164779";
/// 복기 담당의 교훈 글(가짜 AI가 `[복기]` 턴에 주는 `note`).
pub const LESSON: &str = "시험 복기";

// ───────────── 시계 · 달력 ─────────────

/// 시험이 정하는 시계. 자는 것은 짧게만 잔다.
pub struct TestClock(AtomicI64);

impl TestClock {
    pub fn at(time: DateTime<Utc>) -> Arc<Self> {
        Arc::new(Self(AtomicI64::new(time.timestamp_millis())))
    }

    pub fn set(&self, time: DateTime<Utc>) {
        self.0.store(time.timestamp_millis(), Ordering::SeqCst);
    }

    pub fn read(&self) -> DateTime<Utc> {
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

/// 가짜 증권사의 휴장일(주말 말고 달력이 휴장이라 하는 날).
#[derive(Clone, Default)]
pub struct Holidays(Arc<Mutex<HashSet<NaiveDate>>>);

impl Holidays {
    pub fn add(&self, date: NaiveDate) {
        self.0.lock().unwrap().insert(date);
    }

    /// 그 날이 거래일인가(주말 · 휴장일이 아니다).
    pub fn is_open(&self, date: NaiveDate) -> bool {
        !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
            && !self.0.lock().unwrap().contains(&date)
    }

    fn business_day(&self, date: NaiveDate, forward: bool) -> NaiveDate {
        let mut next = date;
        loop {
            next = if forward {
                next.succ_opt().unwrap()
            } else {
                next.pred_opt().unwrap()
            };
            if self.is_open(next) {
                return next;
            }
        }
    }

    /// 토스 달력 응답의 하루(휴장이면 `integrated`가 `null`).
    fn toss_day(&self, date: NaiveDate) -> Value {
        let text = date.format("%Y-%m-%d").to_string();
        if !self.is_open(date) {
            return json!({"date": text, "integrated": null});
        }
        let at = |time: &str| format!("{text}T{time}+09:00");
        json!({
            "date": text,
            "integrated": {
                "regularMarket": {
                    "startTime": at("09:00:00"),
                    "singlePriceAuctionStartTime": at("15:20:00"),
                    "endTime": at("15:30:00"),
                },
            }
        })
    }
}

// ───────────── 가짜 증권사 ─────────────

/// 가짜 증권사(토스): 토큰 · 현재가(시계의 지금 시각으로 물은 종목마다, 값은 `price` — `muted`가 참이면 [`HYNIX`]는
/// 빼고 준다) · 국내 달력. 달력은 실제 증권사처럼 **시계의 오늘과 그 앞뒤 거래일 하루씩만** 준다.
pub async fn fake_broker(
    clock: Arc<TestClock>,
    price: Arc<AtomicI64>,
    muted: Arc<AtomicBool>,
    holidays: Holidays,
) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "zz-round55-quote-token", "token_type": "Bearer", "expires_in": 86_400
        })))
        .mount(&server)
        .await;
    let price_clock = Arc::clone(&clock);
    Mock::given(method("GET"))
        .and(path("/api/v1/prices"))
        .respond_with(move |request: &Request| {
            let now = price_clock.now().with_timezone(&seoul());
            let stamp = now.format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string();
            let asked = request
                .url
                .query_pairs()
                .find(|(key, _)| key == "symbols")
                .map(|(_, value)| value.into_owned())
                .unwrap_or_default();
            let result: Vec<Value> = asked
                .split(',')
                .filter(|symbol| !symbol.is_empty())
                .filter(|symbol| !(muted.load(Ordering::SeqCst) && *symbol == HYNIX))
                .map(|symbol| {
                    json!({
                        "symbol": symbol, "timestamp": stamp,
                        "lastPrice": price.load(Ordering::SeqCst).to_string(), "currency": "KRW"
                    })
                })
                .collect();
            ResponseTemplate::new(200).set_body_json(json!({ "result": result }))
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/market-calendar/KR"))
        .respond_with(move |_: &Request| {
            let today = clock.now().with_timezone(&seoul()).date_naive();
            ResponseTemplate::new(200).set_body_json(json!({"result": {
                "today": holidays.toss_day(today),
                "previousBusinessDay": holidays.toss_day(holidays.business_day(today, false)),
                "nextBusinessDay": holidays.toss_day(holidays.business_day(today, true)),
            }}))
        })
        .mount(&server)
        .await;
    server
}

// ───────────── 가짜 AI ─────────────

fn joined(value: &Value) -> String {
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
        .map(|message| joined(&message["content"]))
        .unwrap_or_default()
}

/// 턴별 답: 하루 판단은 매매하기, 팀장 판단은 사기, 복기(교훈 포함)는 [`LESSON`], 점수는 70.
fn ai_reply(role: &str, last: &str) -> String {
    if last.contains("[하루 판단]") {
        json!({"plan": "trade"}).to_string()
    } else if last.contains("[복기]") {
        json!({"mistakes": [], "note": LESSON}).to_string()
    } else if last.contains("[점수]") {
        json!({"score": 70, "reason": "시험 점수"}).to_string()
    } else if last.contains("판단을 JSON 객체 하나로") {
        json!({
            "decision": "buy", "hypothesis": "시험 가설 실적 개선", "trigger": "시험 계기",
            "entry": 100_000, "stop": 95_000, "target": 110_000, "expected_value": 5_000
        })
        .to_string()
    } else if last.contains("[주문 안]") {
        json!({"side": "buy", "note": "시험 주문 안"}).to_string()
    } else {
        match role {
            "scout" => "새 자료를 확인했다. @strategy @risk 의견을 부탁한다".to_string(),
            "strategy" => "전략상 진입할 만하다.".to_string(),
            "risk" => "손절 기준을 지키면 감당할 만하다.".to_string(),
            _ => "(pass)".to_string(),
        }
    }
}

/// 로컬 가짜 AI 서버.
pub struct FakeAi {
    server: MockServer,
}

impl FakeAi {
    pub async fn start() -> FakeAi {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(move |request: &Request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                let system = joined(&body["system"]);
                let role = system
                    .split("담당 id: ")
                    .nth(1)
                    .and_then(|rest| rest.split_whitespace().next())
                    .unwrap_or("leader")
                    .to_string();
                let last = last_user_text(&body);
                ResponseTemplate::new(200).set_body_json(json!({
                    "id": "msg_round55", "type": "message", "role": "assistant", "model": "test-model",
                    "content": [{"type": "text", "text": ai_reply(&role, &last)}],
                    "stop_reason": "end_turn",
                    "usage": {"input_tokens": 1, "output_tokens": 1}
                }))
            })
            .mount(&server)
            .await;
        FakeAi { server }
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }
}

// ───────────── 가짜 자료 ─────────────

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

/// 경로로 나눠 답한다(round42와 같은 모양). 논의에 필요한 자료가 모두 있어야 담당들이 AI 턴을 돈다.
fn data_reply(request: &Request) -> ResponseTemplate {
    let path = request.url.path();
    match path {
        "/api/list.json" => data_ok(json!({
            "status": "000", "message": "정상", "page_no": 1, "page_count": 100,
            "total_count": 1, "total_page": 1, "list": [{
                "corp_cls": "Y", "corp_name": "시험회사", "corp_code": CORP_HYNIX, "stock_code": HYNIX,
                "report_nm": "주요사항보고서(자기주식취득결정)", "rcept_no": "20261116000055",
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

/// 로컬 가짜 자료 서버(DART · ECOS · FRED · 네이버 뉴스 한 곳).
pub async fn fake_data() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(data_reply)
        .mount(&server)
        .await;
    server
}
