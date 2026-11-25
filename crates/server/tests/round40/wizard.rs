//! 39차 8번: `price-inha setup messenger` — 도는 서버(가짜 텔레그램에 이은)로 토큰 → 확인 → 코드 → 연결까지 가고, 출력에
//! 토큰 글자가 없다(결정 24 · V91 · D1 · D4). 실행 파일(`CARGO_BIN_EXE_price-inha`)을 띄워 표준 입력으로 답을 넣는다
//! (round38 `run.rs`의 길을 본떴다). 환경은 비우고 `HOME`(임시) · `PATH`만 준다.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use price_inha::{
    SETUP_DONE, SETUP_MESSENGER_PAIRED, SETUP_MESSENGER_SEND_HINT, SETUP_SKIPPED,
    SETUP_SUMMARY_TITLE,
};
use serde_json::json;

use crate::common::{
    Bench, FakeTelegram, QUIET, STEP, TOKEN, code_in, is_pairing_reply, plain_files_with,
};

/// 연결할 사람.
const OWNER: i64 = 85_000_001;

/// 띄운 실행 파일. 시험이 도중에 실패해도 남지 않게 버릴 때 죽인다.
struct Wizard {
    child: Child,
    lines: mpsc::Receiver<String>,
    out: Option<std::thread::JoinHandle<String>>,
    err: Option<std::thread::JoinHandle<String>>,
}

impl Drop for Wizard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 한 번 돈 결과.
struct Ran {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Ran {
    fn all(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }

    /// 끝 요약 표의 (구역, 결과) 줄들.
    fn summary(&self) -> Vec<(String, String)> {
        let lines: Vec<&str> = self.stdout.lines().collect();
        let Some(title) = lines
            .iter()
            .rposition(|line| line.trim() == SETUP_SUMMARY_TITLE)
        else {
            return Vec::new();
        };
        lines[title + 1..]
            .iter()
            .map(|line| line.trim())
            .filter(|line| line.starts_with('|'))
            .map(|line| {
                line.split('|')
                    .map(str::trim)
                    .filter(|cell| !cell.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|cells| cells.len() >= 2 && cells[0] != "구역" && !cells[0].starts_with('-'))
            .map(|cells| (cells[0].clone(), cells[1].clone()))
            .collect()
    }
}

impl Wizard {
    /// `setup messenger --stdin-answers --home <홈> --port <포트>`를 띄운다.
    fn spawn(user: &Path, home: &Path, port: u16) -> Wizard {
        let port = port.to_string();
        let mut child = Command::new(env!("CARGO_BIN_EXE_price-inha"))
            .args([
                "setup",
                "messenger",
                "--stdin-answers",
                "--home",
                home.to_str().unwrap(),
                "--port",
                &port,
            ])
            .env_clear()
            .env("HOME", user)
            .env("USERPROFILE", user)
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let (send, lines) = mpsc::channel();
        let out = std::thread::spawn(move || {
            let mut all = String::new();
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = send.send(line.clone());
                all.push_str(&line);
                all.push('\n');
            }
            all
        });
        let err = std::thread::spawn(move || {
            let mut all = String::new();
            let _ = stderr.read_to_string(&mut all);
            all
        });
        Wizard {
            child,
            lines,
            out: Some(out),
            err: Some(err),
        }
    }

    fn write(&mut self, text: &str) {
        let stdin = self.child.stdin.as_mut().unwrap();
        let _ = stdin.write_all(text.as_bytes());
        let _ = stdin.flush();
    }

    /// `needle`이 든 출력 줄이 올 때까지 기다린다. 오면 참.
    fn wait_line(&self, needle: &str) -> bool {
        let deadline = Instant::now() + STEP;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.lines.recv_timeout(left) {
                Ok(line) if line.contains(needle) => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
        false
    }

    /// 표준 입력을 닫고 끝날 때까지 기다린다.
    fn finish(mut self) -> Ran {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + STEP;
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "시간 안에 끝나야 한다");
            std::thread::sleep(Duration::from_millis(20));
        };
        Ran {
            code: status.code(),
            stdout: self.out.take().unwrap().join().unwrap(),
            stderr: self.err.take().unwrap().join().unwrap(),
        }
    }
}

// 검증: V91 D1 D4 V88 N8
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn setup_messenger는_토큰에서_코드를_거쳐_연결까지_가고_출력에_토큰이_없다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let user = tempfile::tempdir().unwrap();
    let (user_dir, home, port) = (
        user.path().to_path_buf(),
        bench.home().to_path_buf(),
        bench.addr().port(),
    );

    let (wizard, hinted) = tokio::task::spawn_blocking(move || {
        let mut wizard = Wizard::spawn(&user_dir, &home, port);
        wizard.write(&format!("{TOKEN}\n"));
        let hinted = wizard.wait_line(SETUP_MESSENGER_SEND_HINT);
        (wizard, hinted)
    })
    .await
    .unwrap();
    assert!(hinted, "토큰이 확인되면 봇에게 말을 걸라고 안내한다");

    tg.push_dm(OWNER, "안녕하세요");
    let reply = tg.wait_sent(OWNER, "코드 답", is_pairing_reply).await;
    let code = code_in(&reply).unwrap();
    let ran = tokio::task::spawn_blocking(move || {
        let mut wizard = wizard;
        wizard.write(&format!("{code}\n"));
        wizard.finish()
    })
    .await
    .unwrap();

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.summary(),
        vec![("messenger".to_string(), SETUP_DONE.to_string())],
        "{}",
        ran.stdout
    );
    assert!(
        ran.stdout.contains(SETUP_MESSENGER_PAIRED),
        "{}",
        ran.stdout
    );
    assert!(
        !ran.all().contains(TOKEN),
        "출력에 토큰이 없다:\n{}",
        ran.all()
    );
    let status = bench.status().await;
    assert_eq!(status["telegram"]["configured"], json!(true), "{status}");
    assert_eq!(status["telegram"]["paired"], json!(true), "{status}");
    assert!(plain_files_with(bench.home(), TOKEN).is_empty());
    assert!(
        plain_files_with(user.path(), TOKEN).is_empty(),
        "사용자 폴더에도 남기지 않는다"
    );
    bench.close().await;
}

// 검증: V91 D1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn setup_messenger에서_빈_토큰_줄이면_건너뛰고_아무것도_부르지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let user = tempfile::tempdir().unwrap();
    let (user_dir, home, port) = (
        user.path().to_path_buf(),
        bench.home().to_path_buf(),
        bench.addr().port(),
    );

    let ran = tokio::task::spawn_blocking(move || {
        let mut wizard = Wizard::spawn(&user_dir, &home, port);
        wizard.write("\n");
        wizard.finish()
    })
    .await
    .unwrap();

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.summary(),
        vec![("messenger".to_string(), SETUP_SKIPPED.to_string())],
        "{}",
        ran.stdout
    );
    tokio::time::sleep(QUIET).await;
    assert!(tg.requests().await.is_empty(), "텔레그램을 부르지 않는다");
    let status = bench.status().await;
    assert_eq!(status["telegram"]["configured"], json!(false), "{status}");
    bench.close().await;
}
