//! price-inha 서버: docs/dev/modules/gate.md 를 따른다.
//!
//! 얇은 실행 파일이다: 하위 명령을 읽는다(9차, `docs/dev/modules/server.md` "9차 작업").
//! 인자 없이 띄우거나 `run`이면 설정을 읽고, 띄우고, Ctrl+C를 기다렸다가 끈다(지금 동작 그대로).
//! `service` · `doctor` · `access-key` · `restore`는 라이브러리의 같은 이름 함수를 부르고 끝난다.
//! `--version`은 설정 · 보관 폴더를 읽기 전에 답한다. 조립은 `lib.rs` 쪽에 있다.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use pi_secrets::SecretStore;
use pi_store::UnixMillis;
use price_inha::{
    AccessKeyRequest, App, AppError, CliError, Command, InstallOptions, Launch, Reach,
    RedactingWriter, RestoreRequest, Service, ServiceAction, ServiceReport, ServiceSetup, Settings,
    SettingsError, SystemRunner, TeamWiring, TradingWiring, VERSION, Wiring, access_key,
    announce_addr, doctor, restore,
};

/// 설정 · 명령을 읽지 못했을 때 보여 주는 쓰는 법. 비밀 값을 받는 인자는 없다.
const USAGE: &str = "쓰는 법: price-inha [run] [--home <폴더>] [--bind <IP>] [--port <수>] \
[--symbols <a,b,c>] [--market <kr|us>] [--interval-ms <수>] \
[--retry-initial-ms <수>] [--retry-max-ms <수>] [--calendar-retry-ms <수>] \
[--register-attempts <수>] [--register-window-ms <수>] [--demo]
  price-inha service install [--start-now] [--dry-run] [설정 인자들]
  price-inha service uninstall [--dry-run] [--home <폴더>]
  price-inha service start|stop|restart|status [--home <폴더>]
  price-inha doctor [--json] [설정 인자들]
  price-inha access-key [--rotate] [--home <폴더>]
  price-inha restore <백업 파일> --identity <age 비밀키 파일> --into <빈 폴더>
  price-inha --version";

/// 설정 치명 오류의 종료 코드(서비스 관리자가 이 코드로는 다시 띄우지 않는다).
const SETTINGS_EXIT: u8 = price_inha::CONFIG_FATAL_EXIT;

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

/// 답을 표준 출력에 쓴다(쓰지 못해도 할 수 있는 일이 없다).
fn show(text: &str) {
    let _ = write!(std::io::stdout(), "{text}");
    let _ = std::io::stdout().flush();
}

/// 기록(tracing)을 표준 오류로 낸다. 보관 폴더의 비밀 보관소를 열 수 있으면 그 값들을 가린 뒤에만 낸다
/// ([`RedactingWriter`], E38 · N8). 열 수 없으면 등록된 비밀도 없으므로 그대로 낸다.
///
/// 연 보관소는 앱에도 그대로 준다([`App::launch_with`]): 띄운 뒤 등록한 키도 가리게.
fn init_tracing(settings: Option<&Settings>) -> Option<Arc<SecretStore>> {
    let secrets = settings
        .and_then(|s| SecretStore::open(s.home.clone()).ok())
        .map(Arc::new);
    let builder = tracing_subscriber::fmt().with_max_level(tracing::Level::INFO);
    match &secrets {
        Some(secrets) => builder
            .with_writer(RedactingWriter::new(Arc::clone(secrets), std::io::stderr))
            .init(),
        None => builder.with_writer(std::io::stderr).init(),
    }
    secrets
}

/// 프로그램 이름을 뺀 인자. 글자로 읽을 수 없는 인자는 말없이 버리지 않고 거부한다.
fn process_args() -> Result<Vec<String>, SettingsError> {
    std::env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| SettingsError::UnknownArgument)
        })
        .collect()
}

/// 설정을 읽지 못했을 때: 까닭과 쓰는 법, 종료 코드 2.
fn settings_failed(error: &SettingsError) -> ExitCode {
    say(&format!("설정을 읽지 못했습니다: {error}"));
    say(USAGE);
    ExitCode::from(SETTINGS_EXIT)
}

