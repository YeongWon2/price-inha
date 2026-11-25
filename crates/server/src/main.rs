//! price-inha 서버: docs/dev/modules/gate.md 를 따른다.
//!
//! 얇은 실행 파일이다: 하위 명령을 읽는다(9차, `docs/dev/modules/server.md` "9차 작업").
//! 인자 없이 띄우거나 `run`이면 설정을 읽고, 띄우고, Ctrl+C를 기다렸다가 끈다(지금 동작 그대로).
//! `service` · `doctor` · `access-key` · `restore`는 라이브러리의 같은 이름 함수를 부르고 끝난다.
//! `--version`은 설정 · 보관 폴더를 읽기 전에 답한다. 조립은 `lib.rs` 쪽에 있다.
//!
//! 서버를 띄우지 않는 명령(`doctor` · `service …` · `restore` · `--version`)은 보관 폴더를 만들거나 고치지
//! 않는다: 비밀 보관소를 여는 것(기록 가리기)은 `run`과 `access-key`뿐이다. `run --log-file <경로>`를 주면
//! 기록을 표준 오류 대신 그 파일에 덧붙여 쓴다(가리기는 그대로 거친다).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use pi_secrets::SecretStore;
use pi_store::UnixMillis;
use price_inha::{
    AccessKeyRequest, App, AppError, CliError, Command, InstallOptions, OpenRequest,
    RedactingWriter, RestoreRequest, Service, ServiceAction, ServiceReport, ServiceSetup, Settings,
    SettingsError, SetupContext, SetupRequest, SystemRunner, VERSION, access_key, access_key_with,
    announce_addr, doctor, install_logging, restore, run_assembly,
};
use tracing_subscriber::fmt::writer::BoxMakeWriter;

/// 설정 · 명령을 읽지 못했을 때 보여 주는 쓰는 법. 비밀 값을 받는 인자는 없다.
const USAGE: &str = "쓰는 법: price-inha [run] [--home <폴더>] [--bind <IP>] [--port <수>] \
[--symbols <a,b,c>] [--market <kr|us>] [--interval-ms <수>] \
[--retry-initial-ms <수>] [--retry-max-ms <수>] [--calendar-retry-ms <수>] \
[--register-attempts <수>] [--register-window-ms <수>] [--log-file <경로>] [--demo]
  price-inha service install [--start-now] [--dry-run] [설정 인자들]
  price-inha service uninstall [--dry-run] [--home <폴더>]
  price-inha service start|stop|restart|status [--home <폴더>]
  price-inha doctor [--json] [설정 인자들]
  price-inha access-key [--rotate] [--home <폴더>]
  price-inha restore <백업 파일> --identity <age 비밀키 파일> --into <빈 폴더>
  price-inha setup [ai|broker|symbols|messenger|service|open] [--dry-run] [설정 인자들]
  price-inha open [home|office|account|templates|rules|history] [--print] [설정 인자들]
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

/// 기록 파일을 덧붙여 쓰게 연다. 부모 폴더가 없으면 만든다. 유닉스에서는 새 파일을 0600으로 만든다.
fn open_log_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        // 이미 있던 파일이 넓게 열려 있으면 좁힌다.
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

/// 기록이 가는 곳: `--log-file`을 줬고 열 수 있으면 그 파일, 아니면 표준 오류.
fn log_sink(log_file: Option<&Path>) -> BoxMakeWriter {
    let Some(path) = log_file else {
        return BoxMakeWriter::new(std::io::stderr);
    };
    match open_log_file(path) {
        Ok(file) => BoxMakeWriter::new(Mutex::new(file)),
        Err(error) => {
            say(&format!(
                "기록 파일을 열지 못해 표준 오류에 기록합니다: {}",
                error.kind()
            ));
            BoxMakeWriter::new(std::io::stderr)
        }
    }
}

/// 서버를 띄우는 길(`run`, 직접 만드는 `access-key`)의 기록(tracing). 보관 폴더의 비밀 보관소를 열 수 있으면
/// 그 값들을 가린 뒤에만 낸다([`RedactingWriter`], E38 · N8). 열 수 없으면 등록된 비밀도 없으므로 그대로 낸다.
/// `create`가 거짓이면 보관 폴더가 이미 있을 때만 연다(없는 폴더에는 가릴 비밀도 없다. 만들지 않는다).
///
/// 연 보관소는 앱에도 그대로 준다([`App::launch_with`]): 띄운 뒤 등록한 키도 가리게.
fn init_tracing(settings: Option<&Settings>, create: bool) -> Option<Arc<SecretStore>> {
    let secrets = settings
        .filter(|s| create || s.home.is_dir())
        .and_then(|s| SecretStore::open(s.home.clone()).ok())
        .map(Arc::new);
    let log_file = settings.and_then(|s| s.log_file.as_deref());
    let sink = log_sink(log_file);
    // 파일에는 색 글자(터미널 제어 글자)를 쓰지 않는다. (39차 9번) HTTP 라이브러리는 경고만 남긴다.
    let ansi = log_file.is_none();
    match &secrets {
        Some(secrets) => install_logging(RedactingWriter::new(Arc::clone(secrets), sink), ansi),
        None => install_logging(sink, ansi),
    }
    secrets
}

