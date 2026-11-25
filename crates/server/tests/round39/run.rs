//! 실행 파일(`CARGO_BIN_EXE_price-inha`)을 띄워 표준 입력으로 답을 넣고 출력 · 종료 코드를 읽는다.
//! 환경은 비우고 `HOME`(임시) · `PATH`(빈 임시 폴더 + `/usr/bin:/bin`)만 준다.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::common::{Root, STEP};

/// 실행 파일 한 번의 결과.
pub struct Ran {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Ran {
    /// 표준 출력과 표준 오류를 합친 글.
    pub fn all(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

/// 실행 파일을 띄울 환경. 프로세스 환경은 비우고 이것만 준다.
#[derive(Clone)]
pub struct Env {
    user: PathBuf,
    path: String,
}

impl Env {
    pub fn new(root: &Root) -> Self {
        Self {
            user: root.user.clone(),
            path: format!("{}:/usr/bin:/bin", root.bin.display()),
        }
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_price-inha"));
        command
            .args(args)
            .env_clear()
            .env("HOME", &self.user)
            .env("USERPROFILE", &self.user)
            .env("PATH", &self.path);
        command
    }
}

/// 띄운 실행 파일. 시험이 도중에 실패해도 남지 않게 버릴 때 죽인다.
pub struct Wizard {
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

impl Wizard {
    /// 띄운다. `stdin`이 거짓이면 표준 입력은 빈 것(터미널 아님)이다.
    pub fn spawn(env: &Env, args: &[&str], stdin: bool) -> Wizard {
        let mut child = env
            .command(args)
            .stdin(if stdin { Stdio::piped() } else { Stdio::null() })
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

    /// 프로세스 번호(끄기 신호를 보낼 때).
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// 표준 입력에 쓴다(먼저 끝나 파이프가 닫혔으면 쓰지 못한 채 넘어간다 — 결과로 본다).
    pub fn write(&mut self, text: &str) {
        if let Some(stdin) = self.child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
            let _ = stdin.flush();
        }
    }

    /// `needle`이 든 출력 줄이 올 때까지 기다린다. 오면 참.
    pub fn wait_line(&self, needle: &str) -> bool {
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

    /// 표준 입력을 닫고 끝날 때까지 기다린다(상한을 넘으면 죽이고 실패).
    pub fn finish(mut self) -> Ran {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + STEP * 2;
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("시간 안에 끝나야 한다");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        Ran {
            code: status.code(),
            stdout: self.out.take().unwrap().join().unwrap(),
            stderr: self.err.take().unwrap().join().unwrap(),
        }
    }
}

/// 실행 파일을 띄워 `answers`를 표준 입력으로 넣고 끝을 기다린다(막는 일이라 따로 돌린다).
pub async fn run_binary(env: Env, args: Vec<String>, answers: Option<String>) -> Ran {
    tokio::task::spawn_blocking(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut wizard = Wizard::spawn(&env, &args, answers.is_some());
        if let Some(answers) = answers {
            wizard.write(&answers);
        }
        wizard.finish()
    })
    .await
    .unwrap()
}

/// 글자 목록 → 인자.
pub fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}
