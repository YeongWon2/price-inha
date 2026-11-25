//! (9차) 하위 명령: 홈서버 · 미니 PC · VPN에서 쓰는 `service` · `doctor` · `access-key` · `restore` · `--version`.
//! (37차) 처음 설정 마법사 `setup`과 대시보드 열기 `open`.
//!
//! 지시서 `docs/dev/modules/server.md` "9차 작업". 원본 동작 근거: `docs/research/install-and-cli.md`
//! 2.2~2.7(Hermes) · 3.2~3.7(OpenClaw). 인자 없이 띄우는 지금 동작은 [`Command::Run`]으로 그대로 간다.

mod access;
mod command;
mod doctor;
mod open;
mod peek;
mod restore;
mod runner;
mod service;
mod setup;

pub use access::{AccessKeyRequest, ShownAccessKey, access_key, access_key_with};
pub use command::{Command, ServiceAction, VERSION};
pub use doctor::{Check, CheckItem, DoctorReport, doctor};
pub use open::{OPEN_NO_SERVER, OPEN_NO_SERVER_EXIT, OPEN_SCREENS, OpenRequest, OpenScreen, open};
pub use restore::{RestoreRequest, RestoreSummary, restore};
pub use runner::{CommandLine, CommandOutput, CommandRunner, SystemRunner};
pub use service::{
    CONFIG_FATAL_EXIT, InstallOptions, LAUNCHD_FILE, LAUNCHD_LABEL, Os, SERVER_ERROR_LOG,
    SERVER_LOG, SERVER_LOGS_DIR, SYSTEMD_UNIT, Service, ServiceDefinition, ServiceReport,
    ServiceSetup, ServiceStatus, WINDOWS_TASK, WINDOWS_TASK_FILE,
};
pub use setup::{
    ANSWER_CLI, ANSWER_KEY, ANSWER_SKIP, SETUP_ALREADY, SETUP_CLI_BILLING_WARNING,
    SETUP_CLI_LOGIN_HINT, SETUP_DOCTOR_HINT, SETUP_DONE, SETUP_FAILED, SETUP_FAILED_EXIT,
    SETUP_MESSENGER_LATER, SETUP_NOT_TERMINAL, SETUP_NOT_TERMINAL_EXIT, SETUP_RUN_HINT,
    SETUP_SECTIONS, SETUP_SECTOR_NOTICE, SETUP_SKIPPED, SETUP_START_SERVER_QUESTION,
    SETUP_SUMMARY_TITLE, SetupContext, SetupRequest, SetupSection, setup,
};
