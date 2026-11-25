//! 마법사의 묻기 · 읽기. 한 줄에 답 하나를 읽고, 비밀값은 터미널이면 보이지 않게 읽는다.
//!
//! 보이지 않는 입력은 새 의존 없이 바깥 명령 `stty -echo` · `stty echo`(제어 터미널 `/dev/tty`에)를
//! [`CommandRunner`]로 부른다. 읽기가 끝나면(오류 · 공황으로 풀려도) [`EchoOff`]의 `Drop`이 echo를 되돌리고,
//! 중간에 끊기는 신호(Ctrl+C 등)는 [`super::signals`]의 지킴이가 되돌린다. 지킴이를 세우지 못했거나
//! `stty`가 실패하면 보이는 입력으로 물러서고 [`VISIBLE_WARNING`]을 말한다.

use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use zeroize::Zeroizing;

use crate::cli::runner::{CommandLine, CommandRunner};

/// 보이지 않게 읽지 못해 보이는 입력으로 물러설 때의 경고.
pub(crate) const VISIBLE_WARNING: &str =
    "입력이 보입니다(글자를 숨기지 못했습니다). 주위를 살피고 넣으세요";

/// 지금 echo를 꺼 두었는가(신호 지킴이가 되돌릴지 본다).
pub(crate) static ECHO_OFF: AtomicBool = AtomicBool::new(false);

/// 어떻게 읽는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// `--stdin-answers`: 질문을 한 줄로 쓰고 답 줄을 그대로 읽는다(비밀값도 같은 줄).
    Lines,
    /// 터미널: 질문 뒤에서 입력을 받는다. `hide`면 비밀값을 보이지 않게 읽을 수 있다.
    Terminal { hide: bool },
}

/// 묻고 읽는 자리.
pub(crate) struct Prompter<'a> {
    input: &'a mut dyn BufRead,
    output: &'a mut dyn Write,
    mode: Mode,
    runner: &'a dyn CommandRunner,
}

impl<'a> Prompter<'a> {
    pub(crate) fn new(
        input: &'a mut dyn BufRead,
        output: &'a mut dyn Write,
        mode: Mode,
        runner: &'a dyn CommandRunner,
    ) -> Self {
        Self {
            input,
            output,
            mode,
            runner,
        }
    }

    /// 한 줄을 쓰고 비운다.
    pub(crate) fn say(&mut self, text: &str) {
        let _ = writeln!(self.output, "{text}");
        let _ = self.output.flush();
    }

    /// 구역 제목.
    pub(crate) fn title(&mut self, text: &str) {
        self.say("");
        self.say(&format!("── {text} ──"));
    }

    fn ask(&mut self, question: &str) {
        match self.mode {
            Mode::Lines => {
                let _ = writeln!(self.output, "{question}");
            }
            Mode::Terminal { .. } => {
                let _ = write!(self.output, "{question} ");
            }
        }
        let _ = self.output.flush();
    }

    /// 묻고 한 줄을 읽는다(앞뒤 공백을 뗀다). 입력이 끝났으면 `None`.
    pub(crate) fn line(&mut self, question: &str) -> Option<String> {
        self.ask(question);
        self.read().map(|line| line.trim().to_string())
    }

    /// 묻지 않고 한 줄(Enter)을 기다린다. 입력이 끝났으면 `None`.
    pub(crate) fn wait_enter(&mut self) -> Option<()> {
        self.read().map(|_| ())
    }

    /// 예/아니오(`y` · `n`, 빈 줄 = `default`). 모르는 답이면 다시 묻는다. 입력이 끝났으면 `None`.
    pub(crate) fn yes_no(&mut self, question: &str, default: bool) -> Option<bool> {
        let hint = if default { "[Y/n]" } else { "[y/N]" };
        loop {
            let answer = self.line(&format!("{question} {hint}"))?;
            match answer.to_lowercase().as_str() {
                "" => return Some(default),
                "y" | "yes" | "예" | "네" => return Some(true),
                "n" | "no" | "아니오" | "아니요" => return Some(false),
                _ => self.say("y 또는 n으로 답하세요"),
            }
        }
    }