fn main() -> ExitCode {
    let args = match process_args() {
        Ok(args) => args,
        Err(error) => return settings_failed(&error),
    };
    let command = match Command::parse(&args) {
        Ok(command) => command,
        Err(error) => {
            say(&format!("명령을 읽지 못했습니다: {error}"));
            say(USAGE);
            return ExitCode::from(SETTINGS_EXIT);
        }
    };
    // 판 번호는 설정 · 보관 폴더 · 기록을 건드리기 전에 답한다(Hermes 2.2).
    if matches!(command, Command::Version) {
        show(&format!("price-inha {VERSION}\n"));
        return ExitCode::SUCCESS;
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            say(&format!("실행 준비를 하지 못했습니다: {}", error.kind()));
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(dispatch(command))
}

/// 하위 명령을 한다.
async fn dispatch(command: Command) -> ExitCode {
    match command {
        Command::Run { args } => serve(&args).await,
        Command::Service { action, args } => match settings_of(&args) {
            Ok(settings) => service_command(action, &settings, &args),
            Err(code) => code,
        },
        Command::Doctor { json, args } => match settings_of(&args) {
            Ok(settings) => doctor_command(json, &settings),
            Err(code) => code,
        },
        Command::AccessKey { rotate, args } => match settings_of(&args) {
            Ok(settings) => access_key_command(rotate, &settings).await,
            Err(code) => code,
        },
        Command::Restore {
            backup,
            identity,
            into,
        } => restore_command(backup, identity, into),
        Command::Version => {
            show(&format!("price-inha {VERSION}\n"));
            ExitCode::SUCCESS
        }
        _ => {
            say(USAGE);
            ExitCode::from(SETTINGS_EXIT)
        }
    }
}

/// 하위 명령의 설정 인자를 읽고 기록을 켠다. 읽지 못하면 종료 코드 2.
fn settings_of(args: &[String]) -> Result<Settings, ExitCode> {
    match Settings::from_process_env(args) {
        Ok(settings) => {
            init_tracing(Some(&settings));
            Ok(settings)
        }
        Err(error) => Err(settings_failed(&error)),
    }
}

/// 명령이 실패했을 때: 까닭(비밀 값 없음), 종료 코드 1.
fn failed(error: &CliError) -> ExitCode {
    say(&error.to_string());
    ExitCode::FAILURE
}

/// 포그라운드로 띄운다(지금 동작).
async fn serve(args: &[String]) -> ExitCode {
    let settings = Settings::from_process_env(args);
    let secrets = init_tracing(settings.as_ref().ok());
    match run(settings, secrets).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Settings(error)) => settings_failed(&error),
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

/// 서비스 정의가 띄울 `run` 인자: 준 설정 인자에서 `--home <폴더>`를 뺀 것(정의는 절대 경로로 따로 싣는다).
fn without_home(args: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg == "--home" {
            rest.next();
        } else {
            kept.push(arg.clone());
        }
    }
    kept
}

/// 서비스 명령이 한(할) 일을 보여 준다.
fn show_report(report: &ServiceReport, done: &str) {
    let mut text = String::new();
    if report.dry_run {
        text.push_str("할 일만 보여 줍니다(쓰지도 부르지도 않았습니다).\n");
    } else {
        text.push_str(&format!("{done}\n"));
    }
    if let Some(definition) = &report.definition {
        text.push_str(&format!("정의 파일: {}\n", definition.path.display()));
        if report.dry_run {
            text.push_str(&format!("{}\n", definition.contents));
        }
    }
    for command in &report.commands {
        text.push_str(&format!(
            "  $ {} {}\n",
            command.program,
            command.args.join(" ")
        ));
    }
    for note in &report.notes {
        text.push_str(&format!("안내: {note}\n"));
    }
    show(&text);
}

