//! (9차) 하위 명령: 홈서버 · 미니 PC · VPN에서 쓰는 `service` · `doctor` · `access-key` · `restore` · `--version`.
//!
//! 지시서 `docs/dev/modules/server.md` "9차 작업". 원본 동작 근거: `docs/research/install-and-cli.md`
//! 2.2~2.7(Hermes) · 3.2~3.7(OpenClaw). 인자 없이 띄우는 지금 동작은 [`Command::Run`]으로 그대로 간다.

mod access;
mod command;
mod doctor;
mod restore;
mod runner;
mod service;

pub use access::{AccessKeyRequest, ShownAccessKey, access_key, access_key_with};
pub use command::{Command, ServiceAction, VERSION};
pub use doctor::{Check, CheckItem, DoctorReport, doctor};
pub use restore::{RestoreRequest, RestoreSummary, restore};
pub use runner::{CommandLine, CommandOutput, CommandRunner, SystemRunner};
pub use service::{
    CONFIG_FATAL_EXIT, InstallOptions, LAUNCHD_FILE, LAUNCHD_LABEL, Os, SERVER_ERROR_LOG,
    SERVER_LOG, SERVER_LOGS_DIR, SYSTEMD_UNIT, Service, ServiceDefinition, ServiceReport,
    ServiceSetup, ServiceStatus, WINDOWS_TASK, WINDOWS_TASK_FILE,
};
