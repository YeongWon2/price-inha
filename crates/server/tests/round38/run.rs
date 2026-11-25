//! 실행 파일(`CARGO_BIN_EXE_price-inha`)을 띄워 표준 입력으로 답을 넣고 출력 · 종료 코드 · 끝 요약을 읽는다.
//! 환경은 비우고 `HOME`(임시) · `PATH`(가짜 `claude`가 든 폴더 + `/usr/bin:/bin`)와 시험이 더한 것만 준다.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use price_inha::SETUP_SUMMARY_TITLE;

use crate::common::{FakeCli, Root, STEP};

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

    /// 끝 요약 표의 (구역, 결과) 줄들. 제목 줄([`SETUP_SUMMARY_TITLE`]) 뒤의 `|`로 시작하는 줄에서 읽는다.
    pub fn summary(&self) -> Vec<(String, String)> {
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

    /// 요약에서 그 구역의 결과.
    pub fn result_of(&self, section: &str) -> Option<String> {
        self.summary()
            .into_iter()
            .find(|(name, _)| name == section)
            .map(|(_, result)| result)
    }

    /// 출력 어디에도 `needle`이 없다.
    pub fn assert_absent(&self, needle: &str, what: &str) {
        assert!(
            !self.all().contains(needle),
            "출력에 {what}이(가) 나오면 안 된다:\n{}",
            self.all()
        );
    }
}

/// 마법사 · `open`을 띄울 환경. 프로세스 환경은 비우고 이것만 준다.
pub struct Env {
    pub user: PathBuf,
    pub path: String,
    pub extra: Vec<(String, String)>,
}

impl Env {
    pub fn new(root: &Root, cli: &FakeCli) -> Self {
        Self {
            user: root.user.clone(),
            path: cli.path_env(),
            extra: Vec::new(),
        }
    }

    pub fn with(mut self, name: &str, value: &str) -> Self {
        self.extra.push((name.to_string(), value.to_string()));
        self
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_price-inha"));
        command
            .args(args)
            .env_clear()
            .env("HOME", &self.user)
            .env("USERPROFILE", &self.user)
            .env("PATH", &self.path);
        for (name, value) in &self.extra {
            command.env(name, value);
        }
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

    /// 표준 입력에 쓴다.
    pub fn write(&mut self, text: &str) {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .expect("표준 입력을 열어 띄웠어야 한다");
        // 마법사가 남은 답을 읽지 않고 먼저 끝났으면(닫힌 파이프) 쓰지 못한 채 넘어간다 — 남은 답을 쓰지 않은
        // 것이 맞는지는 결과로 본다.
        let _ = stdin.write_all(text.as_bytes());
        let _ = stdin.flush();
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
        let deadline = Instant::now() + STEP;
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