fn service_command(action: ServiceAction, settings: &Settings, args: &[String]) -> ExitCode {
    let setup = match ServiceSetup::for_this_computer(&settings.home, without_home(args)) {
        Ok(setup) => setup,
        Err(error) => return failed(&CliError::from(error)),
    };
    let runner = SystemRunner;
    let service = Service::new(setup, &runner);
    let result = match action {
        ServiceAction::Install { start_now, dry_run } => service
            .install(InstallOptions { start_now, dry_run })
            .map(|report| show_report(&report, "서비스를 등록했습니다.")),
        ServiceAction::Uninstall { dry_run } => service
            .uninstall(dry_run)
            .map(|report| show_report(&report, "서비스를 멈추고 정의를 지웠습니다.")),
        ServiceAction::Start => service
            .start()
            .map(|report| show_report(&report, "서비스를 띄웠습니다.")),
        ServiceAction::Stop => service
            .stop()
            .map(|report| show_report(&report, "서비스를 멈췄습니다.")),
        ServiceAction::Restart => service
            .restart()
            .map(|report| show_report(&report, "서비스를 다시 띄웠습니다.")),
        ServiceAction::Status => service.status().map(|status| {
            let home = status
                .home
                .map_or_else(|| "없음".to_string(), |home| home.display().to_string());
            show(&format!(
                "등록: {}\n도는 중: {}\n가리키는 보관 폴더: {home}\n",
                if status.installed { "예" } else { "아니요" },
                if status.running { "예" } else { "아니요" },
            ));
        }),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => failed(&CliError::from(error)),
    }
}

fn doctor_command(json: bool, settings: &Settings) -> ExitCode {
    let setup = match ServiceSetup::for_this_computer(&settings.home, Vec::new()) {
        Ok(setup) => setup,
        Err(error) => return failed(&CliError::from(error)),
    };
    let runner = SystemRunner;
    let report = doctor(settings, &Service::new(setup, &runner));
    if json {
        show(&format!("{}\n", report.to_json()));
    } else {
        show(&report.render());
    }
    ExitCode::from(report.exit_code())
}

async fn access_key_command(rotate: bool, settings: &Settings) -> ExitCode {
    // 원격 이름(https 프록시 뒤 이름)은 아직 서버 설정에 칸이 없다. 생기면 여기서 넘긴다.
    let request = AccessKeyRequest {
        rotate,
        remote_name: None,
    };
    match access_key(settings, &request).await {
        Ok(shown) => {
            show(shown.text());
            ExitCode::SUCCESS
        }
        Err(error) => failed(&error),
    }
}

fn restore_command(backup: PathBuf, identity: PathBuf, into: PathBuf) -> ExitCode {
    init_tracing(None);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        });
    let request = RestoreRequest {
        backup,
        identity,
        into,
    };
    match restore(&request, UnixMillis(now)) {
        Ok(summary) => {
            show(&summary.text);
            ExitCode::SUCCESS
        }
        Err(error) => failed(&error),
    }
}

async fn run(
    settings: Result<Settings, SettingsError>,
    secrets: Option<Arc<SecretStore>>,
) -> Result<(), Failure> {
    let settings = settings.map_err(Failure::Settings)?;
    let demo = settings.demo;
    // 보관소를 열지 못했으면 앱이 다시 열어 보고 그 오류를 알린다.
    // (W5 2차) 실행 파일은 투자 팀(에이전트 · 자료 · 트리거 · 복기)까지 잇는다.
    let launch = Launch {
        trading: TradingWiring::default(),
        team: Some(TeamWiring::official()),
        secrets,
    };
    // (W5 3차) 사무실 · 폰 알림 · 지식 검색까지 잇는다. 백업은 설정 화면에서 고른 저장소 · 공개키
    // (보관 폴더의 `backup-settings.json`)로 앱이 켤 때 잇는다. 그래서 여기 `Reach`의 백업 칸은 비워 둔다.
    let started = App::launch_with(settings, Wiring::default(), launch, Reach::default()).await;
    let app = started.map_err(Failure::App)?;
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
