//! 사용자 서비스 정의(macOS launchd 사용자 에이전트 · Linux systemd 사용자 unit · Windows 예약 작업).
//!
//! 원본 동작: `docs/research/install-and-cli.md` 2.4(Hermes) · 3.4(OpenClaw). 칸은 지시서 "서비스 정의" 표.
//! 바깥 명령은 [`CommandRunner`] 뒤에서 부르고, 정의를 쓰는 폴더는 [`ServiceSetup::definition_dir`]로 받는다.

use std::path::{Path, PathBuf};

use crate::cli::runner::{CommandLine, CommandRunner};
use crate::error::ServiceError;

mod listen;
mod render;
mod steps;

use render::{between, plist_working_dir, xml_unescape};
use steps::{OnFailure, Step, plan_report};

/// macOS 사용자 에이전트 이름(`Label`).
pub const LAUNCHD_LABEL: &str = "kr.priceinha.server";
/// macOS 정의 파일 이름(`~/Library/LaunchAgents/` 아래).
pub const LAUNCHD_FILE: &str = "kr.priceinha.server.plist";
/// Linux 사용자 unit 이름(`~/.config/systemd/user/` 아래).
pub const SYSTEMD_UNIT: &str = "price-inha.service";
/// Windows 예약 작업 이름.
pub const WINDOWS_TASK: &str = "PriceInha";
/// Windows 예약 작업 XML을 두는 파일 이름(정의 폴더 아래).
pub const WINDOWS_TASK_FILE: &str = "PriceInha.xml";
/// 설정 치명 오류의 종료 코드. 서비스 관리자가 이 코드로는 다시 띄우지 않는다.
pub const CONFIG_FATAL_EXIT: u8 = 2;
/// 보관 폴더 아래 기록 폴더 이름.
pub const SERVER_LOGS_DIR: &str = "logs";
/// 표준 출력 기록 파일 이름(macOS).
pub const SERVER_LOG: &str = "server.log";
/// 표준 오류 기록 파일 이름(macOS).
pub const SERVER_ERROR_LOG: &str = "server.error.log";

/// 서비스 관리자가 다른 운영체제.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// launchd 사용자 에이전트.
    MacOs,
    /// systemd 사용자 unit.
    Linux,
    /// 예약 작업.
    Windows,
}

impl Os {
    /// 이 실행 파일이 빌드된 운영체제(`cfg!(target_os)`). 셋 밖이면 없음.
    pub fn current() -> Option<Os> {
        if cfg!(target_os = "macos") {
            Some(Os::MacOs)
        } else if cfg!(target_os = "linux") {
            Some(Os::Linux)
        } else if cfg!(target_os = "windows") {
            Some(Os::Windows)
        } else {
            None
        }
    }
}

/// 서비스 정의를 만들 재료. 시험은 운영체제 · 정의 폴더 · uid를 직접 준다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSetup {
    /// 운영체제.
    pub os: Os,
    /// 띄울 실행 파일의 절대 경로.
    pub exe: PathBuf,
    /// 보관 폴더(절대 경로). 정의의 `run --home`과 작업 폴더가 된다.
    pub home: PathBuf,
    /// 정의 파일을 쓰는 폴더(macOS `~/Library/LaunchAgents`, Linux `~/.config/systemd/user`,
    /// Windows는 작업 XML을 두는 곳).
    pub definition_dir: PathBuf,
    /// 사용자 번호(macOS `gui/<uid>` 영역).
    pub uid: u32,
    /// 정의에 명시하는 `PATH` 값. 환경은 이것만 싣는다(비밀 값은 싣지 않는다).
    pub path_env: String,
    /// `run --home <home>` 뒤에 붙일 지금 준 설정 인자(`--bind` · `--port` 등). `--home`은 빼고 준다.
    pub run_args: Vec<String>,
}

