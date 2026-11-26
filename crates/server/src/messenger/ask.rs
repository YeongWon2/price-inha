//! (41차 3 · 4번) 허용된 사람의 글 나누기: 명령(39 · 40차 그대로) · `/notice <글>`(사무실 전체 공지와 같은 길) ·
//! `@담당` 묻기(사무실 팀 방 멘션과 같은 길). 길이 · 보내기 간격 · 하루 AI 호출 상한은 **AI를 부르기 전에** 판정하고
//! 그 까닭을 답한다. 담당의 답 · 공지 확인 답은 끝난 뒤 뒤에서 보낸다(나가는 글은 명령으로 읽지 않는다).

use std::sync::Arc;
use std::time::Duration;

use pi_channel::{Command, Inbound, parse_command};
use pi_office::{AiBudget, ChatStatus};

use super::commands::{
    MESSENGER_ASK_CAPPED, MESSENGER_ASK_HINT, MESSENGER_ASK_TOO_FAST, MESSENGER_ASK_TOO_LONG,
    MESSENGER_NOTICE_ACKS, MESSENGER_NOTICE_EMPTY, MESSENGER_UNKNOWN,
};
use super::hub::Messenger;
use super::talk::display_name;
use crate::reach::BridgeTurn;
use crate::{CHAT_TOTAL_TIMEOUT, OFFICE_NO_MATCH_TEXT, OFFICE_TEXT_MAX};

/// 사무실로 넘기지 못했을 때.
const HAND_OFF_FAILED: &str = "지금 담당에게 넘기지 못했습니다 — 잠시 뒤에 다시 해 주세요";
/// 담당이 답하지 못했을 때(그 담당 줄의 글).
const NO_ANSWER: &str = "답하지 못했습니다";
/// 공지 확인을 하루 상한으로 받지 못했을 때(그 담당 줄의 글).
const ACK_CAPPED: &str = "오늘 AI 호출 상한으로 확인하지 못했습니다";
/// 답이 끝났는지 다시 보는 간격(벽시계).
const SETTLE_TICK: Duration = Duration::from_millis(50);

/// 허용된 사람의 글 하나: 바로 할 답(가리기 전 글), 담당 답처럼 뒤에서 보낼 것만 있으면 없음.
pub(super) async fn route(
    hub: &Arc<Messenger>,
    item: &Inbound,
    bot: Option<&str>,
) -> Option<String> {
    let text = match parse_command(&item.text, bot) {
        Command::Unknown(text) => text,
        command => return Some(super::commands::answer(hub, command).await),
    };
    let source = format!("messenger-{}", item.message_id);
    if let Some(body) = notice_body(&text, bot) {
        return notice(hub, body, &source).await;
    }
    if text.starts_with('/') {
        return Some(MESSENGER_UNKNOWN.to_string());
    }
    ask(hub, &text, &source).await
}

/// `/notice <글>`(또는 `/notice@봇이름 <글>`)이면 뒤 글(앞뒤 공백을 뗀 것).
fn notice_body<'a>(text: &'a str, bot: Option<&str>) -> Option<&'a str> {
    let body = text.trim().strip_prefix('/')?;
    let (head, rest) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
    let name = match head.split_once('@') {
        Some((name, target)) => {
            let ours = bot
                .map(|me| me.trim_start_matches('@'))
                .is_some_and(|me| !me.is_empty() && me.eq_ignore_ascii_case(target));
            ours.then_some(name)?
        }
        None => head,
    };
    name.eq_ignore_ascii_case("notice").then(|| rest.trim())
}

/// `@담당` 묻기: 판정을 모두 지나면 사무실 팀 방 멘션과 같은 길로 넘기고, 답은 뒤에서 보낸다.
async fn ask(hub: &Arc<Messenger>, text: &str, source: &str) -> Option<String> {
    if text.chars().count() > OFFICE_TEXT_MAX {
        return Some(MESSENGER_ASK_TOO_LONG.to_string());
    }
    let read = super::mention::read(text);
    if !read.any {
        return Some(format!("{MESSENGER_UNKNOWN}\n{MESSENGER_ASK_HINT}"));
    }
    if read.agents.is_empty() {
        return Some(OFFICE_NO_MATCH_TEXT.to_string());
    }
    if !hub.pace() {
        return Some(MESSENGER_ASK_TOO_FAST.to_string());
    }
    // 공지 확인 답과 같은 판정(팀장 예비분은 쓰지 않는다). 걸리면 AI를 부르지 않는다.
    if !read
        .agents
        .iter()
        .all(|agent| hub.linked.budget.try_take(*agent))
    {
        return Some(MESSENGER_ASK_CAPPED.to_string());
    }
    let Some(desk) = hub.desk() else {
        return Some(HAND_OFF_FAILED.to_string());
    };
    match desk.post_from_messenger(&read.text, source).await {
        Ok(turns) => {
            spawn_answers(hub, turns);
            None
        }
        Err(error) => {
            tracing::warn!(%error, "메신저에서 온 글을 사무실로 넘기지 못했습니다");
            Some(HAND_OFF_FAILED.to_string())
        }
    }
}

