//! 20차 시험의 공용 도우미: 서버 띄우기(다시 켜기 포함) · 세션 · 요청 · 해시 표.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_market::TossConfig;
use price_inha::{App, Settings, Upstreams, Wiring};
use serde_json::Value;

/// 한 걸음의 기다림 상한.
pub const STEP: Duration = Duration::from_secs(15);

/// 원본 자산의 (상대 경로, sha256) 표. 상대 경로는 원본 `public/assets/` 기준(`LICENSE.md`만 원본 저장소 꼭대기).
const TABLE: &str = include_str!("deskrpg_assets.txt");

pub fn table() -> Vec<(String, String)> {
    TABLE
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (hash, rel) = line.split_once("  ").expect("표의 줄: <sha256>  <경로>");
            (rel.to_owned(), hash.to_owned())
        })
        .collect()
}

/// 원본 외형 id 50개(표의 `characters/office/<id>.glb`).
pub fn look_ids() -> Vec<String> {
    let mut ids: Vec<String> = table()
        .into_iter()
        .filter_map(|(rel, _)| {
            rel.strip_prefix("characters/office/")
                .and_then(|f| f.strip_suffix(".glb"))
                .map(str::to_owned)
        })
        .collect();
    ids.sort();
    ids
}

fn no_env(_name: &str) -> Option<String> {
    None
}

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

/// 시연 모드로 띄운 서버. 보관 폴더를 지켜 두었다가 다시 켤 수 있다.
pub struct Bench {
    dir: tempfile::TempDir,
    app: App,
}

impl Bench {
    pub async fn start() -> Bench {
        Bench::start_in(tempfile::tempdir().unwrap()).await
    }

    pub async fn start_in(dir: tempfile::TempDir) -> Bench {
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
            peer_override: None,
            cli_locations: Vec::new(),
            ..Wiring::default()
        };
        let app = tokio::time::timeout(STEP, App::start(settings(dir.path()), wiring))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench { dir, app }
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    /// 끄고 같은 보관 폴더로 다시 켠다.
    pub async fn restart(self) -> Bench {
        let Bench { dir, app } = self;
        tokio::time::timeout(STEP, app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
        Bench::start_in(dir).await
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}

/// 받은 응답: 상태 · 본문 바이트 · JSON 본문(JSON이 아니면 `Null`).
pub struct Reply {
    pub status: u16,
    pub bytes: Vec<u8>,
    pub body: Value,
}

impl Reply {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let bytes = reply.bytes().await.expect("본문").to_vec();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Reply {
        status,
        bytes,
        body,
    }
}

pub fn origin_of(addr: SocketAddr) -> String {
    format!("http://{addr}")
}

/// 같은 출처에서 세션을 받아 `Cookie` 헤더 글자를 돌려준다.
pub async fn session(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let reply = tokio::time::timeout(
        STEP,
        reqwest::Client::new()
            .post(format!("{origin}{}", pi_gate::SESSION_PATH))
            .header("Origin", &origin)
            .send(),
    )
    .await
    .expect("응답이 시간 안에 와야 한다")
    .expect("요청이 닿아야 한다");
    assert!(reply.status().is_success(), "세션을 받아야 한다");
    reply
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string())
        .expect("세션 쿠키가 있어야 한다")
}

/// `GET` (쿠키가 있으면 그 세션으로).
pub async fn get(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let mut request = reqwest::Client::new().get(format!("{}{path}", origin_of(addr)));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request).await
}

/// `POST` JSON. `cookie` · `origin`이 `None`이면 그 머리를 붙이지 않는다.
pub async fn post(
    addr: SocketAddr,
    path: &str,
    cookie: Option<&str>,
    origin: Option<&str>,
    body: &Value,
) -> Reply {
    let mut request = reqwest::Client::new()
        .post(format!("{}{path}", origin_of(addr)))
        .header("Content-Type", "application/json")
        .body(body.to_string());
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    if let Some(origin) = origin {
        request = request.header("Origin", origin);
    }
    send(request).await
}
