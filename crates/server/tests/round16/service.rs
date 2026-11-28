//! 서비스 정의(지시서 "서비스 정의" 표)와 부르는 명령 순서. 실제 서비스 관리자를 부르지 않는다.

use std::path::Path;

use price_inha::{
    CommandLine, InstallOptions, LAUNCHD_FILE, LAUNCHD_LABEL, Os, SERVER_ERROR_LOG, SERVER_LOG,
    SERVER_LOGS_DIR, SYSTEMD_UNIT, Service, ServiceError, WINDOWS_TASK, WINDOWS_TASK_FILE,
};

use crate::common::{
    AI_VALUE, BROKER_ID, BROKER_SECRET, EXE, FakeRunner, PATH_ENV, SLOT_VALUE, UID, files_under,
    healthy_home, line, setup, squash,
};

const DRY: InstallOptions = InstallOptions {
    start_now: false,
    dry_run: true,
};
const REAL: InstallOptions = InstallOptions {
    start_now: false,
    dry_run: false,
};

fn domain() -> String {
    format!("gui/{UID}")
}

fn target() -> String {
    format!("gui/{UID}/{LAUNCHD_LABEL}")
}

fn s(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

/// 정의 파일 경로가 운영체제마다 정의 폴더 아래 정해진 이름이다.
fn definition_file(os: Os, definitions: &Path) -> std::path::PathBuf {
    match os {
        Os::MacOs => definitions.join(LAUNCHD_FILE),
        Os::Linux => definitions.join(SYSTEMD_UNIT),
        Os::Windows => definitions.join(WINDOWS_TASK_FILE),
    }
}

/// systemd unit의 `이름=값` 줄들(따옴표는 떼고 본다).
fn unit_lines(unit: &str) -> Vec<String> {
    unit.lines()
        .map(|l| l.trim().replace('"', ""))
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// `program`을 부른 명령의 자리.
fn position(commands: &[CommandLine], wanted: &CommandLine) -> usize {
    commands
        .iter()
        .position(|c| c == wanted)
        .unwrap_or_else(|| panic!("{wanted:?}를 부르지 않았다: {commands:?}"))
}

// 검증: D1 D2
#[test]
fn macos_정의의_칸과_dry_run의_명령_순서() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = FakeRunner::ok();
    let service = Service::new(setup(Os::MacOs, home.path(), defs.path()), &runner);
    let report = service.install(DRY).unwrap();
    assert!(report.dry_run);
    let definition = report.definition.clone().expect("할 정의를 보여 준다");
    assert_eq!(definition.path, defs.path().join(LAUNCHD_FILE));
    assert_eq!(definition, service.definition());
    let plist = squash(&definition.contents);
    let h = s(home.path());
    for wanted in [
        format!("<key>Label</key><string>{LAUNCHD_LABEL}</string>"),
        format!(
            "<key>ProgramArguments</key><array><string>{EXE}</string><string>run</string>\
             <string>--home</string><string>{h}</string><string>--port</string>\
             <string>8123</string><string>--log-file</string><string>{}</string></array>",
            s(&home.path().join(SERVER_LOGS_DIR).join(SERVER_LOG))
        ),
        "<key>RunAtLoad</key><true/>".to_string(),
        "<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>".to_string(),
        "<key>ThrottleInterval</key><integer>30</integer>".to_string(),
        format!("<key>WorkingDirectory</key><string>{h}</string>"),
        format!(
            "<key>EnvironmentVariables</key><dict><key>PATH</key><string>{PATH_ENV}</string></dict>"
        ),
        "<key>StandardOutPath</key><string>/dev/null</string>".to_string(),
        format!(
            "<key>StandardErrorPath</key><string>{}</string>",
            s(&home.path().join(SERVER_LOGS_DIR).join(SERVER_ERROR_LOG))
        ),
    ] {
        assert!(plist.contains(&wanted), "{wanted} 없음:\n{plist}");
    }
    let bootout = line("launchctl", &["bootout", &target()]);
    let bootstrap = line(
        "launchctl",
        &["bootstrap", &domain(), &s(&defs.path().join(LAUNCHD_FILE))],
    );
    assert!(
        position(&report.commands, &bootout) < position(&report.commands, &bootstrap),
        "옛 정의를 내린 뒤 올린다(OpenClaw bootout → bootstrap): {:?}",
        report.commands
    );
    assert!(runner.calls().is_empty(), "dry-run은 부르지 않는다");
    assert!(files_under(defs.path()).is_empty(), "dry-run은 쓰지 않는다");
}

// 검증: D1 D2
#[test]
fn linux_정의의_칸과_dry_run의_명령_순서() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = FakeRunner::ok();
    let service = Service::new(setup(Os::Linux, home.path(), defs.path()), &runner);
    let report = service.install(DRY).unwrap();
    let definition = report.definition.clone().expect("할 정의를 보여 준다");
    assert_eq!(definition.path, defs.path().join(SYSTEMD_UNIT));
    let lines = unit_lines(&definition.contents);
    let h = s(home.path());
    for wanted in [
        "[Unit]".to_string(),
        "[Service]".to_string(),
        "[Install]".to_string(),
        format!("ExecStart={EXE} run --home {h} --port 8123"),
        format!("WorkingDirectory={h}"),
        format!("Environment=PATH={PATH_ENV}"),
        "Restart=on-failure".to_string(),
        "RestartSec=5".to_string(),
        format!("RestartPreventExitStatus={}", price_inha::CONFIG_FATAL_EXIT),
        "WantedBy=default.target".to_string(),
    ] {
        assert!(
            lines.contains(&wanted),
            "{wanted} 없음:\n{}",
            definition.contents
        );
    }
    let environment: Vec<&String> = lines
        .iter()
        .filter(|l| l.starts_with("Environment"))
        .collect();
    assert_eq!(environment.len(), 1, "환경은 PATH만: {environment:?}");
    let reload = line("systemctl", &["--user", "daemon-reload"]);
    let enable = line("systemctl", &["--user", "enable", SYSTEMD_UNIT]);
    let linger = line("loginctl", &["enable-linger"]);
    assert!(position(&report.commands, &reload) < position(&report.commands, &enable));
    position(&report.commands, &linger);
    assert!(
        !report
            .commands
            .iter()
            .any(|c| c.args.iter().any(|a| a == "start" || a == "restart")),
        "--start-now 없이는 띄우지 않는다: {:?}",
        report.commands
    );
    assert!(runner.calls().is_empty(), "dry-run은 부르지 않는다");
    assert!(files_under(defs.path()).is_empty(), "dry-run은 쓰지 않는다");
}

