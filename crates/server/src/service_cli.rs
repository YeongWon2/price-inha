//! `price-inha service …` 하위 명령: 서비스 정의를 등록 · 지우기 · 띄우기 · 멈추기 · 상태 보기(`main.rs`에서 옮김 —
//! 동작은 그대로다).

use std::process::ExitCode;

use price_inha::{
    CliError, InstallOptions, Service, ServiceAction, ServiceReport, ServiceSetup, Settings,
    SystemRunner, service_env_notes,
};

use crate::{failed, show};

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

pub(crate) fn service_command(
    action: ServiceAction,
    settings: &Settings,
    args: &[String],
) -> ExitCode {
    let setup = match ServiceSetup::for_this_computer(&settings.home, without_home(args)) {
        Ok(setup) => setup,
        Err(error) => return failed(&CliError::from(error)),
    };
    let runner = SystemRunner;
    let service = Service::new(setup, &runner);
    let result = match action {
        ServiceAction::Install { start_now, dry_run } => service
            .install(InstallOptions { start_now, dry_run })
            .map(|mut report| {
                // (65차) 환경 변수로만 준 `--bind` · `--port`는 정의에 실리지 않는다는 안내.
                let lookup = |name: &str| std::env::var(name).ok();
                report.notes.extend(service_env_notes(&lookup, &without_home(args)));
                show_report(&report, "서비스를 등록했습니다.")
            }),
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
            let dying = status
                .keeps_dying()
                .then(|| status.last_exit_code.unwrap_or_default());
            let home = status
                .home
                .map_or_else(|| "없음".to_string(), |home| home.display().to_string());
            show(&format!(
                "등록: {}\n도는 중: {}\n가리키는 보관 폴더: {home}\n",
                if status.installed { "예" } else { "아니요" },
                if status.running { "예" } else { "아니요" },
            ));
            // (65차) macOS에서 계속 죽는 서비스를 알린다.
            if let Some(code) = dying {
                show(&format!(
                    "안내: 서비스가 계속 죽습니다(마지막 종료 코드 {code}). price-inha doctor 로 기록 파일 자리를 봅니다\n"
                ));
            }
        }),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => failed(&CliError::from(error)),
    }
}
