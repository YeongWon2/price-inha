//! 하위 명령 읽기. 원본 동작: Hermes 2.2(`gateway run` = 인자 없는 실행, `--version`은 설정을 읽기 전에 답함).

use std::path::PathBuf;

use super::open::OpenScreen;
use super::setup::SetupSection;
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
    /// (37차) 터미널 처음 설정 마법사: `setup [구역] [--stdin-answers] [--dry-run] [설정 인자들]`.
    /// 구역은 `setup` 바로 다음 인자(`-`로 시작하지 않을 때)다. 모르는 구역은 [`CliError::UnknownCommand`].
    Setup {
        /// 한 구역만. `None`이면 모두 차례로.
        section: Option<SetupSection>,
        /// 표준 입력에서 답을 흘려 받는다(숨은 길, 시험 · 자동 설치용).
        stdin_answers: bool,
        /// 서비스 구역을 할 일만 보이게 한다.
        dry_run: bool,
        /// 설정 인자.
        args: Vec<String>,
    },
    /// (37차) 도는 서버의 대시보드를 연다: `open [화면] [--print] [설정 인자들]`.
    /// 화면은 `open` 바로 다음 인자(`-`로 시작하지 않을 때)다. 모르는 화면은 [`CliError::UnknownCommand`].
    Open {
        /// 열 화면(주지 않으면 첫 화면).
        screen: OpenScreen,
        /// 브라우저를 열지 않고 주소만 쓴다.
        print: bool,
        /// 설정 인자.
        args: Vec<String>,
    },
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
        let Some((first, rest)) = args.split_first() else {
            return Ok(Command::Run { args: Vec::new() });
        };
        if first == ARG_VERSION {
            return Ok(Command::Version);
        }
        if first.starts_with('-') {
            // 하위 명령 없이 설정 인자만 준 지금 모양. 그 안의 `--version`도 무엇보다 먼저 답한다.
            if rest.iter().any(|arg| arg == ARG_VERSION) {
                return Ok(Command::Version);
            }
            return Ok(Command::Run {
                args: args.to_vec(),
            });
        }
        match first.as_str() {
            "run" => Ok(Command::Run {
                args: rest.to_vec(),
            }),
            "service" => parse_service(rest),
            "doctor" => {
                let (json, args) = take_flag(rest, "--json");
                Ok(Command::Doctor { json, args })
            }
            "access-key" => {
                let (rotate, args) = take_flag(rest, "--rotate");
                Ok(Command::AccessKey { rotate, args })
            }
            "restore" => parse_restore(rest),
            "setup" => parse_setup(rest),
            "open" => parse_open(rest),
            other => Err(CliError::UnknownCommand(other.to_string())),
        }
    }
}

/// 판 번호를 묻는 인자.
const ARG_VERSION: &str = "--version";

/// `flag`를 모두 빼고, 있었는지와 남은 인자를 돌려준다.
fn take_flag(args: &[String], flag: &str) -> (bool, Vec<String>) {
    let found = args.iter().any(|arg| arg == flag);
    let rest = args.iter().filter(|arg| *arg != flag).cloned().collect();
    (found, rest)
}

/// `service <할 일> [인자들]`.
fn parse_service(args: &[String]) -> Result<Command, CliError> {
    let Some((word, rest)) = args.split_first() else {
        return Err(CliError::MissingArgument {
            name: "service 할 일",
        });
    };
    let (action, args) = match word.as_str() {
        "install" => {
            let (start_now, rest) = take_flag(rest, "--start-now");
            let (dry_run, rest) = take_flag(&rest, "--dry-run");
            (ServiceAction::Install { start_now, dry_run }, rest)
        }
        "uninstall" => {
            let (dry_run, rest) = take_flag(rest, "--dry-run");
            (ServiceAction::Uninstall { dry_run }, rest)
        }
        "start" => (ServiceAction::Start, rest.to_vec()),
        "stop" => (ServiceAction::Stop, rest.to_vec()),
        "restart" => (ServiceAction::Restart, rest.to_vec()),
        "status" => (ServiceAction::Status, rest.to_vec()),
        other => return Err(CliError::UnknownCommand(format!("service {other}"))),
    };
    Ok(Command::Service { action, args })
}

/// 하위 명령 바로 다음의 이름 인자(`-`로 시작하지 않을 때)와 남은 인자.
fn take_word(args: &[String]) -> (Option<&str>, &[String]) {
    match args.split_first() {
        Some((word, rest)) if !word.starts_with('-') => (Some(word.as_str()), rest),
        _ => (None, args),
    }
}

/// `setup [구역] [--stdin-answers] [--dry-run] [설정 인자들]`.
fn parse_setup(args: &[String]) -> Result<Command, CliError> {
    let (word, rest) = take_word(args);
    let section = match word {
        Some(word) => Some(
            SetupSection::parse(word)
                .ok_or_else(|| CliError::UnknownCommand(format!("setup {word}")))?,
        ),
        None => None,
    };
    let (stdin_answers, rest) = take_flag(rest, "--stdin-answers");
    let (dry_run, args) = take_flag(&rest, "--dry-run");
    Ok(Command::Setup {
        section,
        stdin_answers,
        dry_run,
        args,
    })
}

/// `open [화면] [--print] [설정 인자들]`.
fn parse_open(args: &[String]) -> Result<Command, CliError> {
    let (word, rest) = take_word(args);
    let screen = match word {
        Some(word) => OpenScreen::parse(word)
            .ok_or_else(|| CliError::UnknownCommand(format!("open {word}")))?,
        None => OpenScreen::Home,
    };
    let (print, args) = take_flag(rest, "--print");
    Ok(Command::Open {
        screen,
        print,
        args,
    })
}

/// `restore <백업 파일> --identity <파일> --into <폴더>`. 순서는 상관없다.
fn parse_restore(args: &[String]) -> Result<Command, CliError> {
    let mut backup = None;
    let mut identity = None;
    let mut into = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--identity" => {
                let value = rest
                    .next()
                    .ok_or(CliError::MissingArgument { name: "--identity" })?;
                identity = Some(PathBuf::from(value));
            }
            "--into" => {
                let value = rest
                    .next()
                    .ok_or(CliError::MissingArgument { name: "--into" })?;
                into = Some(PathBuf::from(value));
            }
            flag if flag.starts_with('-') => return Err(CliError::UnknownArgument),
            file if backup.is_none() => backup = Some(PathBuf::from(file)),
            _ => return Err(CliError::UnknownArgument),
        }
    }
    Ok(Command::Restore {
        backup: backup.ok_or(CliError::MissingArgument {
            name: "<백업 파일>",
        })?,
        identity: identity.ok_or(CliError::MissingArgument { name: "--identity" })?,
        into: into.ok_or(CliError::MissingArgument { name: "--into" })?,
    })
}
