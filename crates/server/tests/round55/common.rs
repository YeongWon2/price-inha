//! 실제 시세 모드에 가까운 조립의 도우미: **시연 아님** · 주문 길 달력을 주지 않음(시세 흐름이 받은 달력만 쓴다) ·
//! 가짜 증권사가 날마다 "오늘 앞뒤 하루"만 아는 달력을 준다. round52(결정 메모)와 round42(시연이 아닌 조립)의
//! 발판을 본떴다(그 폴더들은 고치지 않는다).
//!
//! 하루는 이렇게 지난다: 시계를 그 날 10시(한국)로 옮기면 시세 흐름이 그 날 달력을 받고 주문 길의 tick이 하루
//! 시작(`day_start`)을 남긴다. 16시로 옮기면 하루 마감(`day_close`)과 그 날 정규장 마지막 시세(`reference_prices`)를
//! 남긴다. 그 뒤 팀의 하루 마감(복기 · 정산, `Crew::review_day`)을 부른다.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use chrono::NaiveDate;
use pi_store::{Store, TradeId};
use price_inha::{App, Crew, DayPlan, Discussed, TEMPLATE_ADJUST_PATH, TEMPLATE_PATH, TeamError};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::assembly::{Parts, fresh_home, launch};
use crate::outside::{FakeAi, HYNIX, Holidays, TestClock, fake_broker, fake_data, kst};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(20);
/// 결정 메모 경로.
const MEMOS: &str = "/api/memos";

/// 2026-11-16 월요일(결정 날).
pub fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
}

/// 결정 날에서 `n`일 뒤(달력 날).
pub fn day(n: u64) -> NaiveDate {
    monday().checked_add_days(chrono::Days::new(n)).unwrap()
}