    /// 비밀값 한 줄. 터미널이면 보이지 않게 읽는다(못 하면 경고 뒤 보이는 입력). 앞뒤 공백을 뗀다.
    /// 읽은 값은 화면 · 기록에 다시 쓰지 않는다. 입력이 끝났으면 `None`.
    pub(crate) fn secret(&mut self, question: &str) -> Option<Zeroizing<String>> {
        let hide = matches!(self.mode, Mode::Terminal { hide: true });
        let guard = if hide {
            EchoOff::begin(self.runner)
        } else {
            None
        };
        if matches!(self.mode, Mode::Terminal { .. }) && guard.is_none() {
            self.say(VISIBLE_WARNING);
        }
        self.ask(question);
        let read = self.read_secret();
        if let Some(guard) = guard {
            drop(guard);
            // Enter가 보이지 않았으니 줄을 바꿔 둔다.
            self.say("");
        }
        read
    }

    fn read(&mut self) -> Option<String> {
        let mut line = String::new();
        match self.input.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line),
        }
    }

    fn read_secret(&mut self) -> Option<Zeroizing<String>> {
        let mut raw = Zeroizing::new(String::new());
        match self.input.read_line(&mut raw) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(Zeroizing::new(raw.trim().to_string())),
        }
    }
}

/// echo를 꺼 둔 동안. 버릴 때 되돌린다.
pub(crate) struct EchoOff<'a> {
    runner: &'a dyn CommandRunner,
}

impl<'a> EchoOff<'a> {
    /// echo를 끈다. 신호 지킴이가 없거나(되돌릴 길이 없다) 끄지 못했으면 `None`(보이는 입력으로 물러선다).
    pub(crate) fn begin(runner: &'a dyn CommandRunner) -> Option<EchoOff<'a>> {
        if !super::signals::guarded() {
            return None;
        }
        ECHO_OFF.store(true, Ordering::SeqCst);
        if run_stty(runner, "-echo") {
            Some(EchoOff { runner })
        } else {
            // 반쯤 꺼졌을 수도 있으니 되돌려 둔다.
            run_stty(runner, "echo");
            ECHO_OFF.store(false, Ordering::SeqCst);
            None
        }
    }
}

impl Drop for EchoOff<'_> {
    fn drop(&mut self) {
        run_stty(self.runner, "echo");
        ECHO_OFF.store(false, Ordering::SeqCst);
    }
}

/// 제어 터미널에 `stty <설정>`을 한다. 성공했는가.
pub(crate) fn run_stty(runner: &dyn CommandRunner, setting: &str) -> bool {
    let command = CommandLine {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), format!("stty {setting} < /dev/tty")],
    };
    runner.run(&command).is_ok_and(|output| output.success)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Mutex;

    use super::*;
    use crate::cli::runner::CommandOutput;

    struct Fake {
        ok: bool,
        calls: Mutex<Vec<String>>,
    }

    impl CommandRunner for Fake {
        fn run(&self, command: &CommandLine) -> std::io::Result<CommandOutput> {
            self.calls.lock().unwrap().push(command.args.join(" "));
            Ok(CommandOutput {
                success: self.ok,
                ..CommandOutput::default()
            })
        }
    }

    #[test]
    fn 예_아니오는_모르는_답이면_다시_묻고_빈_줄은_기본값() {
        let fake = Fake {
            ok: true,
            calls: Mutex::new(Vec::new()),
        };
        let mut input = Cursor::new("maybe\n\nn\n");
        let mut out = Vec::new();
        let mut prompter = Prompter::new(&mut input, &mut out, Mode::Lines, &fake);
        assert_eq!(prompter.yes_no("켤까요?", true), Some(true));
        assert_eq!(prompter.yes_no("켤까요?", true), Some(false));
        assert_eq!(prompter.yes_no("켤까요?", true), None);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.matches("켤까요?").count(), 4);
    }

    #[test]
    fn 비밀값은_되돌려_쓰지_않고_숨기지_못하면_경고한다() {
        let fake = Fake {
            ok: false,
            calls: Mutex::new(Vec::new()),
        };
        let mut input = Cursor::new("  zz-secret-value  \n");
        let mut out = Vec::new();
        let mode = Mode::Terminal { hide: true };
        let mut prompter = Prompter::new(&mut input, &mut out, mode, &fake);
        let value = prompter.secret("키:").unwrap();
        assert_eq!(value.as_str(), "zz-secret-value");
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("zz-secret-value"));
        assert!(text.contains(VISIBLE_WARNING), "{text}");
        assert!(!ECHO_OFF.load(Ordering::SeqCst));
    }
}
