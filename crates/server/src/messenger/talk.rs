//! (41차 1 · 2번) 팀 논의 글(`office.talk`)을 허용된 사람의 대화로 흘려보낸다: 줄 하나는 `"<표시 이름>: <글>"`
//! (종목 논의면 앞에 `"<종목> · "`), 묶음 창([`super::MessengerWiring::talk_batch`], 벽시계) 안의 글은 메시지 하나로.
//! "팀 논의"를 껐거나 조용한 시간이거나 허용된 사람이 없을 때 나온 글은 버린다(나중에 보내지 않는다).

use std::sync::{Arc, MutexGuard, PoisonError};

use pi_channel::{TELEGRAM_TEXT_MAX, split_text};
use serde_json::Value;

use super::hub::Messenger;
use super::status::LoopState;

/// 하루 판단 글의 주제(종목이 아니다 — 앞에 붙이지 않는다).
const DAY_TOPIC: &str = "day";

/// 담당 id의 표시 이름([`crate::OFFICE_ROSTER`], 없으면 id 그대로).
pub(super) fn display_name(id: &str) -> &str {
    crate::OFFICE_ROSTER
        .iter()
        .find(|(known, _)| *known == id)
        .map_or(id, |(_, name)| name)
}

/// `office.talk` 내용 하나의 메신저 줄.
fn talk_line(talk: &Value) -> Option<String> {
    let agent = talk["agent"].as_str()?;
    let text = talk["text"].as_str()?;
    let topic = talk["topic"].as_str().unwrap_or_default();
    let said = format!("{}: {text}", display_name(agent));
    Some(if topic.is_empty() || topic == DAY_TOPIC {
        said
    } else {
        format!("{topic} · {said}")
    })
}

impl Messenger {
    fn lines(&self) -> MutexGuard<'_, Option<Vec<String>>> {
        self.talk_lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// 사무실로 나간 논의 글 하나를 받는다(`office.talk`가 나가는 한곳이 부른다). 보낼 수 있을 때만 묶음에 넣고,
    /// 묶음이 없으면 새로 열어 창이 닫힐 때 보낸다.
    pub(crate) fn on_talk(self: &Arc<Self>, talk: &Value) {
        if self.demo || self.state() == LoopState::Unauthorized || self.token().is_none() {
            return;
        }
        if !self.linked.notify.talk_now(self.linked.now()) {
            return;
        }
        if !matches!(self.with_gate(|gate| gate.allowed()), Ok(Some(_))) {
            return;
        }
        let Some(line) = talk_line(talk) else {
            return;
        };
        let mut open = self.lines();
        if let Some(lines) = open.as_mut() {
            lines.push(line);
            return;
        }
        *open = Some(vec![line]);
        drop(open);
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            self.lines().take();
            return;
        };
        let me = Arc::clone(self);
        runtime.spawn(async move {
            tokio::time::sleep(me.wiring.talk_batch).await;
            let lines = me.lines().take().unwrap_or_default();
            if lines.is_empty() {
                return;
            }
            for piece in split_text(&lines.join("\n"), TELEGRAM_TEXT_MAX) {
                me.send_owner(&piece).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 줄은_이름과_글이고_종목_논의면_종목이_앞에_붙는다() {
        let talk = json!({"agent": "strategy", "text": "들어갈 만하다", "topic": "000660"});
        assert_eq!(
            talk_line(&talk).as_deref(),
            Some("000660 · 전략: 들어갈 만하다")
        );
        let day = json!({"agent": "leader", "text": "오늘은 쉰다", "topic": "day"});
        assert_eq!(talk_line(&day).as_deref(), Some("팀장: 오늘은 쉰다"));
        assert_eq!(display_name("nobody"), "nobody");
    }
}