impl ServiceSetup {
    /// 이 컴퓨터의 기본값(지금 실행 파일 · 사용자 홈 아래 정의 폴더 · 지금 uid · 지금 `PATH`)으로 만든다.
    pub fn for_this_computer(
        home: &Path,
        run_args: Vec<String>,
    ) -> Result<ServiceSetup, ServiceError> {
        let os = Os::current().ok_or(ServiceError::UnsupportedOs)?;
        if !home.is_absolute() {
            return Err(ServiceError::RelativeHome);
        }
        let exe = std::env::current_exe().map_err(io_error)?;
        let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
        let user_home = std::env::home_dir().ok_or(ServiceError::Io {
            kind: std::io::ErrorKind::NotFound,
        })?;
        let definition_dir = match os {
            Os::MacOs => user_home.join("Library").join("LaunchAgents"),
            Os::Linux => std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|dir| dir.is_absolute())
                .unwrap_or_else(|| user_home.join(".config"))
                .join("systemd")
                .join("user"),
            Os::Windows => std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .filter(|dir| dir.is_absolute())
                .unwrap_or_else(|| user_home.join("AppData").join("Local"))
                .join(WINDOWS_TASK),
        };
        Ok(ServiceSetup {
            os,
            exe,
            home: home.to_path_buf(),
            definition_dir,
            uid: current_uid(&user_home),
            path_env: std::env::var("PATH").unwrap_or_else(|_| DEFAULT_PATH.to_string()),
            run_args,
        })
    }
}

/// `PATH`를 읽지 못했을 때 정의에 싣는 값.
const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

/// 지금 사용자 번호. 사용자 홈 폴더의 주인으로 본다(유닉스). 유닉스가 아니면 쓰지 않으므로 0.
#[cfg(unix)]
fn current_uid(user_home: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(user_home)
        .map(|meta| meta.uid())
        .unwrap_or(0)
}

#[cfg(not(unix))]
fn current_uid(_user_home: &Path) -> u32 {
    0
}

fn io_error(error: std::io::Error) -> ServiceError {
    ServiceError::Io { kind: error.kind() }
}

/// 쓸(쓴) 정의 파일.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceDefinition {
    /// 정의 파일 경로(정의 폴더 아래).
    pub path: PathBuf,
    /// 내용(plist · unit · 작업 XML).
    pub contents: String,
}

/// `service install`의 선택.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InstallOptions {
    /// 등록하고 바로 띄운다.
    pub start_now: bool,
    /// 쓰지도 부르지도 않고 할 일만 돌려준다.
    pub dry_run: bool,
}

/// 서비스 명령이 한(할) 일.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceReport {
    /// 쓴(쓸) 정의. 정의를 쓰지 않는 명령이면 없음.
    pub definition: Option<ServiceDefinition>,
    /// 부른(부를) 바깥 명령, 부른 순서대로.
    pub commands: Vec<CommandLine>,
    /// 사람이 할 일 안내(예: linger를 켜지 못했을 때의 수동 명령).
    pub notes: Vec<String>,
    /// 할 일만 보여 줬나.
    pub dry_run: bool,
}

/// `service status`의 답.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    /// 정의가 등록돼 있나.
    pub installed: bool,
    /// 서비스 관리자가 도는 중이라고 답하나.
    pub running: bool,
    /// 정의가 가리키는 보관 폴더.
    pub home: Option<PathBuf>,
}

/// 서비스 관리.
pub struct Service<'a> {
    setup: ServiceSetup,
    runner: &'a dyn CommandRunner,
}

impl<'a> Service<'a> {
    /// 재료와 바깥 명령 실행기로 만든다.
    pub fn new(setup: ServiceSetup, runner: &'a dyn CommandRunner) -> Service<'a> {
        Service { setup, runner }
    }

    /// 재료.
    pub fn setup(&self) -> &ServiceSetup {
        &self.setup
    }

    /// 이 재료로 쓸 정의(파일을 쓰지 않는다).
    pub fn definition(&self) -> ServiceDefinition {
        let setup = &self.setup;
        let (path, contents) = match setup.os {
            Os::MacOs => (setup.definition_dir.join(LAUNCHD_FILE), self.plist()),
            Os::Linux => (setup.definition_dir.join(SYSTEMD_UNIT), self.unit()),
            Os::Windows => (
                setup.definition_dir.join(WINDOWS_TASK_FILE),
                self.task_xml(),
            ),
        };
        ServiceDefinition { path, contents }
    }

    /// 이미 있는 정의가 가리키는 보관 폴더. 정의가 없으면 없음.
    pub fn installed_home(&self) -> Result<Option<PathBuf>, ServiceError> {
        let path = self.definition_path();
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
        };
        let home = match self.setup.os {
            Os::MacOs => plist_working_dir(&contents),
            Os::Linux => contents
                .lines()
                .find_map(|line| line.trim().strip_prefix("WorkingDirectory="))
                .map(|dir| dir.trim().trim_matches('"').to_string()),
            Os::Windows => {
                between(&contents, "<WorkingDirectory>", "</WorkingDirectory>").map(xml_unescape)
            }
        };
        // 정의 파일은 있는데 가리키는 홈을 읽을 수 없으면, 모르는 정의를 덮어쓰지 않게 오류로 둔다.
        home.map(|home| Some(PathBuf::from(home)))
            .ok_or(ServiceError::Io {
                kind: std::io::ErrorKind::InvalidData,
            })
    }