// 검증: D1 D2
#[test]
fn windows_정의의_칸과_dry_run의_명령() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = FakeRunner::ok();
    let service = Service::new(setup(Os::Windows, home.path(), defs.path()), &runner);
    let report = service.install(DRY).unwrap();
    let definition = report.definition.clone().expect("할 정의를 보여 준다");
    assert_eq!(definition.path, defs.path().join(WINDOWS_TASK_FILE));
    let xml = squash(&definition.contents);
    let h = s(home.path());
    assert!(xml.contains("<LogonTrigger>"), "로그온 트리거:\n{xml}");
    let logon = &xml[xml.find("<LogonTrigger>").unwrap()..];
    let logon = &logon[..logon.find("</LogonTrigger>").unwrap()];
    assert!(logon.contains("<Delay>PT30S</Delay>"), "30초 지연: {logon}");
    assert!(
        xml.contains("<RunLevel>LeastPrivilege</RunLevel>"),
        "최소 권한:\n{xml}"
    );
    let retry = &xml[xml.find("<RestartOnFailure>").expect("실패 시 재시도")..];
    let retry = &retry[..retry.find("</RestartOnFailure>").unwrap()];
    assert!(
        retry.contains("<Interval>PT1M</Interval>"),
        "1분 간격: {retry}"
    );
    assert!(xml.contains(&format!("<Command>{EXE}</Command>")), "{xml}");
    let arguments = &xml[xml.find("<Arguments>").expect("인자")..];
    let arguments = arguments[..arguments.find("</Arguments>").unwrap()].replace('"', "");
    assert!(
        arguments.contains(&format!("run --home {h} --port 8123")),
        "{arguments}"
    );
    assert!(
        xml.contains(&format!("<WorkingDirectory>{h}</WorkingDirectory>")),
        "작업 폴더는 보관 폴더:\n{xml}"
    );
    let create = report
        .commands
        .iter()
        .find(|c| c.program == "schtasks" && c.args.iter().any(|a| a == "/Create"))
        .expect("schtasks /Create");
    let tn = create.args.iter().position(|a| a == "/TN").expect("/TN");
    assert_eq!(create.args[tn + 1], WINDOWS_TASK);
    let xml_arg = create.args.iter().position(|a| a == "/XML").expect("/XML");
    assert_eq!(create.args[xml_arg + 1], s(&definition.path));
    assert!(runner.calls().is_empty(), "dry-run은 부르지 않는다");
    assert!(files_under(defs.path()).is_empty(), "dry-run은 쓰지 않는다");
}

