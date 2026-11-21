//! 서비스 명령의 단계: 부를 명령 · 실패했을 때 할 일 · 정의 지우기.

use super::{
    CommandLine, LAUNCHD_LABEL, Os, SYSTEMD_UNIT, Service, ServiceDefinition, ServiceError,
    ServiceReport, WINDOWS_TASK, io_error, line,
};

impl Service<'_> {
    /// Linux면 linger가 켜졌는가(로그아웃 뒤에도 사용자 서비스가 도는가). 알 수 없으면 없음.
    pub(crate) fn linger(&self) -> Option<bool> {
        if self.setup.os != Os::Linux {
            return None;
        }
        let uid = self.setup.uid.to_string();
        let output = self
            .runner
            .run(&line("loginctl", &["show-user", &uid, "--property=Linger"]))
            .ok()?;
        if !output.success {
            return Some(false);
        }
        if output.stdout.trim().is_empty() {
            return None;
        }
        Some(output.stdout.contains("Linger=yes"))
    }

    /// macOS `gui/<uid>`.
    pub(super) fn domain(&self) -> String {
        format!("gui/{}", self.setup.uid)
    }

    /// macOS `gui/<uid>/<label>`.
    pub(super) fn target(&self) -> String {
        format!("gui/{}/{LAUNCHD_LABEL}", self.setup.uid)
    }

    pub(super) fn bootstrap(&self) -> CommandLine {
        let path = self.definition_path().to_string_lossy().into_owned();
        line("launchctl", &["bootstrap", &self.domain(), &path])
    }

    pub(super) fn systemctl(&self, args: &[&str]) -> CommandLine {
        let mut all = vec!["--user"];
        all.extend_from_slice(args);
        line("systemctl", &all)
    }

    pub(super) fn stop_command(&self) -> CommandLine {
        match self.setup.os {
            Os::MacOs => line("launchctl", &["bootout", &self.target()]),
            Os::Linux => self.systemctl(&["stop", SYSTEMD_UNIT]),
            Os::Windows => line("schtasks", &["/End", "/TN", WINDOWS_TASK]),
        }
    }

    /// 등록에서 부르는 명령들(정의를 쓴 뒤).
    pub(super) fn install_steps(&self, start_now: bool) -> Vec<Step> {
        let mut steps = Vec::new();
        match self.setup.os {
            Os::MacOs => {
                // 옛 정의가 올라가 있으면 내린 뒤 올린다(OpenClaw bootout → bootstrap). 없으면 내리기는 실패해도 된다.
                steps.push(Step::run(
                    line("launchctl", &["bootout", &self.target()]),
                    OnFailure::Ignore,
                ));
                steps.push(Step::run(self.bootstrap(), OnFailure::Fail));
                if start_now {
                    steps.push(Step::run(
                        line("launchctl", &["kickstart", &self.target()]),
                        OnFailure::Fail,
                    ));
                }
            }
            Os::Linux => {
                steps.push(Step::run(
                    self.systemctl(&["daemon-reload"]),
                    OnFailure::Fail,
                ));
                steps.push(Step::run(
                    self.systemctl(&["enable", SYSTEMD_UNIT]),
                    OnFailure::Fail,
                ));
                steps.push(Step::run(
                    line("loginctl", &["enable-linger"]),
                    OnFailure::Note(LINGER_NOTE),
                ));
                if start_now {
                    steps.push(Step::run(
                        self.systemctl(&["restart", SYSTEMD_UNIT]),
                        OnFailure::Fail,
                    ));
                }
            }
            Os::Windows => {
                let xml = self.definition_path().to_string_lossy().into_owned();
                steps.push(Step::run(
                    line(
                        "schtasks",
                        &["/Create", "/TN", WINDOWS_TASK, "/XML", &xml, "/F"],
                    ),
                    OnFailure::Fail,
                ));
                if start_now {
                    steps.push(Step::run(
                        line("schtasks", &["/Run", "/TN", WINDOWS_TASK]),
                        OnFailure::Fail,
                    ));
                }
            }
        }
        steps
    }

    /// 단계를 차례로 한다. 꼭 성공해야 하는 명령이 실패하면 거기서 멈추고 [`ServiceError::CommandFailed`].
    pub(super) fn carry_out(&self, steps: &[Step]) -> Result<ServiceReport, ServiceError> {
        let mut report = ServiceReport {
            definition: None,
            commands: Vec::new(),
            notes: Vec::new(),
            dry_run: false,
        };
        for step in steps {
            match step {
                Step::RemoveDefinition => match std::fs::remove_file(self.definition_path()) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(io_error(error)),
                },
                Step::Run(command, on_failure) => {
                    report.commands.push(command.clone());
                    let succeeded = self
                        .runner
                        .run(command)
                        .map(|output| output.success)
                        .unwrap_or(false);
                    if succeeded {
                        continue;
                    }
                    match on_failure {
                        OnFailure::Ignore => {}
                        OnFailure::Note(note) => report.notes.push((*note).to_string()),
                        OnFailure::Fail => {
                            return Err(ServiceError::CommandFailed {
                                program: command.program.clone(),
                            });
                        }
                    }
                }
            }
        }
        Ok(report)
    }
}

/// linger를 켜지 못했을 때 사람이 할 일.
const LINGER_NOTE: &str = "로그아웃 뒤에도 서비스가 돌게 하려면 직접 다음을 실행합니다: \
sudo loginctl enable-linger $USER";

/// 실패했을 때 어떻게 하나.
#[derive(Debug, Clone, Copy)]
pub(super) enum OnFailure {
    /// 실패해도 된다(예: 올라가 있지 않은 정의 내리기).
    Ignore,
    /// 실패하면 사람이 할 일을 남기고 계속한다.
    Note(&'static str),
    /// 실패하면 멈추고 오류.
    Fail,
}

/// 서비스 명령의 한 단계.
#[derive(Debug, Clone)]
pub(super) enum Step {
    /// 바깥 명령을 부른다.
    Run(CommandLine, OnFailure),
    /// 정의 파일을 지운다.
    RemoveDefinition,
}

impl Step {
    pub(super) fn run(command: CommandLine, on_failure: OnFailure) -> Step {
        Step::Run(command, on_failure)
    }
}

/// 부르지 않고 할 일만 담은 보고.
pub(super) fn plan_report(definition: Option<ServiceDefinition>, steps: &[Step]) -> ServiceReport {
    ServiceReport {
        definition,
        commands: steps
            .iter()
            .filter_map(|step| match step {
                Step::Run(command, _) => Some(command.clone()),
                Step::RemoveDefinition => None,
            })
            .collect(),
        notes: Vec::new(),
        dry_run: true,
    }
}
