//! 15차 시험의 도우미: 시연 모드 서버 띄우기 · 끄기.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_market::TossConfig;
use price_inha::{App, Settings, Upstreams, Wiring};

/// 한 걸음의 기다림 상한.
pub const STEP: Duration = Duration::from_secs(15);

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

/// 시연 모드로 띄운 서버.
pub struct Bench {
    _dir: tempfile::TempDir,
    app: App,
}

impl Bench {
    pub async fn start() -> Bench {
        let dir = tempfile::tempdir().unwrap();
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
        Bench { _dir: dir, app }
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}
