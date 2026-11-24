//! 27차 시험의 공용 도우미: 서버 띄우기(그 컴퓨터 · 다른 기기로 보이게), 세션, JSON 요청, 템플릿 준비.
//! round28 · round24의 도우미를 본떴다(그 폴더는 고치지 않는다).

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_approval::{Approvals, Ask, Subject, WaitTimes};
use pi_market::TossConfig;
use pi_store::{
    AccessKeyHash, ApprovalStatus, NewTemplate, Store, TemplateId, TemplateState, UnixMillis,
};
use pi_template::{Style, Template};
use price_inha::{APPROVAL_ANSWER_PATH, App, Settings, TEMPLATE_PATH, Upstreams, Wiring};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// 한 걸음의 기다림 상한.
pub const STEP: Duration = Duration::from_secs(15);

/// 다른 기기의 접속 키(시험용, 글자 수 · 글자 모음은 `pi_gate`의 것).
const ACCESS_KEY: &str = "0123456789ABCDEFGHJK";

/// 다른 기기로 볼 상대 주소.
const REMOTE_PEER: [u8; 4] = [100, 64, 0, 7];

fn no_env(_name: &str) -> Option<String> {
    None
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn settings(home: &Path) -> Settings {
    let args: Vec<String> = ["--home", home.to_str().unwrap(), "--port", "0", "--demo"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

/// 지금 시각(유닉스 밀리초). 서버도 시스템 시계를 쓴다.
pub fn now_ms() -> UnixMillis {
    UnixMillis(chrono::Utc::now().timestamp_millis())
}

/// 승인 모듈이 읽는 시스템 시계.
struct SystemClock;

impl pi_approval::Clock for SystemClock {
    fn now(&self) -> UnixMillis {
        now_ms()
    }
}

/// 시연 모드로 띄운 서버. 상대 주소를 다른 기기로 바꿔 볼 수 있다.
pub struct Bench {
    _dir: tempfile::TempDir,
    remote: Arc<AtomicBool>,
    app: App,
}

impl Bench {
    pub async fn start() -> Bench {
        let dir = tempfile::tempdir().unwrap();
        let remote = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&remote);
        let peer: pi_gate::PeerOverride = Arc::new(move |addr: SocketAddr| {
            if flag.load(Ordering::SeqCst) {
                SocketAddr::from((REMOTE_PEER, addr.port()))
            } else {
                addr
            }
        });
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
            peer_override: Some(peer),
            cli_locations: Vec::new(),
            ..Wiring::default()
        };
        let app = tokio::time::timeout(STEP, App::start(settings(dir.path()), wiring))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            _dir: dir,
            remote,
            app,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    /// 승인 모듈의 눈으로 같은 저장소를 본다.
    pub fn approvals(&self) -> Approvals {
        Approvals::new(self.store(), Arc::new(SystemClock), WaitTimes::default())
    }

    /// 다른 기기로 보이게 한다(세션 없이).
    pub fn go_remote(&self) {
        self.remote.store(true, Ordering::SeqCst);
    }

    /// 다른 기기의 원격 세션을 받는다: 접속 키를 두고, 다른 기기로 보이게 한 뒤 그 키로 들어간다.
    /// 돌려준 뒤에도 다른 기기로 보인다.
    pub async fn remote_login(&self) -> String {
        let salt = vec![7_u8; 16];
        let mut hasher = Sha256::new();
        hasher.update(&salt);
        hasher.update(ACCESS_KEY.as_bytes());
        let hash = AccessKeyHash {
            sha256: hasher.finalize().into(),
            salt,
        };
        self.store()
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

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }

    /// 화면처럼 스타일을 고른다(지금 경로). 새 템플릿 번호.
    pub async fn choose(&self, style: &str) -> TemplateId {
        let reply = post(self.addr(), TEMPLATE_PATH, &json!({ "style": style }), None).await;
        assert_eq!(reply.status, 200, "고르기: {}", reply.text);
        TemplateId(reply.body["id"].as_i64().expect("템플릿 번호"))
    }

    /// 승격대기인 스윙 템플릿과 그 실전 승격 승인 요청(번호 · 지문)을 만든다.
    pub fn pending_template(&self) -> (TemplateId, String, String) {
        let store = self.store();
        let value: Value = serde_json::from_str(&Template::new(Style::Swing).to_json()).unwrap();
        let saved = store
            .create_template(NewTemplate {
                value,
                at: now_ms(),
            })
            .unwrap();
        let pending = store
            .submit_template_for_promotion(saved.id, &json!({ "passed": true }), now_ms())
            .unwrap();
        assert_eq!(pending.state, TemplateState::PromotionPending);
        let requested = self
            .approvals()
            .request(Ask {
                subject: Subject::LivePromotion {
                    template: pending.id,
                    revision: pending.revision,
                },
                reason: "시험: 승격 기준을 모두 넘었다".to_string(),
            })
            .expect("실전 승격 승인 요청");
        (
            pending.id,
            requested.number.as_str().to_string(),
            requested.fingerprint.as_str().to_string(),
        )
    }

    /// 실전중인 스윙 템플릿: 승격대기를 만든 뒤 지금 승인 길로 승인한다.
    pub async fn live_template(&self) -> TemplateId {
        let (id, number, fingerprint) = self.pending_template();
        let body = json!({ "number": number, "fingerprint": fingerprint, "answer": "approve" });
        let reply = post(self.addr(), APPROVAL_ANSWER_PATH, &body, None).await;
        assert_eq!(reply.body["result"], json!("approved"), "{}", reply.text);
        assert_eq!(self.state_of(id), TemplateState::Live, "실전중이다");
        id
    }

    /// 기다리는 승인 요청 수.
    pub fn approvals_pending(&self) -> usize {
        self.store()
            .approvals_in_status(ApprovalStatus::Pending)
            .unwrap()
            .len()
    }

    /// 저장소의 템플릿 상태.
    pub fn state_of(&self, id: TemplateId) -> TemplateState {
        self.store().get_template(id).unwrap().state
    }
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

fn cookie_of(reply: &reqwest::Response) -> String {
    reply
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string())
        .expect("세션 쿠키가 있어야 한다")
}

/// HTTP 응답 하나: 상태 · JSON 본문(JSON이 아니면 `Null`) · 본문 글.
pub struct Reply {
    pub status: u16,
    pub body: Value,
    pub text: String,
}

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다(그 컴퓨터).
async fn login(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let request = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin);
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("세션 요청이 닿아야 한다");
    assert!(reply.status().is_success(), "세션을 받아야 한다");
    cookie_of(&reply)
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

/// 세션을 붙여 같은 출처에서 보낸다. 쿠키를 주지 않으면 그 컴퓨터의 세션을 새로 받는다.
pub async fn post(addr: SocketAddr, path: &str, body: &Value, cookie: Option<&str>) -> Reply {
    let cookie = match cookie {
        Some(cookie) => cookie.to_string(),
        None => login(addr).await,
    };
    let request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie)
        .header("Origin", origin_of(addr))
        .header("Content-Type", "application/json")
        .body(body.to_string());
    reply_of(request).await
}

/// 세션을 붙여 읽는다. 쿠키를 주지 않으면 그 컴퓨터의 세션을 새로 받는다.
pub async fn get(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let cookie = match cookie {
        Some(cookie) => cookie.to_string(),
        None => login(addr).await,
    };
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    reply_of(request).await
}

/// 세션 없이 보낸다(다른 기기로 보일 때 401을 보려고).
pub async fn bare(addr: SocketAddr, method: reqwest::Method, path: &str) -> Reply {
    let request = reqwest::Client::new()
        .request(method, format!("{}{path}", origin_of(addr)))
        .header("Origin", origin_of(addr))
        .header("Content-Type", "application/json")
        .body("{}");
    reply_of(request).await
}

/// 멈추기 경로(`{id}` 자리에 번호).
pub fn stop_path(id: TemplateId) -> String {
    price_inha::TEMPLATE_STOP_PATH.replace("{id}", &id.0.to_string())
}

/// 읽기 응답에서 그 번호의 템플릿.
pub fn entry(view: &Value, id: TemplateId) -> Value {
    view["templates"]
        .as_array()
        .and_then(|list| list.iter().find(|t| t["id"] == json!(id.0)))
        .cloned()
        .unwrap_or_else(|| panic!("{}번 템플릿이 목록에 있다: {view}", id.0))
}

/// 십진수 글자(또는 수)를 수로.
pub fn num(value: &Value) -> f64 {
    match value {
        Value::String(text) => text.parse().unwrap_or_else(|_| panic!("수 글자: {value}")),
        other => other.as_f64().unwrap_or_else(|| panic!("수: {other}")),
    }
}
