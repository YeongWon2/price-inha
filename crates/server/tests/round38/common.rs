//! round38(server 37차 — `setup` 마법사 · `open`) 시험이 함께 쓰는 도우미.
//!
//! - [`Bench`]: 시험이 임시 보관 폴더로 띄운 **도는 서버**(같은 프로세스의 [`price_inha::App`]). 토스 · Anthropic
//!   자리는 로컬 가짜 서버(wiremock)뿐이다. 키의 등록 전 확인은 이 서버가 가짜 서버로 한다.
//! - 실행 파일을 띄우는 것은 `run` 모듈이다(비밀값은 인자로 주지 않는다 — 표준 입력으로만).
//! - [`FakeCli`]: 임시 폴더의 가짜 `claude` 셸 스크립트(로그인됨 / 로그인 안 됨 / 없음). 로그인 여부는 표시 파일로
//!   정하고, 시험이 도중에 [`FakeCli::log_in`]으로 바꿀 수 있다. 실제 `claude`를 부르지 않는다.
//! - 사용자 홈에는 다른 프로그램의 토큰 파일 모양을 한 미끼([`BAIT`])를 둔다.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_market::{SystemClock, TossConfig};
use price_inha::{App, SETUP_PATH, Settings, Upstreams, Wiring};
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 한 걸음(서버 띄우기 · 마법사 한 번)을 기다리는 가장 긴 시간.
pub const STEP: Duration = Duration::from_secs(60);
/// 시험용 AI API 키(진짜 키처럼 보이지 않는다).
pub const AI_KEY: &str = "zz-round38-ai-key-qwmv-0001";
/// 가짜 Anthropic이 받지 않는 키.
pub const WRONG_AI_KEY: &str = "zz-round38-wrong-ai-key-0002";
/// 시험용 증권 키 칸 값.
pub const CLIENT_ID: &str = "zz-round38-client-id-0003";
pub const CLIENT_SECRET: &str = "zz-round38-client-secret-0004";
/// 가짜 토스가 받지 않는 비밀.
pub const WRONG_SECRET: &str = "zz-round38-wrong-secret-0005";
/// 마법사 환경의 `ANTHROPIC_API_KEY` 값(찍히면 안 된다).
pub const ENV_KEY_VALUE: &str = "zz-round38-env-anthropic-0006";
/// 다른 프로그램의 토큰 파일 모양을 한 미끼 글자(읽혀 어디에 나오면 안 된다).
pub const BAIT: &str = "zz-round38-bait-token-never-read";

/// 가짜 CLI가 알리는 버전.
const CLI_VERSION_TEXT: &str = "2.1.316 (Claude Code)";
const TOKEN_PATH: &str = "/oauth2/token";
const MODELS_PATH: &str = "/v1/models";

// ───────────── 임시 폴더 ─────────────

/// 임시 뿌리: 보관 폴더(`pi-home`, 만들지 않는다) · 사용자 홈(`user`, 미끼가 든다).
pub struct Root {
    _dir: TempDir,
    /// 보관 폴더.
    pub home: PathBuf,
    /// 사용자 홈(`HOME`).
    pub user: PathBuf,
}

impl Root {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        // macOS의 임시 폴더는 다른 경로를 거치므로 실제 경로로 바꿔 둔다.
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let user = base.join("user");
        for (folder, file) in [(".claude", ".credentials.json"), (".codex", "auth.json")] {
            std::fs::create_dir_all(user.join(folder)).unwrap();
            let bait = json!({ "claudeAiOauth": { "accessToken": BAIT, "refreshToken": BAIT } });
            std::fs::write(user.join(folder).join(file), bait.to_string()).unwrap();
        }
        Self {
            _dir: dir,
            home: base.join("pi-home"),
            user,
        }
    }

    pub fn home_str(&self) -> &str {
        self.home.to_str().unwrap()
    }
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

