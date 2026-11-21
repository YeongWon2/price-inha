//! 바깥 명령(`launchctl` · `systemctl` · `loginctl` · `schtasks`)을 부르는 자리.
//! 시험은 가짜 실행기로 부른 명령 줄만 본다(실제 서비스 관리자를 부르지 않는다).

/// 바깥 명령 한 줄. 비밀 값을 싣지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLine {
    /// 프로그램 이름(예: `launchctl`).
    pub program: String,
    /// 인자들.
    pub args: Vec<String>,
}

/// 바깥 명령의 결과.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandOutput {
    /// 종료 코드가 0이었나.
    pub success: bool,
    /// 표준 출력.
    pub stdout: String,
    /// 표준 오류.
    pub stderr: String,
}

/// 바깥 명령을 부른다.
pub trait CommandRunner: Send + Sync {
    /// `command`를 부르고 끝날 때까지 기다린다. 부르지 못했으면 입출력 오류.
    fn run(&self, command: &CommandLine) -> std::io::Result<CommandOutput>;
}

/// 운영체제의 프로세스로 부르는 실행기.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, command: &CommandLine) -> std::io::Result<CommandOutput> {
        let _ = command;
        todo!()
    }
}
