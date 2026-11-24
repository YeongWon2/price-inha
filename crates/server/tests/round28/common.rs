//! 26차 시험의 공용 도우미: 서버 띄우기(그 컴퓨터 · 다른 기기로 보이게), 세션, 머리까지 받는 요청.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_market::TossConfig;
use pi_store::{AccessKeyHash, UnixMillis};
use price_inha::{App, Settings, Upstreams, Wiring};
use serde_json::json;
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
        self.app
            .store()
            .set_access_key(hash, UnixMillis(0))
            .expect("접속 키를 둔다");
        self.go_remote();
        let origin = format!("http://127.0.0.1:{}", self.addr().port());
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
        reply
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .map(|raw| raw.split(';').next().unwrap().trim().to_string())
            .expect("세션 쿠키가 있어야 한다")
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}

/// 받은 응답: 상태 · 머리 · 본문 바이트.
pub struct Reply {
    pub status: u16,
    pub headers: reqwest::header::HeaderMap,
    pub bytes: Vec<u8>,
}

impl Reply {
    /// 그 머리의 값. 없으면 빈 글.
    pub fn header(&self, name: &str) -> String {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    }
}

/// HTML이 바로 부르는 같은 출처 파일: 스크립트 · modulepreload · 모양(stylesheet).
pub fn first_refs(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (tag, attr) in [("<script", "src=\""), ("<link", "href=\"")] {
        for piece in html.split(tag).skip(1) {
            let head = piece.split('>').next().unwrap_or_default();
            if tag == "<link" && !(head.contains("modulepreload") || head.contains("stylesheet")) {
                continue;
            }
            if let Some(rest) = head.split(attr).nth(1) {
                let url = rest.split('"').next().unwrap_or_default();
                if url.starts_with('/') && !url.starts_with("//") {
                    out.push(url.to_owned());
                }
            }
        }
    }
    out
}

/// `GET` (쿠키가 있으면 그 세션으로).
pub async fn get(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let mut request = reqwest::Client::new().get(format!("http://127.0.0.1:{}{path}", addr.port()));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let headers = reply.headers().clone();
    let bytes = reply.bytes().await.expect("본문").to_vec();
    Reply {
        status,
        headers,
        bytes,
    }
}