// 검증: D1
#[test]
fn 등록하면_정의를_쓰고_보여_준_명령을_그_순서로_부른다() {
    for os in [Os::MacOs, Os::Linux] {
        let home = tempfile::tempdir().unwrap();
        let defs = tempfile::tempdir().unwrap();
        let runner = FakeRunner::ok();
        let service = Service::new(setup(os, home.path(), defs.path()), &runner);
        let planned = service.install(DRY).unwrap();
        let report = service.install(REAL).unwrap();
        assert!(!report.dry_run);
        assert_eq!(report.commands, planned.commands, "{os:?}: 보여 준 그대로");
        assert_eq!(runner.commands(), report.commands, "{os:?}: 부른 순서");
        let file = definition_file(os, defs.path());
        let written = std::fs::read_to_string(&file).unwrap();
        assert_eq!(written, service.definition().contents, "{os:?}");
        assert_eq!(
            service.installed_home().unwrap().as_deref(),
            Some(home.path()),
            "{os:?}: 정의가 이 홈을 가리킨다"
        );
    }
}

// 검증: D1
#[test]
fn start_now면_등록_뒤에_띄운다() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = FakeRunner::ok();
    let service = Service::new(setup(Os::Linux, home.path(), defs.path()), &runner);
    let report = service
        .install(InstallOptions {
            start_now: true,
            dry_run: true,
        })
        .unwrap();
    let enable = position(
        &report.commands,
        &line("systemctl", &["--user", "enable", SYSTEMD_UNIT]),
    );
    let started = report
        .commands
        .iter()
        .position(|c| {
            c.program == "systemctl"
                && c.args.len() == 3
                && c.args[0] == "--user"
                && (c.args[1] == "start" || c.args[1] == "restart")
                && c.args[2] == SYSTEMD_UNIT
        })
        .expect("띄운다");
    assert!(enable < started, "{:?}", report.commands);

    let win_defs = tempfile::tempdir().unwrap();
    let windows = Service::new(setup(Os::Windows, home.path(), win_defs.path()), &runner);
    let report = windows
        .install(InstallOptions {
            start_now: true,
            dry_run: true,
        })
        .unwrap();
    let created = report
        .commands
        .iter()
        .position(|c| c.args.iter().any(|a| a == "/Create"))
        .expect("/Create");
    let run = position(
        &report.commands,
        &line("schtasks", &["/Run", "/TN", WINDOWS_TASK]),
    );
    assert!(created < run, "{:?}", report.commands);
}

// 검증: D1
#[test]
fn linger를_켜지_못하면_수동_명령을_안내하고_등록은_한다() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = FakeRunner::failing(|c| c.program == "loginctl");
    let service = Service::new(setup(Os::Linux, home.path(), defs.path()), &runner);
    let report = service
        .install(REAL)
        .expect("linger 실패로 등록을 막지 않는다");
    assert!(defs.path().join(SYSTEMD_UNIT).exists());
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("loginctl enable-linger")),
        "수동 명령 안내: {:?}",
        report.notes
    );
}

// 검증: D1
#[test]
fn 다른_홈을_가리키는_정의가_있으면_덮어쓰지_않는다() {
    for os in [Os::MacOs, Os::Linux, Os::Windows] {
        let other = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let defs = tempfile::tempdir().unwrap();
        let runner = FakeRunner::ok();
        let existing = Service::new(setup(os, other.path(), defs.path()), &runner).definition();
        std::fs::write(&existing.path, &existing.contents).unwrap();

        let service = Service::new(setup(os, home.path(), defs.path()), &runner);
        let refused = service.install(REAL);
        assert_eq!(
            refused,
            Err(ServiceError::OtherHome {
                existing: other.path().to_path_buf()
            }),
            "{os:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&existing.path).unwrap(),
            existing.contents,
            "{os:?}: 그대로 둔다"
        );
        assert!(runner.calls().is_empty(), "{os:?}: 아무것도 부르지 않는다");
        assert!(
            service.install(DRY).is_err(),
            "{os:?}: dry-run도 거부를 알린다"
        );
    }
}

// 검증: D1
#[test]
fn 같은_홈이면_정의를_새로_쓴다() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = FakeRunner::ok();
    let mut old = setup(Os::Linux, home.path(), defs.path());
    old.exe = "/opt/old/price-inha".into();
    let stale = Service::new(old, &runner).definition();
    std::fs::write(&stale.path, &stale.contents).unwrap();

    let service = Service::new(setup(Os::Linux, home.path(), defs.path()), &runner);
    service.install(REAL).expect("같은 홈은 새로 쓴다");
    let written = std::fs::read_to_string(defs.path().join(SYSTEMD_UNIT)).unwrap();
    assert_eq!(written, service.definition().contents, "낡은 경로를 피한다");
}

