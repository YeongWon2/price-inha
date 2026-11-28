//! macOS 서비스 기록 파일 돌려 쓰기(server 9차 보강 4 "다음 차수 후보" · 백로그 U 5번): launchd가 `server.log`를
//! 직접 쓰면 끝없이 커진다. 정의는 표준 출력을 버리고(`/dev/null`) `run --log-file <보관 폴더>/logs/server.log`로
//! 서버가 직접 돌려 쓰게 한다(65차 [`price_inha::RotatingLogFile`]). 서버가 켜지기 전 실패(바로 죽음)는 여전히
//! 표준 오류 → `server.error.log`로 남고, 서버가 기록 파일로 뜰 때 그 파일을 작은 상한
//! ([`price_inha::BOOT_LOG_MAX_BYTES`])으로 줄여 마지막 줄들만 남긴다.

use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use price_inha::{
    BOOT_LOG_MAX_BYTES, LOG_FILE_MAX_BYTES, Os, SERVER_ERROR_LOG, SERVER_LOG, SERVER_LOGS_DIR,
    Service, cap_boot_log,
};

use crate::common::{ScriptedRunner, setup};

/// 줄 사이 빈칸을 지운다(`>` 뒤 · `<` 앞).
fn squash(xml: &str) -> String {
    xml.lines().map(str::trim).collect::<Vec<_>>().join("")
}

fn plist(home: &Path, run_args: &[&str]) -> String {
    let defs = tempfile::tempdir().unwrap();
    let runner = ScriptedRunner::ok();
    let service = Service::new(setup(Os::MacOs, home, defs.path(), run_args), &runner);
    squash(&service.definition().contents)
}

// 검증: D1
#[test]
fn macos_정의는_표준_출력을_버리고_서버가_돌려_쓰는_기록_파일을_인자로_싣는다() {
    let home = tempfile::tempdir().unwrap();
    let logs = home.path().join(SERVER_LOGS_DIR);
    let text = plist(home.path(), &[]);
    assert!(
        text.contains("<key>StandardOutPath</key><string>/dev/null</string>"),
        "표준 출력은 버린다(서버가 기록 파일에 쓴다):\n{text}"
    );
    let error = format!(
        "<key>StandardErrorPath</key><string>{}</string>",
        logs.join(SERVER_ERROR_LOG).display()
    );
    assert!(text.contains(&error), "켜지기 전 오류는 남긴다:\n{text}");
    let args = format!(
        "<string>--log-file</string><string>{}</string></array>",
        logs.join(SERVER_LOG).display()
    );
    assert!(text.contains(&args), "run 인자에 기록 파일:\n{text}");

    let given = plist(home.path(), &["--log-file", "/var/tmp/pi.log"]);
    assert_eq!(
        given.matches("--log-file").count(),
        1,
        "사용자가 준 기록 파일을 따른다:\n{given}"
    );
    assert!(given.contains("<string>/var/tmp/pi.log</string>"));
}

fn lines(count: usize) -> String {
    (0..count)
        .map(|n| format!("켜지기 전 오류 줄 {n:06} {}\n", "x".repeat(40)))
        .collect()
}

// 검증: D1
#[test]
fn 켜지기_전_오류_파일은_작은_상한으로_줄여_마지막_줄들만_남긴다() {
    const { assert!(BOOT_LOG_MAX_BYTES >= 16 * 1024 && BOOT_LOG_MAX_BYTES < LOG_FILE_MAX_BYTES) };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SERVER_ERROR_LOG);

    assert!(
        !cap_boot_log(&path, 1000).unwrap(),
        "없으면 아무것도 하지 않는다"
    );
    assert!(!path.exists(), "없는 파일을 만들지 않는다");

    std::fs::write(&path, lines(3)).unwrap();
    assert!(!cap_boot_log(&path, 1000).unwrap(), "상한 안이면 그대로");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), lines(3));

    std::fs::write(&path, lines(200)).unwrap();
    assert!(cap_boot_log(&path, 1000).unwrap(), "넘으면 줄인다");
    let kept = std::fs::read_to_string(&path).unwrap();
    assert!(kept.len() <= 1000, "상한 안: {}", kept.len());
    assert!(
        kept.lines()
            .all(|line| line.starts_with("켜지기 전 오류 줄 ")),
        "줄을 가운데에서 자르지 않는다: {kept}"
    );
    assert!(kept.contains("줄 000199"), "마지막 줄은 남긴다");
    assert!(!kept.contains("줄 000000"));
}

// 검증: D1
#[test]
fn 기록_파일로_띄우면_켜지기_전_오류_파일을_상한으로_줄인다() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("pi-home");
    let logs = home.join(SERVER_LOGS_DIR);
    std::fs::create_dir_all(&logs).unwrap();
    let error_log = logs.join(SERVER_ERROR_LOG);
    let big = lines(10_000);
    assert!(big.len() as u64 > BOOT_LOG_MAX_BYTES);
    std::fs::File::create(&error_log)
        .unwrap()
        .write_all(big.as_bytes())
        .unwrap();
    // 이미 쓰이는 포트로 띄워 곧바로 끝나게 한다(기록 파일을 연 뒤 실패).
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_price-inha"))
        .args(["run", "--demo", "--port", &port, "--home"])
        .arg(&home)
        .arg("--log-file")
        .arg(logs.join(SERVER_LOG))
        .env_clear()
        .env("HOME", root.path().join("user"))
        .env("USERPROFILE", root.path().join("user"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("쓰이는 포트로는 곧 끝나야 한다");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(!status.success(), "쓰이는 포트라 뜨지 못한다");
    drop(taken);
    let kept = std::fs::read_to_string(&error_log).unwrap();
    assert!(
        kept.len() as u64 <= BOOT_LOG_MAX_BYTES,
        "켜지기 전 오류 파일을 상한으로 줄인다: {}",
        kept.len()
    );
    assert!(kept.contains("줄 009999"), "마지막 줄은 남긴다");
}