/// 서버를 띄우지 않는 명령(`doctor` · `service …` · `restore`)의 기록: 가리기 없이 표준 오류로 낸다.
/// 보관 폴더를 열지 않는다(`SecretStore::open`은 폴더를 만들고 권한을 고친다 — 9차 보강 2).
/// 이 명령들은 비밀 값을 읽어 기록하지 않는다.
fn init_plain_tracing() {
    install_logging(std::io::stderr, true);
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
            Ok(settings) => {
                init_plain_tracing();
                service_command(action, &settings, &args)
            }
            Err(code) => code,
        },
        Command::Doctor { json, args } => match settings_of(&args) {
            Ok(settings) => {
                init_plain_tracing();
                doctor_command(json, &settings)
            }
            Err(code) => code,
        },
        Command::AccessKey { rotate, args } => match settings_of(&args) {
            Ok(settings) => {
                // 직접 만드는 길은 보관 폴더를 스스로 만든다. 여기서는 이미 있는 폴더의 비밀만 가린다
                // (그 포트가 다른 프로그램이면 아무것도 만들지 않고 끝나야 한다).
                init_tracing(Some(&settings), false);
                access_key_command(rotate, &settings).await
            }
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
        Command::Setup {
            section,
            stdin_answers,
            dry_run,
            args,
        } => match settings_of(&args) {
            Ok(settings) => {
                let request = SetupRequest {
                    section,
                    stdin_answers,
                    dry_run,
                    args,
                };
                setup_command(&settings, &request).await
            }
            Err(code) => code,
        },
        Command::Open {
            screen,
            print,
            args,
        } => match settings_of(&args) {
            Ok(settings) => {
                let request = OpenRequest { screen, print };
                let code =
                    price_inha::open(&settings, &request, &mut std::io::stdout(), &SystemRunner)
                        .await;
                ExitCode::from(code)
            }
            Err(code) => code,
        },
        _ => {
            say(USAGE);
            ExitCode::from(SETTINGS_EXIT)
        }
    }
}

/// (37차) 처음 설정 마법사: 실제 표준 입출력 · 프로세스 환경 · 자기 경로로 부른다.
async fn setup_command(settings: &Settings, request: &SetupRequest) -> ExitCode {
    use std::io::IsTerminal;
    let stdin = std::io::stdin();
    let interactive = stdin.is_terminal();
    let mut input = stdin.lock();
    let mut output = std::io::stdout();
    let env = |name: &str| std::env::var(name).ok();
    let server_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("price-inha"));
    let context = SetupContext {
        input: &mut input,
        output: &mut output,
        env: &env,
        interactive,
        server_exe,
    };
    ExitCode::from(price_inha::setup(settings, request, context).await)
}

/// 하위 명령의 설정 인자를 읽는다(기록은 명령마다 따로 켠다). 읽지 못하면 종료 코드 2.
fn settings_of(args: &[String]) -> Result<Settings, ExitCode> {
    Settings::from_process_env(args).map_err(|error| settings_failed(&error))
}

/// 명령이 실패했을 때: 까닭(비밀 값 없음), 종료 코드 1.
fn failed(error: &CliError) -> ExitCode {
    say(&error.to_string());
    ExitCode::FAILURE
}

/// 포그라운드로 띄운다(지금 동작).
async fn serve(args: &[String]) -> ExitCode {
    let settings = Settings::from_process_env(args);
    let secrets = init_tracing(settings.as_ref().ok(), true);
    // 기록 파일로 띄운 것(예약 작업)은 표준 오류를 볼 수 없다: 뜨지 못한 까닭을 기록에도 남긴다(가리기를 거친다).
    let to_file = settings.as_ref().is_ok_and(|s| s.log_file.is_some());
    let fail = |text: &str| {
        if to_file {
            tracing::error!("{text}");
        }
        say(text);
        ExitCode::FAILURE
    };
    match run(settings, secrets).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Settings(error)) => settings_failed(&error),
        Err(Failure::App(error)) => fail(&format!("서버 오류: {error}")),
        Err(Failure::Signal) => fail("끄기 신호를 기다릴 수 없습니다"),
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
    // 서비스 정의가 이 홈을 가리키면 그 정의의 주소에서도 도는 서버를 찾는다(10차 10a 0번).
    let runner = SystemRunner;
    let issued = match ServiceSetup::for_this_computer(&settings.home, Vec::new()) {
        Ok(setup) => access_key_with(settings, &request, &Service::new(setup, &runner)).await,
        Err(_) => access_key(settings, &request).await,
    };
    match issued {
        Ok(shown) => {
            show(shown.text());
            ExitCode::SUCCESS
        }
        Err(error) => failed(&error),
    }
}

fn restore_command(backup: PathBuf, identity: PathBuf, into: PathBuf) -> ExitCode {
    init_plain_tracing();
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
    // (W5 2차 · 3차 · 24차) 실행 파일의 조립은 라이브러리의 `run_assembly`에 있다(시험도 같은 조립을 쓴다):
    // 투자 팀 · 사무실 · 폰 알림 · 지식 검색까지 잇고, `--demo`면 시연 달력 · 시연용 가짜 공시를 켠다.
    let assembly = run_assembly(&settings, secrets);
    let started = App::launch_run(settings, assembly).await;
    let app = started.map_err(Failure::App)?;
    let mode = if demo {
        // (24차 4번) 시연 모드는 바깥으로 나가지 않는다(키가 등록돼 있어도).
        " (시연 모드: 가짜 시세 · 가짜 AI · 모의계좌)"
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