/// 보관 폴더의 비밀 보관 파일(`.env`)이 아닌 파일 가운데 `needle`이 든 것.
pub fn plain_files_with(home: &Path, needle: &str) -> Vec<PathBuf> {
    files_under(home)
        .into_iter()
        .filter(|file| {
            file.file_name()
                .is_none_or(|name| name != pi_secrets::ENV_FILE)
        })
        .filter(|file| {
            std::fs::read(file)
                .map(|bytes| String::from_utf8_lossy(&bytes).contains(needle))
                .unwrap_or(false)
        })
        .collect()
}

/// 아무도 듣지 않는 루프백 포트.
pub fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

// ───────────── 가짜 claude ─────────────

/// 가짜 CLI의 처음 상태.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Login {
    /// 설치돼 있고 구독으로 로그인돼 있다.
    In,
    /// 설치돼 있지만 로그인 안 됐다([`FakeCli::log_in`]으로 바꾼다).
    Out,
    /// `PATH`에 없다.
    Missing,
}

/// 임시 폴더의 가짜 `claude`.
pub struct FakeCli {
    _dir: TempDir,
    /// `PATH` 앞에 둘 폴더.
    pub bin_dir: PathBuf,
    /// 가짜 실행 파일 경로(없음이면 만들지 않는다).
    pub bin: PathBuf,
    log: PathBuf,
}

impl FakeCli {
    pub fn new(login: Login) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let bin_dir = root.join("bin");
        let log = root.join("log");
        std::fs::create_dir(&bin_dir).unwrap();
        std::fs::create_dir(&log).unwrap();
        let bin = bin_dir.join("claude");
        if login != Login::Missing {
            std::fs::write(&bin, script(&log)).unwrap();
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let cli = Self {
            _dir: dir,
            bin_dir,
            bin,
            log,
        };
        if login == Login::In {
            cli.log_in();
        }
        cli
    }

    /// 다른 창에서 `claude auth login`을 한 것처럼: 이제부터 로그인됐다고 답한다.
    pub fn log_in(&self) {
        std::fs::write(self.log.join("logged-in"), "").unwrap();
    }

    /// 마법사에게 줄 `PATH`.
    pub fn path_env(&self) -> String {
        format!("{}:/usr/bin:/bin", self.bin_dir.display())
    }

    /// 불린 인자 줄들.
    pub fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.log.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

/// `auth status --json`의 출력(공식 CLI의 모양).
fn auth_json(logged_in: bool) -> String {
    json!({
        "loggedIn": logged_in,
        "authMethod": "claude.ai",
        "apiProvider": "firstParty",
        "subscriptionType": "max",
    })
    .to_string()
}

/// 가짜 실행 파일의 셸 글. 모델 호출은 실패로 끝난다.
fn script(log: &Path) -> String {
    let log = log.display();
    format!(
        "#!/bin/sh\nL='{log}'\nprintf '%s\\n' \"$*\" >> \"$L/calls\"\ncase \"$1\" in\n\
--version)\nprintf '%s\\n' '{CLI_VERSION_TEXT}'\nexit 0\n;;\n\
auth)\nif [ -f \"$L/logged-in\" ]; then\nprintf '%s\\n' '{yes}'\nexit 0\nfi\n\
printf '%s\\n' '{no}'\nexit 1\n;;\nesac\n/bin/cat > /dev/null\nexit 1\n",
        yes = auth_json(true),
        no = auth_json(false),
    )
}

// ───────────── 도는 서버 ─────────────

/// 시험이 띄운 도는 서버와 그 가짜 바깥 서버들.
pub struct Bench {
    app: Option<App>,
    /// 서버가 여는 루프백 포트.
    pub port: u16,
    _toss: MockServer,
    _ai: MockServer,
}

impl Bench {
    /// `home`으로 띄운다. `extra`는 더 줄 설정 인자(예: `--register-attempts 1`).
    pub async fn start(home: &Path, extra: &[&str]) -> Bench {
        let toss = fake_toss().await;
        let ai = fake_ai().await;
        let mut list = vec!["--home", home.to_str().unwrap(), "--port", "0"];
        list.extend_from_slice(extra);
        let args: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
        let settings = Settings::parse(&args, &|_| None, None).expect("설정을 읽어야 한다");
        let wiring = Wiring {
            upstreams: Upstreams {
                toss: TossConfig {
                    base_url: toss.uri(),
                    timeout: Duration::from_secs(3),
                    token_failure_cooldown: Duration::ZERO,
                    ..TossConfig::default()
                },
                anthropic: AnthropicConfig {
                    base_url: ai.uri(),
                    timeout: Duration::from_secs(3),
                    max_retries: 0,
                    retry_base_delay: Duration::from_millis(1),
                },
            },
            clock: Arc::new(SystemClock),
            peer_override: None,
            cli_locations: Vec::new(),
        };
        let app = tokio::time::timeout(STEP, App::start(settings, wiring))
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        let port = app.local_addr().port();
        Bench {
            app: Some(app),
            port,
            _toss: toss,
            _ai: ai,
        }
    }