    /// 정의를 쓰고 등록한다. 다른 보관 폴더를 가리키는 기존 정의가 있으면 [`ServiceError::OtherHome`]이고
    /// 아무것도 바꾸지 않는다. 같은 홈이면 새로 쓴다. `dry_run`이면 쓰지도 부르지도 않는다.
    pub fn install(&self, options: InstallOptions) -> Result<ServiceReport, ServiceError> {
        if !self.setup.home.is_absolute() {
            return Err(ServiceError::RelativeHome);
        }
        if let Some(existing) = self.installed_home()?
            && !same_path(&existing, &self.setup.home)
        {
            return Err(ServiceError::OtherHome { existing });
        }
        let definition = self.definition();
        let steps = self.install_steps(options.start_now);
        if options.dry_run {
            return Ok(plan_report(Some(definition), &steps));
        }
        if let Some(dir) = definition.path.parent() {
            std::fs::create_dir_all(dir).map_err(io_error)?;
        }
        if self.setup.os != Os::Linux {
            // launchd · 예약 작업은 기록 폴더를 만들지 않는다. 보관 폴더 안에 미리 둔다.
            std::fs::create_dir_all(self.setup.home.join(SERVER_LOGS_DIR)).map_err(io_error)?;
        }
        std::fs::write(&definition.path, &definition.contents).map_err(io_error)?;
        let mut report = self.carry_out(&steps)?;
        report.definition = Some(definition);
        Ok(report)
    }

    /// 멈춘 뒤 정의를 지운다. 보관 폴더는 남긴다(Hermes "Keep data").
    pub fn uninstall(&self, dry_run: bool) -> Result<ServiceReport, ServiceError> {
        if self.installed_home()?.is_none() {
            return Ok(ServiceReport {
                definition: None,
                commands: Vec::new(),
                notes: vec!["등록된 서비스 정의가 없습니다".to_string()],
                dry_run,
            });
        }
        let mut steps = vec![Step::run(self.stop_command(), OnFailure::Ignore)];
        match self.setup.os {
            Os::MacOs => steps.push(Step::RemoveDefinition),
            Os::Linux => {
                steps.push(Step::run(
                    self.systemctl(&["disable", SYSTEMD_UNIT]),
                    OnFailure::Fail,
                ));
                steps.push(Step::RemoveDefinition);
                steps.push(Step::run(
                    self.systemctl(&["daemon-reload"]),
                    OnFailure::Ignore,
                ));
            }
            Os::Windows => {
                steps.push(Step::run(
                    line("schtasks", &["/Delete", "/TN", WINDOWS_TASK, "/F"]),
                    OnFailure::Fail,
                ));
                steps.push(Step::RemoveDefinition);
            }
        }
        let mut report = if dry_run {
            plan_report(None, &steps)
        } else {
            self.carry_out(&steps)?
        };
        report.notes.push(format!(
            "보관 폴더는 남겨 둡니다: {}",
            self.setup.home.display()
        ));
        Ok(report)
    }

