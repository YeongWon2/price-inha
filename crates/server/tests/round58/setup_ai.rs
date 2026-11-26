//! `claude`가 `PATH`에 없을 때의 AI 연결 구역: 설치 · 로그인 안내 → Enter → 다시 찾는다(결정 27 — 우리가 대신 받아
//! 돌리지 않고 안내만 한다). 찾았고 로그인돼 있으면 빈 줄이 CLI(구독)다.
//! 마법사는 실행 파일을 띄워 `--stdin-answers`로 답을 흘려 넣는다. 도는 서버가 없으므로 마법사가 띄운다("y").

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use price_inha::{CLAUDE_CODE_DOCS_URL, SETUP_CLI_INSTALL_HINT, SETUP_DONE, SETUP_SKIPPED};

/// 한 단계를 기다리는 가장 긴 시간.
const STEP: Duration = Duration::from_secs(60);

/// 임시 사용자 홈 · 보관 폴더 · 빈 `PATH` 폴더.
struct Root {
    _dir: tempfile::TempDir,
    user: PathBuf,
    home: PathBuf,
    bin: PathBuf,
}

impl Root {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let (user, home, bin) = (base.join("user"), base.join("pi-home"), base.join("bin"));
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        Self {
            _dir: dir,
            user,
            home,
            bin,
        }
    }

    /// `PATH`: 빈 폴더 + 시스템 폴더(그 안에 `claude`는 없다).
    fn path_env(&self) -> String {
        format!("{}:/usr/bin:/bin", self.bin.display())
    }

    /// 로그인된 가짜 `claude`를 `PATH` 폴더에 놓는다(공식 CLI의 버전 · 로그인 확인 모양).
    fn install_fake_claude(&self) -> PathBuf {
        let path = self.bin.join("claude");
        let auth = r#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","subscriptionType":"max"}"#;
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n--version) echo '2.1.316 (Claude Code)'; exit 0 ;;\n\
auth) echo '{auth}'; exit 0 ;;\nesac\n/bin/cat > /dev/null\nexit 1\n"
        );
        std::fs::write(&path, script).unwrap();
        make_runnable(&path);
        path
    }
}

#[cfg(unix)]
fn make_runnable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn make_runnable(_path: &Path) {}

/// 띄운 마법사. 버릴 때 죽인다.
struct Wizard {
    child: Child,
    stdin: Option<ChildStdin>,
    out: Arc<Mutex<String>>,
}

impl Wizard {
    fn spawn(root: &Root) -> Self {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let home = root.home.to_str().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_price-inha"))
            .args(["setup", "ai", "--stdin-answers", "--home", home])
            .args(["--port", &port.to_string()])
            .env_clear()
            .env("HOME", &root.user)
            .env("USERPROFILE", &root.user)
            .env("PATH", root.path_env())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let out = Arc::new(Mutex::new(String::new()));
        for pipe in [
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        {
            let out = Arc::clone(&out);
            std::thread::spawn(move || pump(pipe, &out));
        }
        let stdin = child.stdin.take();
        Self { child, stdin, out }
    }

    fn write(&mut self, text: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(text.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    fn text(&self) -> String {
        self.out.lock().unwrap().clone()
    }

    /// `needle`이 `times`번 나올 때까지 기다린다.
    fn wait_for(&self, needle: &str, times: usize) -> bool {
        let until = Instant::now() + STEP;
        while Instant::now() < until {
            if self.text().matches(needle).count() >= times {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    /// 표준 입력을 닫고 끝나기를 기다린다. (종료 코드, 출력)
    fn finish(mut self) -> (Option<i32>, String) {
        drop(self.stdin.take());
        let until = Instant::now() + STEP;
        let code = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status.code();
            }
            if Instant::now() > until {
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        std::thread::sleep(Duration::from_millis(200));
        (code, self.text())
    }
}

impl Drop for Wizard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn pump(mut pipe: Box<dyn Read + Send>, out: &Mutex<String>) {
    let mut chunk = [0u8; 4096];
    while let Ok(n) = pipe.read(&mut chunk) {
        if n == 0 {
            return;
        }
        out.lock()
            .unwrap()
            .push_str(&String::from_utf8_lossy(&chunk[..n]));
    }
}

/// 끝 요약 표에서 `ai` 구역의 결과.
fn ai_result(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('|'))
        .filter_map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            (cells.first() == Some(&"ai")).then(|| cells.get(1).map(|c| (*c).to_string()))
        })
        .next_back()
        .flatten()
}

// 검증: D4
#[test]
fn claude가_없으면_설치와_로그인을_안내하고_엔터_뒤에_다시_찾는다() {
    let root = Root::new();
    let mut wizard = Wizard::spawn(&root);
    wizard.write("y\ncli\n");
    assert!(
        wizard.wait_for(SETUP_CLI_INSTALL_HINT, 1),
        "설치 · 로그인 안내: {}",
        wizard.text()
    );
    assert!(
        wizard.text().contains(CLAUDE_CODE_DOCS_URL),
        "공식 문서 주소"
    );
    wizard.write("\n");
    assert!(
        wizard.wait_for("고르세요", 2),
        "Enter 뒤 다시 찾고 다시 묻는다: {}",
        wizard.text()
    );
    wizard.write("skip\n");
    let (code, text) = wizard.finish();
    assert_eq!(code, Some(0), "{text}");
    assert_eq!(ai_result(&text).as_deref(), Some(SETUP_SKIPPED), "{text}");
}

// 검증: D4 V31
#[test]
fn 안내_뒤에_설치하고_엔터를_누르면_찾아서_빈_줄이_cli를_고른다() {
    let root = Root::new();
    let mut wizard = Wizard::spawn(&root);
    wizard.write("y\ncli\n");
    assert!(
        wizard.wait_for(SETUP_CLI_INSTALL_HINT, 1),
        "설치 · 로그인 안내: {}",
        wizard.text()
    );
    let claude = root.install_fake_claude();
    wizard.write("\n");
    assert!(
        wizard.wait_for("로그인됨", 1),
        "다시 찾아 로그인을 확인했다: {}",
        wizard.text()
    );
    assert!(
        wizard.text().contains(&claude.display().to_string()),
        "찾은 경로를 보인다"
    );
    assert!(
        wizard.wait_for("빈 줄 = cli", 1),
        "로그인된 CLI가 기본이다: {}",
        wizard.text()
    );
    wizard.write("\n");
    let (code, text) = wizard.finish();
    assert_eq!(code, Some(0), "{text}");
    assert_eq!(ai_result(&text).as_deref(), Some(SETUP_DONE), "{text}");
}
