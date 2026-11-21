//! 사용자 서비스 정의(macOS launchd 사용자 에이전트 · Linux systemd 사용자 unit · Windows 예약 작업).
//!
//! 원본 동작: `docs/research/install-and-cli.md` 2.4(Hermes) · 3.4(OpenClaw). 칸은 지시서 "서비스 정의" 표.
//! 바깥 명령은 [`CommandRunner`] 뒤에서 부르고, 정의를 쓰는 폴더는 [`ServiceSetup::definition_dir`]로 받는다.

use std::path::{Path, PathBuf};

use crate::cli::runner::{CommandLine, CommandRunner};
use crate::error::ServiceError;

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
        todo!()
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
        let _ = (home, run_args);
        todo!()
    }
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
        let _ = self.runner;
        todo!()
    }

    /// 이미 있는 정의가 가리키는 보관 폴더. 정의가 없으면 없음.
    pub fn installed_home(&self) -> Result<Option<PathBuf>, ServiceError> {
        todo!()
    }

    /// 정의를 쓰고 등록한다. 다른 보관 폴더를 가리키는 기존 정의가 있으면 [`ServiceError::OtherHome`]이고
    /// 아무것도 바꾸지 않는다. 같은 홈이면 새로 쓴다. `dry_run`이면 쓰지도 부르지도 않는다.
    pub fn install(&self, options: InstallOptions) -> Result<ServiceReport, ServiceError> {
        let _ = options;
        todo!()
    }

    /// 멈춘 뒤 정의를 지운다. 보관 폴더는 남긴다(Hermes "Keep data").
    pub fn uninstall(&self, dry_run: bool) -> Result<ServiceReport, ServiceError> {
        let _ = dry_run;
        todo!()
    }

    /// 서비스 관리자를 통해 띄운다.
    pub fn start(&self) -> Result<ServiceReport, ServiceError> {
        todo!()
    }

    /// 서비스 관리자를 통해 멈춘다(macOS는 `launchctl bootout`으로 정의를 내린다).
    pub fn stop(&self) -> Result<ServiceReport, ServiceError> {
        todo!()
    }

    /// 서비스 관리자를 통해 다시 띄운다.
    pub fn restart(&self) -> Result<ServiceReport, ServiceError> {
        todo!()
    }

    /// 등록 여부 · 도는지 · 정의가 가리키는 홈.
    pub fn status(&self) -> Result<ServiceStatus, ServiceError> {
        todo!()
    }
}