    fn addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.port))
    }

    /// 화면과 같은 길로 설정 상태(`GET /api/setup`)를 읽는다.
    pub async fn status(&self) -> Value {
        let origin = format!("http://{}", self.addr());
        let client = reqwest::Client::new();
        let reply = client
            .post(format!("{origin}{}", pi_gate::SESSION_PATH))
            .header("Origin", &origin)
            .send()
            .await
            .expect("세션 요청이 닿아야 한다");
        let cookie = reply
            .headers()
            .get("set-cookie")
            .expect("세션 쿠키가 있어야 한다")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .trim()
            .to_string();
        let reply = client
            .get(format!("{origin}{SETUP_PATH}"))
            .header("Cookie", cookie)
            .send()
            .await
            .expect("설정 상태 요청이 닿아야 한다");
        assert_eq!(reply.status().as_u16(), 200, "설정 상태를 읽는다");
        reply.json().await.expect("설정 상태는 JSON이다")
    }

    pub async fn stop(mut self) {
        if let Some(app) = self.app.take() {
            tokio::time::timeout(STEP, app.shutdown())
                .await
                .expect("시간 안에 꺼져야 한다")
                .expect("끄기가 성공해야 한다");
        }
    }
}

/// 설정 상태에서 그 항목이 등록됐는가.
pub fn registered(status: &Value, name: &str) -> bool {
    status["items"]
        .as_array()
        .expect("items는 목록이다")
        .iter()
        .find(|item| item["name"] == json!(name))
        .and_then(|item| item["registered"].as_bool())
        .unwrap_or_else(|| panic!("설정 상태에 {name} 항목이 있어야 한다: {status}"))
}

/// 가짜 토스: 토큰 발급만. 본문에 시험용 비밀이 있으면 토큰, 아니면 `invalid_client`.
async fn fake_toss() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN_PATH))
        .respond_with(|req: &Request| {
            if String::from_utf8_lossy(&req.body).contains(CLIENT_SECRET) {
                ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "zz-round38-toss-token", "token_type": "Bearer", "expires_in": 86_400
                }))
            } else {
                ResponseTemplate::new(401).set_body_json(json!({
                    "error": "invalid_client", "error_description": "자격이 맞지 않습니다"
                }))
            }
        })
        .mount(&server)
        .await;
    server
}

/// 가짜 Anthropic: 모델 목록만. 시험용 키면 200, 아니면 401.
async fn fake_ai() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(MODELS_PATH))
        .respond_with(|req: &Request| {
            let ok = req
                .headers
                .get("x-api-key")
                .is_some_and(|v| v.as_bytes() == AI_KEY.as_bytes());
            if ok {
                ResponseTemplate::new(200).set_body_json(json!({
                    "data": [], "has_more": false, "first_id": null, "last_id": null
                }))
            } else {
                ResponseTemplate::new(401).set_body_json(json!({
                    "type": "error",
                    "error": { "type": "authentication_error", "message": "invalid x-api-key" }
                }))
            }
        })
        .mount(&server)
        .await;
    server
}
