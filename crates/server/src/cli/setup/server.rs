//! 마법사가 값을 저장하는 길: 도는 서버의 같은 컴퓨터(루프백) 설정 경로. 도는 서버를 `access-key`와 같은
//! 차례로 찾고, 없으면 묻고 뒤에서 띄운다(설정 인자만 싣는다 — 비밀값은 인자 · 환경으로 넘기지 않는다).
//! 마법사가 띄운 서버는 [`Spawned::stop`](끄기 신호 → 기다림 → 그래도 살아 있으면 죽임)으로 끄고, 그 전에
//! 마법사가 끝나도(공황 등) `Drop`이 죽이고 거둔다 — 자식 프로세스가 남지 않는다.

use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use zeroize::Zeroizing;

use super::prompt::Prompter;
use super::signals::{remember_server, terminate};
use super::{SETUP_FAILED_EXIT, SETUP_RUN_HINT, SETUP_START_SERVER_QUESTION, SetupRequest};
use crate::cli::access::{Located, Place, Running, locate, places_for};
use crate::cli::proof::raw_proof;
use crate::cli::runner::SystemRunner;
use crate::cli::service::{Service, ServiceSetup};
use crate::{NOT_THIS_HOME_SERVER, Settings, SetupRefusal};

/// 설정 경로 한 번의 상한(등록 전 확인이 바깥에 닿는 시간을 넉넉히 둔다).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
/// 띄운 서버가 답하기를 기다리는 상한.
const START_WAIT: Duration = Duration::from_secs(60);
/// 끄기 신호 뒤 스스로 끝나기를 기다리는 상한.
const STOP_WAIT: Duration = Duration::from_secs(15);
/// 기다리며 다시 볼 간격.
const POLL: Duration = Duration::from_millis(100);

/// 응답의 `error` 글자로 되돌릴 거절들.
const REFUSALS: [SetupRefusal; 14] = [
    SetupRefusal::KeyRejected,
    SetupRefusal::Unreachable,
    SetupRefusal::InvalidInput,
    SetupRefusal::TooManyAttempts,
    SetupRefusal::FixedByArgument,
    SetupRefusal::SaveFailed,
    SetupRefusal::BodyTooLarge,
    SetupRefusal::CliNotInstalled,
    SetupRefusal::CliTooOld,
    SetupRefusal::CliNotLoggedIn,
    SetupRefusal::CliNotSubscription,
    SetupRefusal::CliUnchecked,
    SetupRefusal::CliWorkDirNotEmpty,
    SetupRefusal::ShuttingDown,
];

/// 설정 경로의 답.
pub(crate) enum Reply {
    /// 200(받아들였다).
    Accepted,
    /// 서버가 거절했다(그 까닭).
    Refused(SetupRefusal),
    /// 닿지 못했거나 답을 읽지 못했다.
    Broken,
}

/// 도는 서버와 그 컴퓨터의 세션.
pub(crate) struct Link {
    running: Running,
    /// 찾은 루프백 포트.
    pub(crate) port: u16,
}

impl Link {
    /// `GET` — 200이면 본문.
    pub(crate) async fn get(&self, path: &str) -> Option<Value> {
        match self.running.send(path, None, REQUEST_TIMEOUT).await {
            Some((200, value)) => Some(value),
            _ => None,
        }
    }

    /// `POST`(JSON 본문). 본문에 비밀값이 들 수 있다 — 기록하지 않는다.
    pub(crate) async fn post(&self, path: &str, body: Vec<u8>) -> Reply {
        match self.running.send(path, Some(body), REQUEST_TIMEOUT).await {
            None => Reply::Broken,
            Some((200, _)) => Reply::Accepted,
            Some((_, value)) => value
                .get("error")
                .and_then(Value::as_str)
                .and_then(refusal_named)
                .map_or(Reply::Broken, Reply::Refused),
        }
    }