// 검증: D1
#[test]
fn 제거는_멈춘_뒤_정의를_지우고_보관_폴더는_남긴다() {
    for os in [Os::MacOs, Os::Linux, Os::Windows] {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("keep.txt"), b"data").unwrap();
        let defs = tempfile::tempdir().unwrap();
        let file = definition_file(os, defs.path());
        let installer = FakeRunner::ok();
        let definition = Service::new(setup(os, home.path(), defs.path()), &installer).definition();
        std::fs::write(&definition.path, &definition.contents).unwrap();

        let runner = FakeRunner::ok().watching(&file);
        let service = Service::new(setup(os, home.path(), defs.path()), &runner);
        let planned = service.uninstall(true).unwrap();
        assert!(file.exists(), "{os:?}: dry-run은 지우지 않는다");
        assert!(runner.calls().is_empty(), "{os:?}: dry-run은 부르지 않는다");

        let report = service.uninstall(false).unwrap();
        assert_eq!(report.commands, planned.commands, "{os:?}");
        let stop = match os {
            Os::MacOs => line("launchctl", &["bootout", &target()]),
            Os::Linux => line("systemctl", &["--user", "stop", SYSTEMD_UNIT]),
            Os::Windows => line("schtasks", &["/End", "/TN", WINDOWS_TASK]),
        };
        let calls = runner.calls();
        let first = calls.first().expect("멈춘다");
        assert_eq!(first.command, stop, "{os:?}: 먼저 멈춘다");
        assert!(
            first.definition_existed,
            "{os:?}: 멈출 때는 정의가 아직 있다"
        );
        assert!(!file.exists(), "{os:?}: 정의를 지운다");
        assert!(
            home.path().join("keep.txt").exists(),
            "{os:?}: 보관 폴더는 남긴다"
        );
        assert_eq!(service.installed_home().unwrap(), None, "{os:?}");
    }
}

// 검증: D1
#[test]
fn 멈춤은_서비스_관리자를_통한다() {
    for (os, stop) in [
        (Os::MacOs, line("launchctl", &["bootout", &target()])),
        (
            Os::Linux,
            line("systemctl", &["--user", "stop", SYSTEMD_UNIT]),
        ),
        (
            Os::Windows,
            line("schtasks", &["/End", "/TN", WINDOWS_TASK]),
        ),
    ] {
        let home = tempfile::tempdir().unwrap();
        let defs = tempfile::tempdir().unwrap();
        let runner = FakeRunner::ok();
        let service = Service::new(setup(os, home.path(), defs.path()), &runner);
        let report = service.stop().unwrap();
        assert_eq!(runner.commands(), vec![stop.clone()], "{os:?}");
        assert_eq!(report.commands, vec![stop], "{os:?}");
    }
}

// 검증: D1
#[test]
fn 상태는_등록_여부와_도는지와_가리키는_홈() {
    let home = tempfile::tempdir().unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = FakeRunner::ok();
    let service = Service::new(setup(Os::Linux, home.path(), defs.path()), &runner);
    let before = service.status().unwrap();
    assert!(!before.installed);
    assert_eq!(before.home, None);

    service.install(REAL).unwrap();
    let up = service.status().unwrap();
    assert!(up.installed);
    assert!(up.running, "관리자가 성공으로 답하면 도는 중");
    assert_eq!(up.home.as_deref(), Some(home.path()));

    let down_runner = FakeRunner::failing(|c| c.program == "systemctl");
    let down = Service::new(setup(Os::Linux, home.path(), defs.path()), &down_runner)
        .status()
        .unwrap();
    assert!(down.installed);
    assert!(!down.running, "관리자가 실패로 답하면 멈춘 것");
}

// 검증: N8 D1
#[tokio::test]
async fn 정의에는_비밀_값을_싣지_않는다() {
    let home = tempfile::tempdir().unwrap();
    healthy_home(home.path()).await;
    let defs = tempfile::tempdir().unwrap();
    let runner = FakeRunner::ok();
    for os in [Os::MacOs, Os::Linux, Os::Windows] {
        let definition = Service::new(setup(os, home.path(), defs.path()), &runner).definition();
        for value in [BROKER_ID, BROKER_SECRET, AI_VALUE, SLOT_VALUE] {
            assert!(
                !definition.contents.contains(value),
                "{os:?}: 정의에 키 값이 있다"
            );
        }
    }
}
