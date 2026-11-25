//! 허용된 사람의 글을 명령으로 읽는다(V90). 읽기만 하고 실행하지 않는다.

/// 메신저 명령. 승인(`/approve`)은 명령에 없다(결정 24 — 승인은 웹에서만).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `/status` — 상태.
    Status,
    /// `/stop` — 멈춤.
    Stop,
    /// `/help` — 도움말.
    Help,
    /// `/approvals` — 기다리는 승인 목록.
    Approvals,
    /// `/reject <짧은 번호>` — 거절. 번호는 받은 글자 그대로(대소문자를 바꾸지 않는다).
    Reject {
        /// 거절할 승인의 짧은 번호.
        id: String,
    },
    /// `/approve …` — 메신저로는 승인하지 않는다. 부르는 쪽이 "승인은 웹에서만"을 안내한다.
    ApproveNotHere,
    /// 그 밖의 글(앞뒤 공백을 뗀 글). 모르는 명령 · 인자가 빠진 `/reject` · 다른 봇에게 보낸 명령도 여기다.
    Unknown(String),
}

/// 글을 명령으로 읽는다.
///
/// - 앞뒤 공백을 무시하고, 명령 이름의 대소문자를 가리지 않는다.
/// - 텔레그램의 `/cmd@봇이름` 꼴을 받는다. `bot_username`(앞의 `@` 없이)과 대소문자 없이 같을 때만
///   명령으로 읽고, 다른 봇 이름이면 [`Command::Unknown`]이다. `bot_username`이 `None`이면
///   `@…` 꼴은 모두 [`Command::Unknown`]이다.
/// - `/approve`(인자 유무와 상관없이)는 [`Command::ApproveNotHere`]다. `/approvals`와 헷갈리지 않는다.
pub fn parse_command(text: &str, bot_username: Option<&str>) -> Command {
    let trimmed = text.trim();
    let unknown = || Command::Unknown(trimmed.to_owned());
    let Some(body) = trimmed.strip_prefix('/') else {
        return unknown();
    };
    let mut words = body.split_whitespace();
    let head = words.next().unwrap_or_default();
    let name = match head.split_once('@') {
        Some((name, target)) => {
            let ours = bot_username
                .map(|me| me.trim_start_matches('@'))
                .is_some_and(|me| !me.is_empty() && me.eq_ignore_ascii_case(target));
            if !ours {
                return unknown();
            }
            name
        }
        None => head,
    };
    match name.to_lowercase().as_str() {
        "status" => Command::Status,
        "stop" => Command::Stop,
        "help" => Command::Help,
        "approvals" => Command::Approvals,
        "approve" => Command::ApproveNotHere,
        "reject" => match words.next() {
            Some(id) => Command::Reject { id: id.to_owned() },
            None => unknown(),
        },
        _ => unknown(),
    }
}
