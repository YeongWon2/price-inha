//! price-inha 서버: docs/dev/modules/gate.md 를 따른다.
//!
//! 얇은 실행 파일이다: 설정을 읽고, 띄우고, Ctrl+C를 기다렸다가 끈다. 조립은 `lib.rs` 쪽에 있다.

use std::io::Write;
use std::process::ExitCode;

use price_inha::{App, AppError, Settings, SettingsError, Wiring, announce_addr};

/// 설정을 읽지 못했을 때 보여 주는 쓰는 법. 비밀 값을 받는 인자는 없다.
const USAGE: &str = "쓰는 법: price-inha [--home <폴더>] [--bind <IP>] [--port <수>] \
[--symbols <a,b,c>] [--market <kr|us>] [--interval-ms <수>] \
[--retry-initial-ms <수>] [--retry-max-ms <수>] [--calendar-retry-ms <수>] \
[--register-attempts <수>] [--register-window-ms <수>] [--demo]";

/// 끝난 까닭. 어느 쪽도 비밀 값을 담지 않는다.
enum Failure {
    Settings(SettingsError),
    App(AppError),
    Signal,
}

/// 사람이 읽을 글을 표준 오류에 쓴다(쓰지 못해도 할 수 있는 일이 없다).
fn say(text: &str) {
    let _ = writeln!(std::io::stderr(), "price-inha: {text}");
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::INFO)
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Settings(error)) => {
            say(&format!("설정을 읽지 못했습니다: {error}"));
            say(USAGE);
            ExitCode::from(2)
        }
        Err(Failure::App(error)) => {
            say(&format!("서버 오류: {error}"));
            ExitCode::FAILURE
        }
        Err(Failure::Signal) => {
            say("끄기 신호를 기다릴 수 없습니다");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Failure> {
    let settings = Settings::from_process().map_err(Failure::Settings)?;
    let demo = settings.demo;
    let app = App::start(settings, Wiring::default())
        .await
        .map_err(Failure::App)?;
    let mode = if demo {
        " (시연용 가짜 시세)"
    } else {
        " (필수 설정이 빠져 있으면 이 주소의 설정 화면에서 등록합니다)"
    };
    say(&format!(
        "http://{} 에서 열렸습니다{mode}. 끄려면 Ctrl+C",
        // 모든 주소로 열었으면 실제로 들어올 수 있는 주소(루프백)를 알려 준다.
        announce_addr(app.local_addr())
    ));
    let waited = wait_for_stop().await;
    say("끄는 중입니다");
    // 신호를 기다리지 못했어도 연결과 저장소는 정리하고 끝낸다.
    app.shutdown().await.map_err(Failure::App)?;
    waited.map_err(|_| Failure::Signal)
}

/// Ctrl+C(유닉스에서는 SIGTERM도)를 기다린다.
#[cfg(unix)]
async fn wait_for_stop() -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result,
        _ = terminate.recv() => Ok(()),
    }
}

/// Ctrl+C를 기다린다.
#[cfg(not(unix))]
async fn wait_for_stop() -> std::io::Result<()> {
    tokio::signal::ctrl_c().await
}
