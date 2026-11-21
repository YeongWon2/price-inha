//! 하위 명령 읽기. 원본 동작: Hermes 2.2(`gateway run` = 인자 없는 실행, `--version`은 설정을 읽기 전에 답함).

use std::path::PathBuf;

use crate::error::CliError;

/// 이 실행 파일의 판 번호. `--version`이 보관 폴더 · 설정을 읽지 않고 답한다.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 읽은 하위 명령.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// 포그라운드로 띄운다(지금 동작). 인자 없는 `price-inha`와 `price-inha run`이 같다.
    /// `args`는 [`crate::Settings::parse`]에 그대로 넘기는 나머지 인자다.
    Run {
        /// 설정 인자(`--home` · `--bind` · `--port` 등).
        args: Vec<String>,
    },
    /// 사용자 서비스 정의를 다룬다.
    Service {
        /// 할 일.
        action: ServiceAction,
        /// 설정 인자. `install`이면 정의가 띄울 `run`에 함께 실린다(`--home`은 절대 경로로).
        args: Vec<String>,
    },
    /// 진단한다. 문제가 남으면 종료 코드 1.
    Doctor {
        /// 사람이 읽는 줄 대신 JSON으로 낸다.
        json: bool,
        /// 설정 인자.
        args: Vec<String>,
    },
    /// 접속 키를 만들어 터미널에 한 번만 보여 준다.
    AccessKey {
        /// 이미 있는 키를 바꾼다(없으면 이미 있을 때 거부).
        rotate: bool,
        /// 설정 인자.
        args: Vec<String>,
    },
    /// 백업 파일을 빈 폴더에 되살린다.
    Restore {
        /// 암호화된 백업 파일.
        backup: PathBuf,
        /// age 비밀키 파일(`--identity`).
        identity: PathBuf,
        /// 되살릴 빈 폴더(`--into`).
        into: PathBuf,
    },
    /// 판 번호를 보여 준다.
    Version,
}

/// `service`의 할 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    /// 정의를 쓰고 등록한다.
    Install {
        /// 등록하고 바로 띄운다.
        start_now: bool,
        /// 쓰지도 부르지도 않고 할 일만 보여 준다.
        dry_run: bool,
    },
    /// 멈추고 정의를 지운다. 보관 폴더는 남긴다.
    Uninstall {
        /// 쓰지도 부르지도 않고 할 일만 보여 준다.
        dry_run: bool,
    },
    /// 서비스 관리자를 통해 띄운다.
    Start,
    /// 서비스 관리자를 통해 멈춘다.
    Stop,
    /// 서비스 관리자를 통해 다시 띄운다.
    Restart,
    /// 등록 여부 · 도는지 · 정의가 가리키는 홈.
    Status,
}

impl Command {
    /// 프로그램 이름을 뺀 인자에서 하위 명령을 읽는다.
    ///
    /// 첫 인자가 하위 명령 이름이 아니면(없거나 `--`로 시작하면) [`Command::Run`]이다(지금 동작 그대로).
    /// `--version`은 무엇보다 먼저 본다. 모르는 하위 명령은 [`CliError::UnknownCommand`].
    pub fn parse(args: &[String]) -> Result<Command, CliError> {
        let _ = args;
        todo!()
    }
}
