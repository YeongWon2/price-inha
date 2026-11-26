//! 서비스 안내(지시서 9차 보강 4 "다음 차수 후보"):
//! - 환경 변수로 준 `--bind` · `--port`는 서비스 정의에 실리지 않는다 — 등록 때 그 사실과 고치기를 한 줄씩 알린다.
//!   인자로 함께 줬으면(정의에 실린다) 알리지 않는다.
//! - macOS에서 계속 죽는 서비스: `launchctl print`가 "도는 중이 아님 + 마지막 종료 코드가 0이 아님"이면
//!   `status`가 그것을 알고, `doctor`의 서비스 줄이 ✗와 기록 파일 자리를 알린다.

use price_inha::{
    CheckItem, CommandOutput, ENV_BIND, ENV_PORT, Os, SERVER_ERROR_LOG, Service, doctor,
    service_env_notes,
};

use crate::common::{ScriptedRunner, plain_settings, setup};

fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_string())
    }
}

// 검증: D1
#[test]
fn 환경_변수로_준_주소와_포트는_정의에_안_실린다고_알린다() {
    let lookup = env_of(&[(ENV_BIND, "0.0.0.0"), (ENV_PORT, "9100")]);
    let notes = service_env_notes(&lookup, &[]);
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(
        notes[0].contains(ENV_BIND) && notes[0].contains("--bind 0.0.0.0"),
        "{notes:?}"
    );
    assert!(
        notes[1].contains(ENV_PORT) && notes[1].contains("--port 9100"),
        "{notes:?}"
    );
    assert!(
        notes.iter().all(|note| note.contains("실리지 않")),
        "{notes:?}"
    );

    let given = vec!["--port".to_string(), "9100".to_string()];
    let notes = service_env_notes(&lookup, &given);
    assert_eq!(notes.len(), 1, "인자로 준 포트는 알리지 않는다: {notes:?}");
    assert!(notes[0].contains(ENV_BIND));

    assert!(service_env_notes(&env_of(&[]), &[]).is_empty());
}

fn launchctl_print(state: &str, last_exit: &str) -> CommandOutput {
    CommandOutput {
        success: true,
        stdout: format!(
            "gui/501/kr.priceinha.server = {{\n\tactive count = 0\n\tstate = {state}\n\truns = 14\n\tlast exit code = {last_exit}\n}}\n"
        ),
        stderr: String::new(),
    }
}

fn mac_service_with(
    home: &std::path::Path,
    defs: &std::path::Path,
    output: CommandOutput,
) -> ScriptedRunner {
    let runner = ScriptedRunner::answering(move |command| {
        if command.program == "launchctl" && command.args.first().is_some_and(|a| a == "print") {
            output.clone()
        } else {
            CommandOutput {
                success: true,
                ..CommandOutput::default()
            }
        }
    });
    // 정의 파일을 미리 쓴다(같은 홈).
    let writer = ScriptedRunner::ok();
    Service::new(setup(Os::MacOs, home, defs, &[]), &writer)
        .install(price_inha::InstallOptions {
            start_now: false,
            dry_run: false,
        })
        .unwrap();
    runner
}

// 검증: D1
#[test]
fn macos에서_계속_죽는_서비스를_상태와_진단이_알린다() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = mac_service_with(
        home.path(),
        defs.path(),
        launchctl_print("spawn scheduled", "78: EX_CONFIG"),
    );
    let service = Service::new(setup(Os::MacOs, home.path(), defs.path(), &[]), &runner);
    let status = service.status().unwrap();
    assert!(status.installed);
    assert!(!status.running);
    assert_eq!(status.last_exit_code, Some(78));
    assert!(status.keeps_dying());

    let report = doctor(&plain_settings(home.path()), &service);
    let line = report.check(CheckItem::Service).unwrap();
    assert!(!line.ok, "계속 죽으면 ✗: {line:?}");
    assert!(line.what.contains("78"), "{line:?}");
    assert!(
        line.fix
            .as_deref()
            .is_some_and(|fix| fix.contains(SERVER_ERROR_LOG)),
        "기록 파일 자리를 알린다: {line:?}"
    );
}

// 검증: D1
#[test]
fn 잘_도는_서비스와_한_번도_끝나지_않은_서비스는_죽는다고_하지_않는다() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    for (state, last) in [
        ("running", "1"),
        ("not running", "(never exited)"),
        ("not running", "0"),
    ] {
        let runner = mac_service_with(home.path(), defs.path(), launchctl_print(state, last));
        let service = Service::new(setup(Os::MacOs, home.path(), defs.path(), &[]), &runner);
        let status = service.status().unwrap();
        assert!(!status.keeps_dying(), "{state} · {last}");
        let report = doctor(&plain_settings(home.path()), &service);
        assert!(
            report.check(CheckItem::Service).unwrap().ok,
            "{state} · {last}"
        );
    }
    let linux = ScriptedRunner::ok();
    let service = Service::new(setup(Os::Linux, home.path(), defs.path(), &[]), &linux);
    assert_eq!(service.status().unwrap().last_exit_code, None);
}
