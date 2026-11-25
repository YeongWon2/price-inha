//! 터미널 마법사의 신호 지킴이: 도중에 끊겨도(Ctrl+C · 끄기 신호 · 창 닫힘) 꺼 둔 echo를 되돌리고, 마법사가
//! 띄운 서버에 끄기 신호를 보낸 뒤 끝낸다. 지킴이를 세우지 못하면 비밀값을 보이지 않게 읽지 않는다
//! ([`super::prompt::EchoOff::begin`]).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::prompt::{ECHO_OFF, run_stty};
use crate::cli::runner::{CommandLine, CommandRunner, SystemRunner};

/// 신호로 끊겼을 때의 종료 코드(128 + SIGINT).
const INTERRUPTED_EXIT: i32 = 130;

/// 지킴이가 섰는가.
static GUARDED: AtomicBool = AtomicBool::new(false);
/// 마법사가 띄운 서버의 프로세스 번호(없으면 0).
static SERVER_PID: AtomicU32 = AtomicU32::new(0);

/// 지킴이가 섰는가(보이지 않는 입력을 써도 되는가).
pub(crate) fn guarded() -> bool {
    GUARDED.load(Ordering::SeqCst)
}

/// 마법사가 띄운 서버를 적는다(0이면 지운다).
pub(crate) fn remember_server(pid: u32) {
    SERVER_PID.store(pid, Ordering::SeqCst);
}

/// 프로세스에 끄기 신호(SIGTERM)를 보낸다. 보냈는가.
pub(crate) fn terminate(runner: &dyn CommandRunner, pid: u32) -> bool {
    let command = CommandLine {
        program: "kill".to_string(),
        args: vec!["-TERM".to_string(), pid.to_string()],
    };
    runner.run(&command).is_ok_and(|output| output.success)
}

/// 지킴이를 세운다(터미널 마법사만, tokio 실행기 안에서). 신호를 받으면 되돌리고 끝낸다.
#[cfg(unix)]
pub(crate) fn watch() {
    use tokio::signal::unix::{SignalKind, signal};
    let (Ok(mut interrupt), Ok(mut stop), Ok(mut hangup)) = (
        signal(SignalKind::interrupt()),
        signal(SignalKind::terminate()),
        signal(SignalKind::hangup()),
    ) else {
        return;
    };
    GUARDED.store(true, Ordering::SeqCst);
    tokio::spawn(async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = stop.recv() => {}
            _ = hangup.recv() => {}
        }
        restore_and_exit();
    });
}

/// 유닉스가 아니면 지킴이를 세우지 않는다(보이는 입력으로 물러선다).
#[cfg(not(unix))]
pub(crate) fn watch() {}

fn restore_and_exit() -> ! {
    let runner = SystemRunner;
    if ECHO_OFF.load(Ordering::SeqCst) {
        run_stty(&runner, "echo");
    }
    let pid = SERVER_PID.load(Ordering::SeqCst);
    if pid != 0 {
        terminate(&runner, pid);
    }
    {
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = writeln!(out, "\n마법사를 멈췄습니다. 저장한 것은 그대로 있습니다");
        let _ = out.flush();
    }
    std::process::exit(INTERRUPTED_EXIT)
}
