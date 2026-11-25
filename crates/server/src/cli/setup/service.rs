//! 서비스 구역: "계속 켜 둘까요?"(빈 줄 = 예). 예면 `service install`과 같은 일을 같은 손잡이
//! ([`Service::install`])로 한다 — `--dry-run`이면 `service install --dry-run`과 같은 할 일 글만 보이고 쓰지도
//! 부르지도 않는다. 아니오면 [`SETUP_RUN_HINT`].
//!
//! 마법사가 뒤에서 띄운 서버가 있는데 실제로 등록하면(드라이런이 아님), 그 서버를 먼저 끄고 서비스가 바로 띄우게
//! 한다(`--start-now`) — 같은 포트를 두 서버가 다투지 않고, 끝난 뒤에도 서버가 계속 돈다.

use super::SETUP_RUN_HINT;
use super::prompt::Prompter;
use super::server::Spawned;
use super::summary::Outcome;
use crate::Settings;
use crate::cli::runner::SystemRunner;
use crate::cli::service::{InstallOptions, Service, ServiceReport, ServiceSetup};

/// 서비스 구역을 돈다. 실제로 등록했으면 마법사가 띄운 서버(`spawned`)는 끄고 비운다.
pub(crate) async fn service(
    settings: &Settings,
    args: &[String],
    dry_run: bool,
    spawned: &mut Option<Spawned>,
    prompter: &mut Prompter<'_>,
) -> Outcome {
    prompter.title("계속 켜 두기");
    let Some(yes) = prompter.yes_no("컴퓨터를 켜면 서버가 저절로 뜨게 계속 켜 둘까요?", true)
    else {
        return Outcome::ended();
    };
    if !yes {
        prompter.say(SETUP_RUN_HINT);
        return Outcome::skipped("등록하지 않음");
    }
    let setup = match ServiceSetup::for_this_computer(&settings.home, without_home(args)) {
        Ok(setup) => setup,
        Err(error) => {
            prompter.say(&format!("서비스를 준비하지 못했습니다: {error}"));
            return Outcome::failed("준비하지 못함");
        }
    };
    let start_now = !dry_run && spawned.is_some();
    if start_now && let Some(server) = spawned.take() {
        prompter.say("마법사가 띄운 서버를 끄고 서비스로 다시 띄웁니다");
        server.stop().await;
    }
    let runner = SystemRunner;
    let installed = Service::new(setup, &runner).install(InstallOptions { start_now, dry_run });
    match installed {
        Ok(report) => {
            prompter.say(&render(&report));
            if dry_run {
                Outcome::done("할 일만 보임(--dry-run)")
            } else {
                Outcome::done("서비스로 등록함")
            }
        }
        Err(error) => {
            prompter.say(&format!("서비스를 등록하지 못했습니다: {error}"));
            Outcome::failed("등록하지 못함")
        }
    }
}

/// 서비스 정의가 띄울 `run` 인자: 준 설정 인자에서 `--home <폴더>`를 뺀 것(`service install`과 같다 —
/// 정의는 보관 폴더를 절대 경로로 따로 싣는다).
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

/// `service install`이 보이는 것과 같은 글(정의 파일 · 부를 명령 · 안내).
fn render(report: &ServiceReport) -> String {
    let mut text = String::new();
    if report.dry_run {
        text.push_str("할 일만 보여 줍니다(쓰지도 부르지도 않았습니다).\n");
    } else {
        text.push_str("서비스를 등록했습니다.\n");
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
    text.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 홈_인자만_뺀다() {
        let args: Vec<String> = ["--home", "/x", "--port", "9"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(without_home(&args), ["--port", "9"]);
    }
}
