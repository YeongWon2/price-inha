//! 실행 파일 자체: `--version`은 보관 폴더 없이 답하고, `run`을 붙여도 지금 동작과 같다.
//! 서버를 띄우지 않는 명령은 곧 끝나고, 보관 폴더를 만들거나 고치지 않는다(9차 보강 2).
//! 서버를 띄우는 시험은 하나뿐이다(9차 보강 3의 `--log-file`): 시연 모드로 루프백 빈 포트에 잠깐 띄웠다 끈다.
//! 사용자 홈을 임시 폴더로 돌려 두므로 서비스 정의 폴더도 그 안이고, 정의가 없으니 실제 서비스 관리자를
//! 부르지 않는다.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::common::files_under;

/// 띄우기 · 끄기를 기다리는 상한.
const STEP: Duration = Duration::from_secs(30);

/// 보관 폴더 · 사용자 홈을 임시 폴더 안 없는 곳으로 두고 실행 파일을 부른다.
fn price_inha(args: &[&str], root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_price-inha"))
        .args(args)
        .env_clear()
        .env(price_inha::ENV_HOME, root.join("pi-home"))
        .env("HOME", root.join("user"))
        .env("USERPROFILE", root.join("user"))
        .output()
        .unwrap()
}

// 검증: D1
#[test]
fn 판_번호는_보관_폴더_없이도_답한다() {
    let root = tempfile::tempdir().unwrap();
    let out = price_inha(&["--version"], root.path());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "판 번호를 보여 준다: {stdout}"
    );
    assert!(
        !root.path().join("pi-home").exists(),
        "보관 폴더를 만들지 않는다"
    );
    assert!(
        !root.path().join("user").exists(),
        "사용자 홈도 건드리지 않는다"
    );
}

// 검증: D1
#[test]
fn run을_붙여도_설정_오류는_지금과_같다() {
    let root = tempfile::tempdir().unwrap();
    let bare = price_inha(&["--port", "not-a-port"], root.path());
    let run = price_inha(&["run", "--port", "not-a-port"], root.path());
    assert_eq!(bare.status.code(), Some(2), "설정 치명 오류는 종료 코드 2");
    assert_eq!(run.status.code(), bare.status.code(), "같은 종료 코드");
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&bare.stderr),
        "같은 안내"
    );
}

// 검증: D1 N8
#[cfg(unix)]
#[test]
fn doctor는_넓은_보관_폴더를_고치지_않고_알린다() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("wide-home");
    std::fs::create_dir(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755)).unwrap();

    let out = price_inha(&["doctor", "--home", home.to_str().unwrap()], root.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "문제가 남으면 1: {stdout}");
    assert!(
        stdout.contains(&format!("chmod 700 {}", home.display())),
        "고치기를 보여 준다: {stdout}"
    );
    let mode = std::fs::metadata(&home).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "진단은 권한을 고치지 않는다");
    assert!(
        files_under(&home).is_empty(),
        "진단은 보관 폴더에 아무것도 만들지 않는다: {:?}",
        files_under(&home)
    );
}

// 검증: D1
#[test]
fn doctor는_없는_보관_폴더를_만들지_않는다() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("no-such-home");
    let out = price_inha(&["doctor", "--home", home.to_str().unwrap()], root.path());
    assert_eq!(
        out.status.code(),
        Some(1),
        "보관 폴더가 없는 것은 문제다: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(!home.exists(), "진단이 보관 폴더를 만들면 안 된다");
}

// 검증: D1
#[test]
fn service_status는_없는_보관_폴더를_만들지_않는다() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("no-such-home");
    let out = price_inha(
        &["service", "status", "--home", home.to_str().unwrap()],
        root.path(),
    );
    assert!(
        out.status.success(),
        "등록되지 않은 상태를 답한다: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!home.exists(), "상태 보기가 보관 폴더를 만들면 안 된다");
}

/// 띄운 서버. 시험이 도중에 실패해도 남지 않게 버릴 때 죽인다.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Running {
    /// 끄기 신호를 보낸다(유닉스는 SIGTERM — 서비스 관리자가 멈출 때와 같다).
    #[cfg(unix)]
    fn stop(&mut self) {
        let sent = Command::new("/bin/kill")
            .args(["-TERM", &self.0.id().to_string()])
            .status()
            .unwrap();
        assert!(sent.success(), "끄기 신호를 보내야 한다");
    }

    /// 유닉스가 아니면 바로 끝낸다.
    #[cfg(not(unix))]
    fn stop(&mut self) {
        self.0.kill().unwrap();
    }

    /// 끝날 때까지 기다린다(상한 안에 끝나지 않으면 실패).
    fn finished(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + STEP;
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "시간 안에 꺼져야 한다");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

// 검증: D1 E38
#[test]
fn log_file을_주면_기록이_그_파일에_생긴다() {
    let root = tempfile::tempdir().unwrap();
    let log = root.path().join("not-yet").join("logs").join("server.log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_price-inha"))
        .args(["run", "--demo", "--port", "0", "--log-file"])
        .arg(&log)
        .env_clear()
        .env(price_inha::ENV_HOME, root.path().join("pi-home"))
        .env("HOME", root.path().join("user"))
        .env("USERPROFILE", root.path().join("user"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let mut server = Running(child);

    // 표준 오류의 안내 줄을 읽는다. 열렸다는 줄이 오면 알리고, 끝까지 읽어 둔다.
    let (opened, wait_opened) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut said = String::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if line.contains("열렸습니다") {
                let _ = opened.send(());
            }
            said.push_str(&line);
            said.push('\n');
        }
        said
    });
    let up = wait_opened.recv_timeout(STEP);
    if up.is_err() {
        drop(server);
        panic!(
            "--log-file을 주고도 서버가 떠야 한다:\n{}",
            reader.join().unwrap()
        );
    }
    server.stop();
    let status = server.finished();
    let said = reader.join().unwrap();
    #[cfg(unix)]
    assert!(status.success(), "끄기 신호로 깨끗이 끝난다: {said}");
    #[cfg(not(unix))]
    assert!(!said.is_empty() && status.code().is_some());

    let written = std::fs::read_to_string(&log).expect("없던 폴더도 만들어 기록 파일을 쓴다");
    assert!(!written.trim().is_empty(), "기록 파일에 내용이 있다");
}
