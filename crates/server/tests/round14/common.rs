//! 7차 테스트의 공용 도우미: 서버 띄우기, 그 컴퓨터 · 다른 기기 흉내, HTTP 요청.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use price_inha::{App, Launch, Settings, TradingWiring, Wiring};
use serde_json::{Value, json};
use tempfile::TempDir;

/// 한 걸음의 기다림 상한.
pub const STEP: Duration = Duration::from_secs(15);

/// 다른 기기로 볼 상대 주소(글자).
pub const REMOTE_DEVICE: &str = "100.64.0.7";

const REMOTE_PEER: [u8; 4] = [100, 64, 0, 7];

fn no_env(_name: &str) -> Option<String> {
    None
}

fn settings(home: &Path) -> Settings {
    let args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--demo",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

/// 떠 있는 서버 하나와 그 보관 폴더.
pub struct Bench {
    dir: TempDir,
    remote: Arc<AtomicBool>,
    pub app: App,
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
        let wiring = Wiring {
            peer_override: Some(peer),
            cli_locations: Vec::new(),
            ..Wiring::default()
        };
        let launch = Launch {
            trading: TradingWiring::default(),
            team: None,
            secrets: None,
        };
        let start = App::launch(settings(dir.path()), wiring, launch);
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench { dir, remote, app }
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn home(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    /// 다른 기기로 보이게 한다.
    pub fn go_remote(&self) {
        self.remote.store(true, Ordering::SeqCst);
    }

    /// 다시 그 컴퓨터로 보이게 한다.
    pub fn back_home(&self) {
        self.remote.store(false, Ordering::SeqCst);
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}

pub fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태 · 본문 글자 · JSON 본문(JSON이 아니면 `Null`) · `Set-Cookie`의 첫 마디.
pub struct Reply {
    pub status: u16,
    pub text: String,
    pub body: Value,
    pub cookie: Option<String>,
}

async fn send(request: reqwest::RequestBuilder) -> Reply {
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let cookie = reply
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string());
    let bytes = reply.bytes().await.expect("본문을 읽어야 한다").to_vec();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Reply {
        status,
        text: String::from_utf8_lossy(&bytes).into_owned(),
        body,
        cookie,
    }
}

/// 세션을 청한다. `key`가 있으면 그 접속 키를 본문에 넣는다.
pub async fn ask_session(addr: SocketAddr, key: Option<&str>) -> Reply {
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin);
    if let Some(key) = key {
        request = request
            .header("Content-Type", "application/json")
            .body(json!({ pi_gate::ACCESS_KEY_FIELD: key }).to_string());
    }
    send(request).await
}

/// 세션을 받아 `Cookie` 헤더 글자를 돌려준다(받지 못하면 실패).
pub async fn session(addr: SocketAddr, key: Option<&str>) -> String {
    let reply = ask_session(addr, key).await;
    assert!(
        (200..300).contains(&reply.status),
        "세션을 받아야 한다: {}",
        reply.status
    );
    reply.cookie.expect("세션 쿠키가 있어야 한다")
}

pub async fn get_with(addr: SocketAddr, path: &str, cookie: Option<&str>) -> Reply {
    let mut request = reqwest::Client::new().get(format!("{}{path}", origin_of(addr)));
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request).await
}

/// 같은 출처로 JSON을 보낸다.
pub async fn post_with(addr: SocketAddr, path: &str, cookie: &str, body: &Value) -> Reply {
    let origin = origin_of(addr);
    let request = reqwest::Client::new()
        .post(format!("{origin}{path}"))
        .header("Origin", &origin)
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .body(body.to_string());
    send(request).await
}

/// 그 컴퓨터에서 접속 키를 발급받는다(그 컴퓨터로 보이는 상태에서 부른다).
pub async fn issue_key(bench: &Bench) -> (String, Reply) {
    let cookie = session(bench.addr(), None).await;
    let reply = post_with(
        bench.addr(),
        price_inha::ACCESS_KEY_PATH,
        &cookie,
        &json!({}),
    )
    .await;
    assert_eq!(reply.status, 200, "발급이 되어야 한다: {}", reply.text);
    let key = reply.body[pi_gate::ACCESS_KEY_FIELD]
        .as_str()
        .expect("답에 키가 있어야 한다")
        .to_string();
    (key, reply)
}

/// 보관 폴더 아래 모든 파일의 바이트에 `needle`이 있는가.
pub fn home_contains(home: &Path, needle: &str) -> bool {
    let needle = needle.as_bytes();
    let mut stack = vec![home.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path)
                && bytes.windows(needle.len()).any(|w| w == needle)
            {
                return true;
            }
        }
    }
    false
}
