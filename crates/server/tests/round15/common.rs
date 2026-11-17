//! 8차 테스트의 공용 도우미: 가짜 자료 · GitHub 서버, 서버 띄우기, 그 컴퓨터 · 다른 기기 흉내, HTTP 요청.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pi_data::DataConfig;
use pi_secrets::{KeySlot, SecretStore};
use price_inha::{App, BackupWiring, Launch, Reach, Settings, TeamWiring, TradingWiring, Wiring};
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 한 걸음의 기다림 상한.
pub const STEP: Duration = Duration::from_secs(15);

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

/// 가짜 바깥 서버 하나(자료 세 출처와 GitHub `/user`). `accept`면 키를 받아들이고, 아니면 거절한다.
async fn fake_upstream(accept: bool) -> MockServer {
    let server = MockServer::start().await;
    let (dart, ecos, fred, github) = if accept {
        (
            ResponseTemplate::new(200).set_body_json(json!({"status": "013", "message": "없음"})),
            ResponseTemplate::new(200).set_body_json(json!({"StatisticTableList": {}})),
            ResponseTemplate::new(200).set_body_json(json!({"seriess": []})),
            ResponseTemplate::new(200).set_body_json(json!({"login": "test"})),
        )
    } else {
        (
            ResponseTemplate::new(200).set_body_json(json!({"status": "010", "message": "키"})),
            ResponseTemplate::new(200).set_body_json(
                json!({"RESULT": {"CODE": "INFO-100", "MESSAGE": "인증키가 유효하지 않습니다."}}),
            ),
            ResponseTemplate::new(400).set_body_json(json!({"error_code": 400})),
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
    };
    for (matcher, template) in [
        (path_regex("^/api/list\\.json$"), dart),
        (path_regex("^/api/StatisticTableList/"), ecos),
        (path_regex("^/fred/series$"), fred),
    ] {
        Mock::given(method("GET"))
            .and(matcher)
            .respond_with(template)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(github)
        .mount(&server)
        .await;
    server
}

/// 떠 있는 서버 하나와 그 보관 폴더 · 비밀 보관소 · 가짜 바깥 서버.
pub struct Bench {
    dir: TempDir,
    remote: Arc<AtomicBool>,
    pub secrets: Arc<SecretStore>,
    pub upstream: MockServer,
    pub app: App,
}

impl Bench {
    /// 가짜 바깥 서버가 키를 받아들이면(`accept`) 등록되고, 아니면 거절된다.
    pub async fn start(accept: bool) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        let upstream = fake_upstream(accept).await;
        let base = upstream.uri();
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
        let secrets = Arc::new(SecretStore::open(dir.path().to_path_buf()).unwrap());
        let team = TeamWiring {
            data: DataConfig {
                dart_base_url: base.clone(),
                ecos_base_url: base.clone(),
                fred_base_url: base.clone(),
                timeout: Duration::from_secs(3),
            },
            ..TeamWiring::official()
        };
        let launch = Launch {
            trading: TradingWiring::default(),
            team: Some(team),
            secrets: Some(Arc::clone(&secrets)),
        };
        let reach = Reach {
            push: None,
            backup: Some(BackupWiring {
                api_base: base,
                repo: "owner/backup".to_string(),
                recipient: age::x25519::Identity::generate().to_public().to_string(),
            }),
        };
        let start = App::launch_with(settings(dir.path()), wiring, launch, reach);
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench {
            dir,
            remote,
            secrets,
            upstream,
            app,
        }
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

    /// 보관소에 그 칸이 등록됐으면 그 칸의 첫 값.
    pub fn stored(&self, slot: KeySlot) -> Option<String> {
        let values = self.secrets.read_slot(slot).unwrap()?;
        let name = slot.fields()[0];
        values.get(name).map(|value| value.expose().to_string())
    }

    /// 가짜 바깥 서버가 받은 요청 수.
    pub async fn upstream_hits(&self) -> usize {
        self.upstream
            .received_requests()
            .await
            .map_or(0, |requests| requests.len())
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

/// 세션을 받아 `Cookie` 헤더 글자를 돌려준다. `key`가 있으면 그 접속 키로 청한다.
pub async fn session(addr: SocketAddr, key: Option<&str>) -> String {
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin);
    if let Some(key) = key {
        request = request
            .header("Content-Type", "application/json")
            .body(json!({ pi_gate::ACCESS_KEY_FIELD: key }).to_string());
    }
    let reply = send(request).await;
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

/// 그 컴퓨터에서 접속 키를 발급받아 다른 기기의 세션을 연다(부른 뒤 다른 기기로 보인다).
pub async fn remote_session(bench: &Bench) -> String {
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
    bench.go_remote();
    session(bench.addr(), Some(&key)).await
}

/// 보관 폴더 아래 비밀 키 파일 말고 모든 파일의 바이트에 `needle`이 있는가(저장소 · 기록 등).
pub fn outside_secrets_contains(home: &Path, needle: &str) -> bool {
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
            } else if path.file_name().and_then(|n| n.to_str()) == Some(pi_secrets::ENV_FILE) {
                continue;
            } else if let Ok(bytes) = std::fs::read(&path)
                && bytes.windows(needle.len()).any(|w| w == needle)
            {
                return true;
            }
        }
    }
    false
}