fn ymd(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

// ───────────── 띄운 서버 ─────────────

/// 떠 있는 서버 하나와 그 보관 폴더 · 바깥 자리.
pub struct Bench {
    dir: TempDir,
    parts: Parts,
    app: Option<App>,
}

impl Bench {
    /// [`Bench::start_bare`]에 더해 매매하기 좋게 만든다: 스윙 템플릿을 고르고 종목 비중 10%로 맞춘 뒤 오늘
    /// 매매하기로 한다(round52의 발판과 같다).
    pub async fn start(holidays: &[NaiveDate]) -> Bench {
        let bench = Bench::start_bare(holidays).await;
        let addr = bench.app().local_addr();
        let reply = post_json(addr, TEMPLATE_PATH, &json!({"style": "swing"})).await;
        let id = reply["id"].as_i64().expect("템플릿 번호");
        let body = json!({"id": id, "changes": {"symbol_weight_pct": "10"}});
        post_json(addr, TEMPLATE_ADJUST_PATH, &body).await;
        let _today = bench.app().trading().decide_day(DayPlan::Trade);
        bench
    }

    /// 결정 날(월요일) 10시에 띄우고 그 날의 하루 시작이 남을 때까지 기다린다. `holidays`는 달력의 휴장일.
    /// 템플릿을 고르지 않는다(사기 판단 뒤 주문 도구가 "고른 템플릿이 없습니다"로 실패한다).
    pub async fn start_bare(holidays: &[NaiveDate]) -> Bench {
        let clock = TestClock::at(kst(monday(), 10, 0));
        let price = Arc::new(AtomicI64::new(100_000));
        let days = Holidays::default();
        for holiday in holidays {
            days.add(*holiday);
        }
        let muted = Arc::new(AtomicBool::new(false));
        let broker = fake_broker(
            Arc::clone(&clock),
            Arc::clone(&price),
            Arc::clone(&muted),
            days.clone(),
        );
        let parts = Parts {
            broker: broker.await,
            clock,
            price,
            muted,
            holidays: days,
            ai: FakeAi::start().await,
            data: fake_data().await,
        };
        let dir = fresh_home().await;
        let app = launch(dir.path(), &parts).await;
        let bench = Bench {
            dir,
            parts,
            app: Some(app),
        };
        bench.wait_event("day_start", monday()).await;
        bench.let_quotes_flow().await;
        bench
    }

    fn app(&self) -> &App {
        self.app.as_ref().expect("서버가 떠 있다")
    }

    pub async fn close(mut self) {
        if let Some(app) = self.app.take() {
            tokio::time::timeout(STEP, app.shutdown())
                .await
                .expect("시간 안에 꺼져야 한다")
                .expect("끄기가 성공해야 한다");
        }
    }

    /// 서버를 끈다(보관 폴더는 남는다).
    pub async fn stop(&mut self) {
        if let Some(app) = self.app.take() {
            tokio::time::timeout(STEP, app.shutdown())
                .await
                .expect("시간 안에 꺼져야 한다")
                .expect("끄기가 성공해야 한다");
        }
    }

    /// 그 날 10시로 시계를 옮겨 같은 보관 폴더로 다시 띄운다.
    pub async fn restart_on(&mut self, date: NaiveDate, price: i64) {
        self.parts.price.store(price, Ordering::SeqCst);
        self.parts.clock.set(kst(date, 10, 0));
        self.app = Some(launch(self.dir.path(), &self.parts).await);
        if self.parts.holidays.is_open(date) {
            self.wait_event("day_start", date).await;
            self.let_quotes_flow().await;
        }
    }

    pub fn store(&self) -> Store {
        self.app().store()
    }

    pub fn crew(&self) -> Arc<Crew> {
        self.app().crew().expect("팀을 잇는다")
    }

    /// 시세 흐름이 몇 바퀴 돌아 그 시각의 시세가 주문 길에 닿게 기다린다.
    async fn let_quotes_flow(&self) {
        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    /// 그 종류 · 그 날짜(`date` 칸)의 저장 이벤트가 생길 때까지 기다린다.
    pub async fn wait_event(&self, kind: &str, date: NaiveDate) {
        let store = self.store();
        let text = ymd(date);
        let found = || {
            events(&store, kind)
                .iter()
                .any(|e| e["date"].as_str() == Some(text.as_str()))
        };
        let deadline = tokio::time::Instant::now() + STEP;
        while !found() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "{kind} {text}이 남아야 한다"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// 그 종목을 논의한다(가짜 AI의 팀장 판단은 사기).
    pub async fn discuss(&self) -> Result<Discussed, TeamError> {
        tokio::time::timeout(STEP, self.crew().discuss(HYNIX, "시험 논의"))
            .await
            .expect("논의가 시간 안에 끝나야 한다")
    }

    /// 사기로 판단된 매매를 만든다.
    pub async fn judged_buy(&self) -> TradeId {
        let discussed = self.discuss().await.expect("논의가 끝나야 한다");
        assert_eq!(discussed.outcome.decision, pi_agent::Decision::Buy);
        discussed.trade
    }

    /// 그 날 하루를 온전히 지낸다: 거래일이면 그 값으로 10시에 시세를 받고(종가가 된다) 16시에 마감한 뒤 팀의 하루
    /// 마감을 부른다. 주말 · 휴장일이면 시계만 옮기고 팀의 하루 마감을 부른다.
    pub async fn live_day(&self, date: NaiveDate, price: i64) {
        self.parts.price.store(price, Ordering::SeqCst);
        self.parts.clock.set(kst(date, 10, 0));
        let open = self.parts.holidays.is_open(date);
        if open {
            self.wait_event("day_start", date).await;
        }
        self.let_quotes_flow().await;
        self.close_on(date).await;
    }

    /// [`Bench::live_day`]와 같되 가짜 증권사가 그 날 [`HYNIX`] 시세를 주지 않는다(다른 종목 시세로 달력은 받는다).
    /// 그래서 그 날 [`HYNIX`]의 종가가 남지 않는다.
    pub async fn silent_day(&self, date: NaiveDate) {
        self.parts.muted.store(true, Ordering::SeqCst);
        self.live_day(date, 100_000).await;
        self.parts.muted.store(false, Ordering::SeqCst);
    }

    /// 그 날짜(포함) 이전 결정 날의 정산 대기 메모 수.
    pub async fn pending_through(&self, date: NaiveDate) -> usize {
        let last = ymd(date);
        self.memos()
            .await
            .iter()
            .filter(|memo| memo["state"] == "pending")
            .filter(|memo| memo["date"].as_str().is_some_and(|d| d <= last.as_str()))
            .count()
    }

    async fn close_on(&self, date: NaiveDate) {
        self.parts.clock.set(kst(date, 16, 0));
        if self.parts.holidays.is_open(date) {
            self.wait_event("day_close", date).await;
        } else {
            self.let_quotes_flow().await;
        }
        tokio::time::timeout(STEP, self.crew().review_day())
            .await
            .expect("마감이 시간 안에 끝나야 한다")
            .expect("마감이 끝나야 한다");
    }

    /// 결정 메모 목록(`?days=60`).
    pub async fn memos(&self) -> Vec<Value> {
        let reply = get_json(self.app().local_addr(), &format!("{MEMOS}?days=60")).await;
        reply.as_array().expect("배열이다").clone()
    }

    /// 그 매매의 메모.
    pub async fn memo_of(&self, trade: TradeId) -> Value {
        let list = self.memos().await;
        list.into_iter()
            .find(|memo| memo["trade"] == trade.0)
            .unwrap_or_else(|| panic!("매매 {}의 메모가 있어야 한다", trade.0))
    }
}

/// 저장된 이벤트 가운데 그 종류인 것의 내용(번호 순서).
pub fn events(store: &Store, kind: &str) -> Vec<Value> {
    store
        .read_after(0, 100_000)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == kind)
        .map(|event| event.payload)
        .collect()
}

/// 그 날짜의 `reference_prices`에 남은 [`HYNIX`] 종가.
pub fn close_of(store: &Store, date: NaiveDate) -> Option<String> {
    let text = ymd(date);
    events(store, "reference_prices")
        .into_iter()
        .find(|e| e["date"].as_str() == Some(text.as_str()))
        .and_then(|e| e["prices"][HYNIX].as_str().map(str::to_string))
}

// ───────────── HTTP ─────────────

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
async fn session(client: &reqwest::Client, origin: &str) -> String {
    let reply = client
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", origin)
        .send()
        .await
        .expect("세션 요청이 닿아야 한다");
    reply
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string())
        .expect("세션 쿠키가 있어야 한다")
}

async fn body_of(reply: reqwest::Response, path: &str) -> Value {
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문");
    assert_eq!(status, 200, "{path}: {text}");
    serde_json::from_str(&text).expect("JSON 본문")
}

/// 새 세션으로 JSON을 읽는다(200이어야 한다).
async fn get_json(addr: SocketAddr, path: &str) -> Value {
    let origin = format!("http://{addr}");
    let client = reqwest::Client::new();
    let cookie = session(&client, &origin).await;
    let reply = client
        .get(format!("{origin}{path}"))
        .header("Cookie", cookie)
        .send()
        .await
        .expect("요청이 닿아야 한다");
    body_of(reply, path).await
}

/// 새 세션 · 같은 출처로 JSON을 보낸다(200이어야 한다).
async fn post_json(addr: SocketAddr, path: &str, body: &Value) -> Value {
    let origin = format!("http://{addr}");
    let client = reqwest::Client::new();
    let cookie = session(&client, &origin).await;
    let reply = client
        .post(format!("{origin}{path}"))
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .header("Origin", &origin)
        .body(body.to_string())
        .send()
        .await
        .expect("요청이 닿아야 한다");
    body_of(reply, path).await
}