/// `/notice`: 빈 글은 안내, 그 밖은 사무실 전체 공지와 같은 길. 확인 답은 모두 끝난 뒤 메시지 하나로 보낸다.
async fn notice(hub: &Arc<Messenger>, text: &str, source: &str) -> Option<String> {
    if text.is_empty() {
        return Some(MESSENGER_NOTICE_EMPTY.to_string());
    }
    if text.chars().count() > OFFICE_TEXT_MAX {
        return Some(MESSENGER_ASK_TOO_LONG.to_string());
    }
    if !hub.pace() {
        return Some(MESSENGER_ASK_TOO_FAST.to_string());
    }
    let Some(desk) = hub.desk() else {
        return Some(HAND_OFF_FAILED.to_string());
    };
    match desk.notice_from_messenger(text, source).await {
        Ok(turns) => {
            spawn_acks(hub, turns);
            None
        }
        Err(error) => {
            tracing::warn!(%error, "메신저 공지를 사무실로 넘기지 못했습니다");
            Some(HAND_OFF_FAILED.to_string())
        }
    }
}

/// 그 응답이 끝날 때까지 기다린 글: 답함이면 답, 그 밖(실패 · 기다리다 그만둠)이면 없음.
async fn settled(hub: &Messenger, request: &str) -> Option<String> {
    let deadline = tokio::time::Instant::now() + CHAT_TOTAL_TIMEOUT * 2;
    loop {
        if hub.is_closing() || tokio::time::Instant::now() >= deadline {
            return None;
        }
        let response = hub.linked.office.response(request)?;
        if response.status.is_terminal() {
            return (response.status == ChatStatus::Complete).then_some(response.content);
        }
        tokio::time::sleep(SETTLE_TICK).await;
    }
}

/// 차례 하나의 줄 `"<표시 이름>: <글>"`. 턴이 없으면(상한) `capped`.
async fn line_of(hub: &Messenger, turn: &BridgeTurn, capped: &str) -> String {
    let text = match &turn.request {
        Some(request) => settled(hub, request)
            .await
            .unwrap_or_else(|| NO_ANSWER.to_string()),
        None => capped.to_string(),
    };
    format!("{}: {text}", display_name(turn.agent.id()))
}

/// 지목한 담당의 답을 지목한 차례로 하나씩 보낸다(담당마다 메시지 하나).
fn spawn_answers(hub: &Arc<Messenger>, turns: Vec<BridgeTurn>) {
    let hub = Arc::clone(hub);
    tokio::spawn(async move {
        for turn in &turns {
            let line = line_of(&hub, turn, NO_ANSWER).await;
            if hub.is_closing() {
                return;
            }
            hub.send_owner(&line).await;
        }
    });
}

/// 공지 확인 답이 모두 끝나면 머리 뒤에 담당마다 한 줄씩 이어 메시지 하나로 보낸다.
fn spawn_acks(hub: &Arc<Messenger>, turns: Vec<BridgeTurn>) {
    let hub = Arc::clone(hub);
    tokio::spawn(async move {
        let mut lines = vec![MESSENGER_NOTICE_ACKS.to_string()];
        for turn in &turns {
            lines.push(line_of(&hub, turn, ACK_CAPPED).await);
        }
        if !hub.is_closing() {
            hub.send_owner(&lines.join("\n")).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 공지_명령은_봇_이름_꼴도_받고_뒤_글을_뗀다() {
        assert_eq!(
            notice_body("/notice 오늘은 쉰다", None),
            Some("오늘은 쉰다")
        );
        assert_eq!(notice_body("/NOTICE", None), Some(""));
        assert_eq!(
            notice_body("/notice@my_bot  글 ", Some("my_bot")),
            Some("글")
        );
        assert_eq!(notice_body("/notice@other 글", Some("my_bot")), None);
        assert_eq!(notice_body("/noticeboard 글", None), None);
        assert_eq!(notice_body("공지 /notice", None), None);
    }
}