    /// 서비스 관리자를 통해 띄운다.
    pub fn start(&self) -> Result<ServiceReport, ServiceError> {
        let steps = match self.setup.os {
            // 멈춤(bootout)이 정의를 내렸으므로 다시 올린 뒤 띄운다. 이미 올라가 있으면 올리기는 실패해도 된다.
            Os::MacOs => vec![
                Step::run(self.bootstrap(), OnFailure::Ignore),
                Step::run(
                    line("launchctl", &["kickstart", &self.target()]),
                    OnFailure::Fail,
                ),
            ],
            Os::Linux => vec![Step::run(
                self.systemctl(&["start", SYSTEMD_UNIT]),
                OnFailure::Fail,
            )],
            Os::Windows => vec![Step::run(
                line("schtasks", &["/Run", "/TN", WINDOWS_TASK]),
                OnFailure::Fail,
            )],
        };
        self.carry_out(&steps)
    }

    /// 서비스 관리자를 통해 멈춘다(macOS는 `launchctl bootout`으로 정의를 내린다).
    pub fn stop(&self) -> Result<ServiceReport, ServiceError> {
        self.carry_out(&[Step::run(self.stop_command(), OnFailure::Fail)])
    }

    /// 서비스 관리자를 통해 다시 띄운다.
    pub fn restart(&self) -> Result<ServiceReport, ServiceError> {
        let steps = match self.setup.os {
            Os::MacOs => vec![
                Step::run(self.bootstrap(), OnFailure::Ignore),
                Step::run(
                    line("launchctl", &["kickstart", "-k", &self.target()]),
                    OnFailure::Fail,
                ),
            ],
            Os::Linux => vec![Step::run(
                self.systemctl(&["restart", SYSTEMD_UNIT]),
                OnFailure::Fail,
            )],
            Os::Windows => vec![
                Step::run(
                    line("schtasks", &["/End", "/TN", WINDOWS_TASK]),
                    OnFailure::Ignore,
                ),
                Step::run(
                    line("schtasks", &["/Run", "/TN", WINDOWS_TASK]),
                    OnFailure::Fail,
                ),
            ],
        };
        self.carry_out(&steps)
    }

    /// 등록 여부 · 도는지 · 정의가 가리키는 홈.
    pub fn status(&self) -> Result<ServiceStatus, ServiceError> {
        let home = self.installed_home()?;
        if home.is_none() {
            return Ok(ServiceStatus {
                installed: false,
                running: false,
                home: None,
            });
        }
        let (command, running_word): (CommandLine, Option<&[&str]>) = match self.setup.os {
            Os::MacOs => (
                line("launchctl", &["print", &self.target()]),
                Some(&["state = running"]),
            ),
            Os::Linux => (self.systemctl(&["is-active", SYSTEMD_UNIT]), None),
            Os::Windows => (
                line("schtasks", &["/Query", "/TN", WINDOWS_TASK, "/FO", "LIST"]),
                Some(&["Running", "실행 중"]),
            ),
        };
        let running = match self.runner.run(&command) {
            Ok(output) => {
                output.success
                    && running_word.is_none_or(|words| {
                        output.stdout.is_empty()
                            || words.iter().any(|word| output.stdout.contains(word))
                    })
            }
            Err(_) => false,
        };
        Ok(ServiceStatus {
            installed: true,
            running,
            home,
        })
    }

    /// 정의 파일 경로.
    fn definition_path(&self) -> PathBuf {
        let name = match self.setup.os {
            Os::MacOs => LAUNCHD_FILE,
            Os::Linux => SYSTEMD_UNIT,
            Os::Windows => WINDOWS_TASK_FILE,
        };
        self.setup.definition_dir.join(name)
    }

    /// 정의가 띄우는 인자: `run --home <home> [지금 준 인자들]`.
    fn program_args(&self) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--home".to_string(),
            self.setup.home.to_string_lossy().into_owned(),
        ];
        args.extend(self.setup.run_args.iter().cloned());
        args
    }

    fn home_text(&self) -> String {
        self.setup.home.to_string_lossy().into_owned()
    }

    fn log_path(&self, name: &str) -> String {
        self.setup
            .home
            .join(SERVER_LOGS_DIR)
            .join(name)
            .to_string_lossy()
            .into_owned()
    }
}

fn line(program: &str, args: &[&str]) -> CommandLine {
    CommandLine {
        program: program.to_string(),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
    }
}

/// 두 경로가 같은 곳인가(가능하면 실제 경로로 견준다).
fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
