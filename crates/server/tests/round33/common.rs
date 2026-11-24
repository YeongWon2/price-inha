//! 31차 시험의 공용 도우미: 시계 · "실제" 주소 자리의 로컬 가짜 서버 · 키 등록 · 실행 파일 조립으로 띄우기 · HTTP.
//! round26 · round32의 도우미를 본떴다(그 폴더들은 고치지 않는다).
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0` · 시연 모드로 띄운다. 조립은 실행 파일과 같은 [`run_assembly`]이고, 시험은
//! 시계 · 바깥 주소(모두 로컬 가짜 서버) · CLI 찾는 자리(비움) · 상대 주소(다른 기기로 바꿔 볼 수 있다) · 담당 모델
//! 이름만 바꾼다. 가짜 서버는 무엇을 물어도 503으로 답하고 받은 요청을 센다.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_ai::AnthropicConfig;
use pi_data::{DataConfig, NAVER_NEWS_KEYS, NaverNewsConfig};
use pi_market::{Clock, Symbol, TossConfig};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use pi_store::{AccessKeyHash, UnixMillis};
use price_inha::{App, RunAssembly, Settings, Upstreams, Wiring, run_assembly, save_watchlist};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(15);
/// 지켜볼 종목(보관 폴더에 저장한다).
const WATCHED: &str = "005930";
/// 조립의 담당 모델 이름을 이것으로 바꿔 띄운다(진짜 모델 이름처럼 보이지 않는다).
pub const MODEL: &str = "zz-test-model-round33";

/// 띄우기 전에 등록하는 키 값들(진짜 키처럼 보이지 않는다). 어느 응답에도 나오면 안 된다.
pub const BROKER_ID: &str = "qzbv-r33-broker-id-wpkm";
pub const BROKER_SECRET: &str = "jxfr-r33-broker-secret-tqhn";
pub const AI_VALUE: &str = "vkzq-r33-ai-value-mxjd";
pub const SLOT_VALUE: &str = "hgpw-r33-slot-value-zbqf";
pub const NAVER_ID: &str = "pfmd-r33-naver-id-ckwy";
pub const NAVER_SECRET: &str = "tnrb-r33-naver-secret-glxs";

/// 등록한 키 값 모두.
pub const KEY_VALUES: [&str; 6] = [
    BROKER_ID,
    BROKER_SECRET,
    AI_VALUE,
    SLOT_VALUE,
    NAVER_ID,
    NAVER_SECRET,
];

// ───────────── 시계 ─────────────

/// 테스트가 정하는 시계. 자는 것은 짧게만 잔다.
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

/// 2026-11-22 일요일 14:00(한국 시각).
fn sunday_afternoon() -> DateTime<Utc> {
    let offset = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    let date = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
    offset
        .from_local_datetime(&date.and_hms_opt(14, 0, 0).unwrap())
        .unwrap()
        .with_timezone(&Utc)
}

// ───────────── "실제" 주소 자리의 가짜 서버 ─────────────

/// 무엇을 물어도 503으로 답하고 받은 요청을 세는 로컬 가짜 서버.
async fn counting_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    server
}

/// 증권사(토스) · AI(Anthropic) · 자료(DART · ECOS · FRED) · 네이버 뉴스 주소 자리의 가짜 서버들.
pub struct Outside {
    broker: MockServer,
    ai: MockServer,
    data: MockServer,
    news: MockServer,
}

impl Outside {
    async fn start() -> Outside {
        Outside {
            broker: counting_server().await,
            ai: counting_server().await,
            data: counting_server().await,
            news: counting_server().await,
        }
    }

    async fn received(server: &MockServer) -> usize {
        server
            .received_requests()
            .await
            .map_or(0, |list| list.len())
    }

    /// (이름, 받은 요청 수) 넷.
    pub async fn counts(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("증권사", Outside::received(&self.broker).await),
            ("AI", Outside::received(&self.ai).await),
            ("자료", Outside::received(&self.data).await),
            ("네이버", Outside::received(&self.news).await),
        ]
    }
}

// ───────────── 보관 폴더 · 설정 · 조립 ─────────────

fn no_env(_name: &str) -> Option<String> {
    None
}

fn parse(list: &[&str]) -> Settings {
    let args: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
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

/// 띄우기 전 보관 폴더에 모든 키(증권사 · AI · 칸 키 · 네이버)를 [`KEY_VALUES`]로 등록한다.
async fn register_keys(home: &Path) {
    let secrets = SecretStore::open(home).unwrap();
    let broker = Credentials::new()
        .with("client_id", BROKER_ID)
        .with("client_secret", BROKER_SECRET);
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", AI_VALUE);
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    for slot in KeySlot::ALL {
        let values = Credentials::new().with(slot.fields()[0], SLOT_VALUE);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    let naver = Credentials::new()
        .with("client_id", NAVER_ID)
        .with("client_secret", NAVER_SECRET);
    secrets
        .register_provider(NAVER_NEWS_KEYS.provider_keys(), naver, &AcceptAll)
        .await
        .unwrap();
}

/// 다른 기기의 접속 키(시험용, 글자 수 · 글자 모음은 `pi_gate`의 것).
const ACCESS_KEY: &str = "0123456789ABCDEFGHJK";

/// 다른 기기로 볼 상대 주소.
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

/// 바깥 주소 자리를 가짜 서버로 둔 위탁 값.
fn upstreams(outside: &Outside) -> Upstreams {
    Upstreams {
        toss: TossConfig {
            base_url: outside.broker.uri(),
            timeout: Duration::from_secs(3),
            token_failure_cooldown: Duration::ZERO,
            ..TossConfig::default()
        },
        anthropic: AnthropicConfig {
            base_url: outside.ai.uri(),
            timeout: Duration::from_secs(3),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
        },
    }
}

/// 실행 파일 조립에서 시계 · 바깥 주소 · CLI 찾는 자리 · 상대 주소 · 담당 모델 이름만 바꾼다.
fn bend(
    mut assembly: RunAssembly,
    clock: &Arc<TestClock>,
    outside: &Outside,
    remote: &Arc<AtomicBool>,
) -> RunAssembly {
    assembly.wiring.clock = Arc::clone(clock) as Arc<dyn Clock>;
    assembly.wiring.cli_locations = Vec::new();
    assembly.wiring.upstreams = upstreams(outside);
    assembly.wiring.peer_override = Some(peer_override(Arc::clone(remote)));
    let team = assembly
        .launch
        .team
        .as_mut()
        .expect("실행 파일 조립은 팀을 잇는다");
    let data = outside.data.uri();
    team.data = DataConfig::new(data.clone(), data.clone(), data);
    team.data.timeout = Duration::from_secs(3);
    team.news = NaverNewsConfig::new(outside.news.uri()).with_timeout(Duration::from_secs(3));
    team.model = MODEL.to_string();
    assembly
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 가짜 서버들.
pub struct Bench {
    _dir: TempDir,
    remote: Arc<AtomicBool>,
    app: App,
    pub outside: Outside,
}

impl Bench {
    /// 모든 키를 등록해 두고 시연 모드 실행 파일 조립으로 띄운다.
    pub async fn with_keys() -> Bench {
        Bench::start(true).await
    }

    /// 키 없이 시연 모드 실행 파일 조립으로 띄운다.
    pub async fn without_keys() -> Bench {
        Bench::start(false).await
    }

    async fn start(keys: bool) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        if keys {
            register_keys(dir.path()).await;
        }
        save_watchlist(dir.path(), &[Symbol::new(WATCHED)]).unwrap();
        Bench::launch(dir).await
    }

    /// 끄고, 같은 보관 폴더로 다시 띄운다(시계 · 가짜 서버 · 상대 주소는 새로).
    pub async fn restart(self) -> Bench {
        let Bench { _dir: dir, app, .. } = self;
        tokio::time::timeout(STEP, app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        Bench::launch(dir).await
    }

    /// 그 보관 폴더로 시연 모드 실행 파일 조립을 띄운다.
    async fn launch(dir: TempDir) -> Bench {
        let clock = TestClock::at(sunday_afternoon());
        let outside = Outside::start().await;
        let remote = Arc::new(AtomicBool::new(false));
        let home = dir.path().to_str().unwrap();
        let settings = parse(&[
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
            "--demo",
        ]);
        let assembly = bend(run_assembly(&settings, None), &clock, &outside, &remote);
        let app = tokio::time::timeout(STEP, App::launch_run(settings, assembly))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            _dir: dir,
            remote,
            app,
            outside,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    /// 다른 기기로 보이게 한다(세션 없이).
    pub fn go_remote(&self) {
        self.remote.store(true, Ordering::SeqCst);
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
        self.go_remote();
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

    /// 끈다.
    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}

/// 팀 없이(`App::start`) 미국 시장 · 시연 모드로 띄운 서버와 그 보관 폴더 · 가짜 서버들.
pub struct UsBench {
    _dir: TempDir,
    _outside: Outside,
    pub app: App,
}

impl UsBench {
    pub async fn start() -> UsBench {
        let dir = tempfile::tempdir().unwrap();
        let outside = Outside::start().await;
        let home = dir.path().to_str().unwrap();
        let settings = parse(&[
            "--home",
            home,
            "--port",
            "0",
            "--interval-ms",
            "20",
            "--demo",
            "--market",
            "us",
            "--symbols",
            "AAPL",
        ]);
        let wiring = Wiring {
            upstreams: upstreams(&outside),
            clock: TestClock::at(sunday_afternoon()) as Arc<dyn Clock>,
            peer_override: None,
            cli_locations: Vec::new(),
        };
        let app = tokio::time::timeout(STEP, App::start(settings, wiring))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        UsBench {
            _dir: dir,
            _outside: outside,
            app,
        }
    }
}

// ───────────── HTTP ─────────────

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태 · 본문 글자 그대로 · JSON 본문(JSON이 아니면 `Null`).
pub struct Reply {
    pub status: u16,
    pub text: String,
    pub body: Value,
}

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let text = reply.text().await.expect("본문을 읽어야 한다");
    let body = serde_json::from_str(&text).unwrap_or(Value::Null);
    Reply { status, text, body }
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

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
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

/// 그 세션(`Cookie` 헤더 글자)으로 읽는다.
pub async fn get_with(addr: SocketAddr, path: &str, cookie: &str) -> Reply {
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    send(request).await
}

/// 세션 없이 읽는다.
pub async fn get_anonymous(addr: SocketAddr, path: &str) -> Reply {
    send(reqwest::Client::new().get(format!("{}{path}", origin_of(addr)))).await
}

/// 같은 출처(`Origin`)에서 JSON 글자 `body`를 `POST`한다. `cookie`가 없으면 세션 없이 보낸다.
pub async fn post_with(addr: SocketAddr, path: &str, cookie: Option<&str>, body: &str) -> Reply {
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .post(format!("{origin}{path}"))
        .header("Origin", &origin)
        .header("Content-Type", "application/json")
        .body(body.to_string());
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request).await
}

/// 세션을 받아 읽는다.
pub async fn get(addr: SocketAddr, path: &str) -> Reply {
    let cookie = login(addr).await;
    get_with(addr, path, &cookie).await
}

/// 객체의 칸 이름들(정렬).
pub fn keys_of(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap_or_else(|| panic!("객체여야 한다: {value}"))
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}
