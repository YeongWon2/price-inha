//! (39차) 나가는 알림: 폰 푸시로 가는 알림을 허용된 사람의 대화에도 보낸다. 가린 글(`OutText::redacted`)만 보내고,
//! 가리기에 실패하면 보내지 않는다. 보내기 실패는 팀의 일을 막지 않는다.

use std::sync::Arc;

use pi_channel::{Channel, ChatId, OutText, RedactError, Redactor};
use pi_notify::NoticeKind;
use pi_secrets::SecretStore;

use super::hub::Messenger;
use super::status::LoopState;

/// 승인 요청 알림에 붙는 거절 안내의 머리(뒤에 짧은 번호).
pub const MESSENGER_REJECT_HINT: &str = "거절: /reject ";
/// 거절 안내 뒤에 붙는 말.
pub const MESSENGER_WEB_ONLY: &str = " · 승인은 웹에서";

/// 메신저로 나가는 글의 가리개: server의 가리개(비밀 보관소)를 `pi_channel::Redactor`로 감싼다.
pub(crate) struct Hidden(pub(crate) Arc<SecretStore>);

impl Redactor for Hidden {
    fn redact(&self, text: &str) -> Result<String, RedactError> {
        // 비밀 보관소의 가리기는 실패를 돌려주지 않는다(가릴 것이 없으면 그대로). 실패 길은 `OutText::redacted`가
        // 맡는다 — 이 가리개가 실패를 돌려주면 그 글은 보내지 않는다.
        Ok(self.0.redact(text))
    }
}

/// 나갈 글: 제목 · 본문, 승인 요청이면 거절 안내.
fn compose(kind: NoticeKind, title: &str, body: &str, short: Option<&str>) -> String {
    let mut text = format!("{title}\n{body}");
    if kind == NoticeKind::ApprovalRequest
        && let Some(short) = short
    {
        text.push_str(&format!(
            "\n{MESSENGER_REJECT_HINT}{short}{MESSENGER_WEB_ONLY}"
        ));
    }
    text
}

impl Messenger {
    /// 알림 하나를 허용된 사람의 대화로 뒤에서 보낸다(부르는 쪽을 기다리게 하지 않는다).
    /// 시연 · 토큰 없음 · 토큰 거절 · 허용된 사람 없음이면 보내지 않는다. `short`는 승인 요청의 짧은 번호.
    pub(crate) fn forward(
        self: &Arc<Self>,
        kind: NoticeKind,
        title: &str,
        body: &str,
        short: Option<String>,
    ) {
        if self.demo || self.state() == LoopState::Unauthorized {
            return;
        }
        let raw = compose(kind, title, body, short.as_deref());
        let me = Arc::clone(self);
        tokio::spawn(async move { me.send_owner(&raw).await });
    }

    /// 허용된 사람의 대화로 글 하나를 보낸다(가린 글만, 실패하면 물러서며 정한 횟수까지). 허용된 사람이 없으면
    /// 보내지 않는다. (41차) 논의 글 · 담당 답 · 공지 확인 답도 이 길로 나간다 — 나가는 글은 명령으로 읽지 않는다.
    pub(crate) async fn send_owner(&self, raw: &str) {
        if self.demo || self.state() == LoopState::Unauthorized {
            return;
        }
        let owner = match self.with_gate(|gate| gate.allowed()) {
            Ok(Some(owner)) => owner,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(%error, "메신저 장부를 읽지 못해 알림을 보내지 않습니다");
                return;
            }
        };
        let Some(token) = self.token() else {
            return;
        };
        let text = match OutText::redacted(raw, &Hidden(Arc::clone(&self.secrets))) {
            Ok(text) => text,
            Err(error) => {
                tracing::warn!(%error, "가리지 못해 메신저 알림을 보내지 않습니다");
                return;
            }
        };
        let Some(telegram) = self.telegram(&token) else {
            return;
        };
        drop(token);
        // 텔레그램 1:1 대화의 대화 번호는 보낸 사람 번호와 같다(1:1만 허용한다).
        let chat = ChatId(owner.0);
        let attempts = self.wiring.send_attempts.max(1);
        let mut wait = self.wiring.backoff_initial;
        for attempt in 1..=attempts {
            match telegram.send(&chat, &text).await {
                Ok(_) => return,
                Err(error) if attempt == attempts => {
                    tracing::warn!(%error, attempts, "메신저 알림을 보내지 못해 버립니다");
                }
                Err(error) => {
                    tracing::info!(%error, attempt, "메신저 알림 보내기가 실패해 다시 보냅니다");
                    self.clock.sleep(wait).await;
                    wait = (wait * 2).min(self.wiring.backoff_max);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 승인_요청에만_거절_안내가_붙는다() {
        let text = compose(NoticeKind::ApprovalRequest, "승인", "요약", Some("abc123"));
        assert!(text.ends_with("거절: /reject abc123 · 승인은 웹에서"));
        let other = compose(NoticeKind::Anomaly, "이상", "본문", Some("abc123"));
        assert_eq!(other, "이상\n본문");
    }
}