    /// (39차) `POST`(JSON 본문)의 상태와 몸통 그대로. 닿지 못했으면 `None`. 본문을 기록하지 않는다.
    pub(crate) async fn post_raw(&self, path: &str, body: Vec<u8>) -> Option<(u16, Value)> {
        self.running.send(path, Some(body), REQUEST_TIMEOUT).await
    }
}

/// (39차) `error` 글자 → 거절(없으면 `None`).
pub(crate) fn refusal_of(value: &Value) -> Option<SetupRefusal> {
    value
        .get("error")
        .and_then(Value::as_str)
        .and_then(refusal_named)
}

/// 다시 물어도 소용없는 거절(그 구역은 실패로 적고 다음 구역으로 간다).
pub(crate) fn gives_up(refusal: SetupRefusal) -> bool {
    matches!(
        refusal,
        SetupRefusal::TooManyAttempts
            | SetupRefusal::FixedByArgument
            | SetupRefusal::SaveFailed
            | SetupRefusal::ShuttingDown
    )
}

/// `error` 글자 → 거절.
fn refusal_named(reason: &str) -> Option<SetupRefusal> {
    REFUSALS
        .into_iter()
        .find(|refusal| refusal.reason() == reason)
}

/// 마법사가 띄운 서버.
pub(crate) struct Spawned {
    child: Option<Child>,
}

impl Spawned {
    fn exited(&mut self) -> Option<ExitStatus> {
        self.child
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten())
    }

    /// 끈다: 끄기 신호를 보내고 기다린 뒤, 그래도 살아 있으면 죽이고 거둔다.
    pub(crate) async fn stop(mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if terminate(&SystemRunner, child.id()) {
            let deadline = Instant::now() + STOP_WAIT;
            while Instant::now() < deadline {
                if let Ok(Some(_)) = child.try_wait() {
                    remember_server(0);
                    return;
                }
                tokio::time::sleep(POLL).await;
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        remember_server(0);
    }
}

impl Drop for Spawned {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            remember_server(0);
        }
    }
}

/// 도는 서버에 닿은 결과.
pub(crate) enum Connected {
    /// 닿았다. 마법사가 띄웠으면 그 서버도.
    Ready(Link, Option<Spawned>),
    /// 이어 갈 수 없다(그 종료 코드로 끝낸다). 아무것도 저장하지 않았다.
    Stop(u8),
}

/// 도는 서버를 찾고, 없으면 묻고 띄운다.
pub(crate) async fn connect(
    settings: &Settings,
    request: &SetupRequest,
    server_exe: &Path,
    prompter: &mut Prompter<'_>,
) -> Connected {
    let runner = SystemRunner;
    let service = ServiceSetup::for_this_computer(&settings.home, Vec::new())
        .ok()
        .map(|setup| Service::new(setup, &runner));
    match locate(&settings.home, places_for(settings, service.as_ref())).await {
        Located::Ours(running, port) => Connected::Ready(Link { running, port }, None),
        Located::NotOurs(port) => {
            prompter.say(&not_ours(port));
            Connected::Stop(SETUP_FAILED_EXIT)
        }
        Located::NotThisHome(port) => {
            prompter.say(&not_this_home(port));
            Connected::Stop(SETUP_FAILED_EXIT)
        }
        Located::Nowhere => match prompter.yes_no(SETUP_START_SERVER_QUESTION, true) {
            Some(true) => start(settings, request, server_exe, prompter).await,
            _ => {
                prompter.say(SETUP_RUN_HINT);
                Connected::Stop(SETUP_FAILED_EXIT)
            }
        },
    }
}

fn not_ours(port: u16) -> String {
    format!("127.0.0.1:{port}에서 이 서버가 아닌 것이 답합니다. --port를 확인하세요")
}

