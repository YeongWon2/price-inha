//! round39(server 38차 — 실행 증표) 시험이 함께 쓰는 도우미.
//!
//! - [`Root`]: 임시 보관 폴더(`pi-home`) · 사용자 홈 · 빈 `PATH` 폴더.
//! - [`launch`]: 시험이 임시 보관 폴더로 띄운 **진짜** 도는 서버(같은 프로세스의 [`App`], 시연 모드 — 바깥 주소 없음).
//! - [`Impostor`]: 세션 모양(`POST /api/session`에 204 + 세션 쿠키)만 흉내 내고 받은 요청을 모두 기록하는 가짜
//!   서버(로컬 wiremock). `whoami`에는 답하지 않거나([`Whoami::Silent`]) 주어진 증표로 만든 요약값을
//!   답한다([`Whoami::Proves`] — 증표를 아는 도우미 · 다른 서버의 증표). 그 밖의 요청에는 그럴듯한 200을 준다.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use price_inha::{
    ACCESS_KEY_PATH, App, Launch, RUN_PROOF_DOMAIN, RUN_PROOF_FILE, Settings, TradingWiring,
    WHOAMI_NONCE_FIELD, WHOAMI_PATH, WHOAMI_PROOF_FIELD, Wiring,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 한 걸음(서버 띄우기 · 실행 파일 한 번)을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(60);
/// 시험용 AI API 키(진짜 키처럼 보이지 않는다).
pub const AI_KEY: &str = "zz-round39-ai-key-qwmv-0001";
/// 시험용 증권 키 칸 값.
pub const CLIENT_ID: &str = "zz-round39-client-id-0002";
pub const CLIENT_SECRET: &str = "zz-round39-client-secret-0003";
/// 사칭 서버가 "새 접속 키"라며 내미는 글자(찍히면 안 된다).
pub const PLANTED_KEY: &str = "ZZPLANTEDKEYROUND39X";
/// 이 보관 폴더의 증표 자리에 시험이 써 두는 증표(64글자 16진수).
pub const TOKEN_HOME: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
/// 다른 서버의 증표.
pub const TOKEN_OTHER: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
/// 사칭 서버가 주는 세션 쿠키 값.
const FAKE_SESSION: &str = "zz-round39-fake-session";

// ───────────── 임시 폴더 ─────────────

/// 임시 뿌리: 보관 폴더(`pi-home`, 만들지 않는다) · 사용자 홈(`user`) · 빈 `PATH` 폴더(`bin`).
pub struct Root {
    _dir: TempDir,
    /// 보관 폴더.
    pub home: PathBuf,
    /// 사용자 홈(`HOME`).
    pub user: PathBuf,
    /// `PATH` 앞에 둘 빈 폴더(`claude`가 없다).
    pub bin: PathBuf,
}