/// (38차) 세션은 주지만 실행 증표로 이 보관 폴더의 서버임을 확인하지 못했다. 아무것도 보내지 않았다.
fn not_this_home(port: u16) -> String {
    format!(
        "127.0.0.1:{port}: {NOT_THIS_HOME_SERVER}. 키를 보내지 않았습니다. --home 과 --port 를 확인하세요"
    )
}

/// (38차) 띄운 서버가 새 증표 파일을 썼나(띄우기 전에 남아 있던 `stale`과 다른 글자의 파일이 있다).
fn fresh_proof(home: &Path, stale: Option<&Zeroizing<String>>) -> bool {
    raw_proof(home).is_some_and(|now| stale.is_none_or(|old| old.as_str() != now.as_str()))
}

/// `run`과 같은 설정 인자로 뒤에서 띄우고 답할 때까지 기다린다.
async fn start(
    settings: &Settings,
    request: &SetupRequest,
    server_exe: &Path,
    prompter: &mut Prompter<'_>,
) -> Connected {
    let port = settings.port;
    // 띄우기 전에 남은 증표 파일(끝내 지우지 못하고 죽은 서버의 것)은 띄운 서버의 것이 아니다.
    let stale = raw_proof(&settings.home);
    if port == 0 {
        prompter.say("포트 0(빈 포트를 고름)으로는 띄운 서버를 찾을 수 없습니다. --port를 주세요");
        return Connected::Stop(SETUP_FAILED_EXIT);
    }
    let spawned = Command::new(server_exe)
        .arg("run")
        .args(&request.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let child = match spawned {
        Ok(child) => child,
        Err(error) => {
            prompter.say(&format!("서버를 띄우지 못했습니다: {}", error.kind()));
            prompter.say(SETUP_RUN_HINT);
            return Connected::Stop(SETUP_FAILED_EXIT);
        }
    };
    remember_server(child.id());
    let mut spawned = Spawned { child: Some(child) };
    let deadline = Instant::now() + START_WAIT;
    while Instant::now() < deadline {
        if let Some(status) = spawned.exited() {
            prompter.say(&format!(
                "띄운 서버가 바로 끝났습니다({status}). `price-inha run`으로 까닭을 보세요"
            ));
            return Connected::Stop(SETUP_FAILED_EXIT);
        }
        // (38차) 자식이라는 것만으로 믿지 않는다: 띄운 서버가 새 증표 파일을 쓸 때까지 기다린 뒤 같은 확인을
        // 거친다(띄운 서버가 포트를 얻지 못하고 다른 것이 그 포트에서 답하면 확인에 실패한다).
        if !fresh_proof(&settings.home, stale.as_ref()) {
            tokio::time::sleep(POLL).await;
            continue;
        }
        match locate(&settings.home, vec![Place::loopback(port)]).await {
            Located::Ours(running, port) => {
                prompter.say(&format!(
                    "서버를 뒤에서 띄웠습니다(127.0.0.1:{port}). 서비스로 등록하지 않으면 마법사가 끝날 때 끕니다"
                ));
                return Connected::Ready(Link { running, port }, Some(spawned));
            }
            Located::NotOurs(port) => {
                prompter.say(&not_ours(port));
                spawned.stop().await;
                return Connected::Stop(SETUP_FAILED_EXIT);
            }
            Located::NotThisHome(port) => {
                prompter.say(&not_this_home(port));
                spawned.stop().await;
                return Connected::Stop(SETUP_FAILED_EXIT);
            }
            Located::Nowhere => tokio::time::sleep(POLL).await,
        }
    }
    prompter.say("띄운 서버가 시간 안에 답하지 않았습니다. `price-inha run`으로 까닭을 보세요");
    spawned.stop().await;
    Connected::Stop(SETUP_FAILED_EXIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 거절_글자는_모두_되돌린다() {
        for refusal in REFUSALS {
            assert_eq!(refusal_named(refusal.reason()), Some(refusal));
        }
        assert_eq!(refusal_named("nope"), None);
    }
}