impl Root {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        // macOS의 임시 폴더는 다른 경로를 거치므로 실제 경로로 바꿔 둔다.
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let user = base.join("user");
        let bin = base.join("bin");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        Self {
            _dir: dir,
            home: base.join("pi-home"),
            user,
            bin,
        }
    }

    pub fn home_str(&self) -> &str {
        self.home.to_str().unwrap()
    }

    /// 보관 폴더를 0700으로 만든다(서버가 켤 때처럼).
    pub fn make_home(&self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(&self.home).unwrap();
        std::fs::set_permissions(&self.home, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
}

/// 아무도 듣지 않는 루프백 포트(잠깐 열었다 닫은 포트).
pub fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// 폴더 아래 모든 파일.
pub fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// 보관 폴더에서 증표 파일이 아닌 파일 가운데 `needle`이 든 것.
pub fn other_files_with(home: &Path, needle: &str) -> Vec<PathBuf> {
    files_under(home)
        .into_iter()
        .filter(|file| file.file_name().is_none_or(|name| name != RUN_PROOF_FILE))
        .filter(|file| {
            std::fs::read(file)
                .map(|bytes| String::from_utf8_lossy(&bytes).contains(needle))
                .unwrap_or(false)
        })
        .collect()
}

// ───────────── 증표 파일 ─────────────

/// 증표 파일 경로.
pub fn proof_path(home: &Path) -> PathBuf {
    home.join(RUN_PROOF_FILE)
}

/// 증표 파일의 (내용, 권한 비트). 없으면 `None`.
pub fn read_proof(home: &Path) -> Option<(String, u32)> {
    use std::os::unix::fs::PermissionsExt;
    let file = proof_path(home);
    let text = std::fs::read_to_string(&file).ok()?;
    let mode = std::fs::metadata(&file).ok()?.permissions().mode() & 0o777;
    Some((text, mode))
}

/// 시험이 증표 파일을 그 권한으로 써 둔다(보관 폴더는 만든다).
pub fn write_proof(root: &Root, token: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    root.make_home();
    let file = proof_path(&root.home);
    std::fs::write(&file, token).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// 증표는 소문자 16진수 64글자다.
pub fn is_token(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 계약의 식을 시험이 따로 계산한다(38차 보강): 소문자 16진수(SHA-256(앞머리 v2 ‖ "\n" ‖ 증표 ‖ "\n" ‖ nonce
/// ‖ "\n" ‖ 십진수 포트)).
pub fn expected_proof(token: &str, nonce: &str, port: u16) -> String {
    let mut hasher = Sha256::new();
    hasher.update(RUN_PROOF_DOMAIN.as_bytes());
    hasher.update(b"\n");
    hasher.update(token.as_bytes());
    hasher.update(b"\n");
    hasher.update(nonce.as_bytes());
    hasher.update(b"\n");
    hasher.update(port.to_string().as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ───────────── 진짜 도는 서버 ─────────────

/// 보관 폴더 `home`으로 진짜 서버를 루프백 빈 포트에 띄운다(시연 모드, 팀 · 바깥 잇기 없음).
pub async fn launch(home: &Path) -> App {
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
    .map(|arg| (*arg).to_string())
    .collect();
    let settings = Settings::parse(&args, &|_| None, None).unwrap();
    let wiring = Wiring {
        cli_locations: Vec::new(),
        ..Wiring::default()
    };
    let launch = Launch {
        trading: TradingWiring::default(),
        team: None,
        secrets: None,
    };
    tokio::time::timeout(STEP, App::launch(settings, wiring, launch))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

/// 끈다.
pub async fn close(app: App) {
    tokio::time::timeout(STEP, app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}

/// 프록시를 거쳐 온 요청의 전달 헤더(있기만 해도 다른 기기다).
pub const PROXIED: (&str, &str) = ("X-Forwarded-For", "100.64.0.7");

/// HTTP 응답 하나: 상태 · 본문 글 · JSON(아니면 `Null`) · `Set-Cookie`의 첫 마디.
pub struct Reply {
    pub status: u16,
    pub text: String,
    pub body: Value,
    pub cookie: Option<String>,
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

async fn send(request: reqwest::RequestBuilder, remote: bool) -> Reply {
    let request = if remote {
        request.header(PROXIED.0, PROXIED.1)
    } else {
        request
    };
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let cookie = reply
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string());
    let text = reply.text().await.expect("본문을 읽어야 한다");
    Reply {
        status,
        body: serde_json::from_str(&text).unwrap_or(Value::Null),
        text,
        cookie,
    }
}

/// 세션을 청한다. `key`가 있으면 접속 키를 싣는다. `remote`면 다른 기기로 보인다.
pub async fn enter(addr: SocketAddr, key: Option<&str>, remote: bool) -> Reply {
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin);
    if let Some(key) = key {
        request = request
            .header("Content-Type", "application/json")
            .body(json!({ pi_gate::ACCESS_KEY_FIELD: key }).to_string());
    }
    send(request, remote).await
}

/// 다른 서버의 [`WHOAMI_PATH`]에 그 컴퓨터의 세션으로 `body`를 보내고 답(상태 · 본문 글)을 돌려준다 — 중계
/// 사칭이 받은 nonce를 진짜 서버로 넘길 때 쓴다(사칭도 같은 컴퓨터라 세션을 얻는다).
pub async fn relay_whoami(addr: SocketAddr, body: &str) -> (u16, String) {
    let cookie = local_session(addr).await;
    let reply = whoami(addr, Some(&cookie), body, false).await;
    (reply.status, reply.text)
}

/// 그 컴퓨터의 세션 쿠키.
pub async fn local_session(addr: SocketAddr) -> String {
    enter(addr, None, false)
        .await
        .cookie
        .expect("그 컴퓨터는 키 없이 세션을 받는다")
}

/// 화면과 같은 길로 접속 키를 만든다(그 컴퓨터의 세션).
pub async fn issue_on_screen(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let cookie = local_session(addr).await;
    let reply = send(
        reqwest::Client::new()
            .post(format!("{origin}{ACCESS_KEY_PATH}"))
            .header("Origin", &origin)
            .header("Cookie", cookie)
            .header("Content-Type", "application/json")
            .body("{}"),
        false,
    )
    .await;
    assert_eq!(reply.status, 200, "화면의 발급");
    reply.body[pi_gate::ACCESS_KEY_FIELD]
        .as_str()
        .expect("발급 답에 키가 있다")
        .to_string()
}

/// `POST` [`WHOAMI_PATH`]에 `body`(글 그대로)를 보낸다. `cookie`가 있으면 싣는다.
pub async fn whoami(addr: SocketAddr, cookie: Option<&str>, body: &str, remote: bool) -> Reply {
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .post(format!("{origin}{WHOAMI_PATH}"))
        .header("Origin", &origin)
        .header("Content-Type", "application/json")
        .body(body.to_string());
    if let Some(cookie) = cookie {
        request = request.header("Cookie", cookie);
    }
    send(request, remote).await
}

/// nonce 하나를 실은 확인 본문.
pub fn nonce_body(nonce: &str) -> String {
    json!({ WHOAMI_NONCE_FIELD: nonce }).to_string()
}

// ───────────── 사칭 서버 ─────────────

/// 사칭 서버가 `whoami`에 어떻게 답하나.
#[derive(Clone)]
pub enum Whoami {
    /// 그 경로를 모른다(404).
    Silent,
    /// 이 증표로 만든 요약값을 답한다(요청의 nonce로).
    Proves(String),
}

/// 세션 모양만 흉내 내는 가짜 서버.
pub struct Impostor {
    server: MockServer,
}

impl Impostor {
    /// 빈 포트에 세운다.
    pub async fn start(whoami: Whoami) -> Impostor {
        Self::mount(MockServer::start().await, whoami).await
    }

    /// 이미 연 리스너(정한 포트)에 세운다.
    pub async fn start_on(listener: std::net::TcpListener, whoami: Whoami) -> Impostor {
        let server = MockServer::builder().listener(listener).start().await;
        Self::mount(server, whoami).await
    }

    async fn mount(server: MockServer, whoami: Whoami) -> Impostor {
        // 증표를 아는 도우미는 CLI가 접속한 자기 포트로 요약값을 만든다(38차 보강의 식).
        let own_port = server.address().port();
        Mock::given(method("POST"))
            .and(path(pi_gate::SESSION_PATH))
            .respond_with(
                ResponseTemplate::new(204).insert_header(
                    "set-cookie",
                    format!(
                        "{}={FAKE_SESSION}; Path=/; HttpOnly; SameSite=Strict",
                        pi_gate::SESSION_COOKIE
                    )
                    .as_str(),
                ),
            )
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(WHOAMI_PATH))
            .respond_with(move |request: &Request| match &whoami {
                Whoami::Silent => ResponseTemplate::new(404),
                Whoami::Proves(token) => {
                    let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                    let nonce = body[WHOAMI_NONCE_FIELD].as_str().unwrap_or_default();
                    ResponseTemplate::new(200).set_body_json(
                        json!({ WHOAMI_PROOF_FIELD: expected_proof(token, nonce, own_port) }),
                    )
                }
            })
            .with_priority(1)
            .mount(&server)
            .await;
        // 접속 키 발급에는 "새 키"를 내민다(믿으면 사용자에게 사칭의 키가 보인다).
        Mock::given(method("POST"))
            .and(path(ACCESS_KEY_PATH))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({ pi_gate::ACCESS_KEY_FIELD: PLANTED_KEY, "generation": 7 }),
                ),
            )
            .with_priority(1)
            .mount(&server)
            .await;
        // 그 밖의 모든 요청에는 받아들였다는 200.
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .with_priority(9)
            .mount(&server)
            .await;
        Impostor { server }
    }

    pub fn port(&self) -> u16 {
        self.server.address().port()
    }

    /// 받은 모든 요청.
    pub async fn requests(&self) -> Vec<Request> {
        self.server.received_requests().await.unwrap_or_default()
    }

    /// 세션 청과 `whoami` 말고 받은 쓰기 요청(GET이 아닌 것)의 경로.
    pub async fn writes(&self) -> Vec<String> {
        self.requests()
            .await
            .into_iter()
            .filter(|request| request.method.as_str() != "GET")
            .map(|request| request.url.path().to_string())
            .filter(|at| at != pi_gate::SESSION_PATH && at != WHOAMI_PATH)
            .collect()
    }

    /// 받은 요청 어디(주소 · 헤더 · 본문)에든 `needle`이 들었나.
    pub async fn saw(&self, needle: &str) -> bool {
        self.requests().await.iter().any(|request| {
            request.url.as_str().contains(needle)
                || format!("{:?}", request.headers).contains(needle)
                || String::from_utf8_lossy(&request.body).contains(needle)
        })
    }

    /// 받은 `whoami` 요청의 nonce들.
    pub async fn nonces(&self) -> Vec<String> {
        self.requests()
            .await
            .into_iter()
            .filter(|request| request.url.path() == WHOAMI_PATH)
            .map(|request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
                body[WHOAMI_NONCE_FIELD]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    }
}

/// 증표 파일이 이상한 세 경우(이 보관 폴더의 서버가 아님을 CLI가 알아야 한다).
#[derive(Clone, Copy, Debug)]
pub enum Broken {
    /// 증표 파일이 없다(사칭은 아무 증표로나 답한다).
    Missing,
    /// 증표 파일이 0644다 — 남이 읽을 수 있으니 사칭이 그 증표로 맞는 요약값을 답해도 믿지 않는다.
    Wide,
    /// 증표 파일은 0600인데 답이 다른 서버의 증표로 만든 것이다.
    Other,
}

pub const BROKEN: [Broken; 3] = [Broken::Missing, Broken::Wide, Broken::Other];

impl Broken {
    /// 보관 폴더를 그 경우로 차리고 사칭 서버를 세운다.
    pub async fn stage(self, root: &Root) -> Impostor {
        match self {
            Broken::Missing => {
                root.make_home();
                Impostor::start(Whoami::Proves(TOKEN_HOME.to_string())).await
            }
            Broken::Wide => {
                write_proof(root, TOKEN_HOME, 0o644);
                Impostor::start(Whoami::Proves(TOKEN_HOME.to_string())).await
            }
            Broken::Other => {
                write_proof(root, TOKEN_HOME, 0o600);
                Impostor::start(Whoami::Proves(TOKEN_OTHER.to_string())).await
            }
        }
    }
}
