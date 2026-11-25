//! (39차) 받기 루프: long polling → `Gatekeeper::judge` → 페어링 코드 답 · 명령 실행. 오류는 종류대로 쉬고
//! 물러서며, 토큰이 거절되면 멈춘다(계약은 `crate::messenger` 문서).

use std::sync::Arc;
use std::time::Duration;

use pi_channel::{Channel, Cursor, Error, Inbound, Judgement, OutText, Telegram};
use pi_notify::NoticeKind;
use tokio::sync::watch;

use super::book::{load_cursor, save_cursor};
use super::hub::Messenger;
use super::outbound::Hidden;
use super::status::LoopState;

/// 처음 말을 건 사람에게 보내는 답의 앞부분. 뒤에 코드가 붙는다.
pub const MESSENGER_PAIRING_REPLY: &str =
    "이 코드를 이 컴퓨터의 설정 화면이나 `price-inha setup messenger`에 넣으세요: ";

/// 토큰이 거절돼 멈췄을 때의 알림.
const UNAUTHORIZED_TITLE: &str = "메신저 토큰 거절";
const UNAUTHORIZED_BODY: &str =
    "텔레그램이 봇 토큰을 거절해 메신저 받기를 멈췄습니다. 설정 화면에서 토큰을 다시 등록하세요";

/// 한 번의 기다림이 끝난 까닭.
enum Woke {
    /// 다 잤다.
    Slept,
    /// 멈추라는 알림이 왔다.
    Stopped,
}

/// 멈춤 알림을 보며 시계로 잔다.
async fn nap(hub: &Messenger, duration: Duration, stop: &mut watch::Receiver<bool>) -> Woke {
    tokio::select! {
        () = hub.clock.sleep(duration) => Woke::Slept,
        _ = stop.wait_for(|stopped| *stopped) => Woke::Stopped,
    }
}

/// 받기 실패의 다음 할 일.
enum Next {
    /// 그만큼 쉬고 다시.
    Wait(Duration),
    /// 루프를 끝낸다.
    Quit,
}

/// 루프 몸통. 멈춤 알림이 오거나 토큰이 거절되면 끝난다.
pub(crate) async fn run(hub: Arc<Messenger>, telegram: Telegram, mut stop: watch::Receiver<bool>) {
    let mut backoff = hub.wiring.backoff_initial;
    // 봇 이름을 모르면(띄울 때) 먼저 확인한다 — 명령의 `@봇이름` 꼴을 읽는 데 쓴다.
    while hub.bot().is_none() {
        let checked = tokio::select! {
            result = telegram.check() => result,
            _ = stop.wait_for(|stopped| *stopped) => return,
        };
        match checked {
            Ok(bot) => hub.set_bot(Some(bot)),
            Err(error) => match failure(&hub, &error, &mut backoff) {
                Next::Wait(wait) => {
                    if let Woke::Stopped = nap(&hub, wait, &mut stop).await {
                        return;
                    }
                }
                Next::Quit => return,
            },
        }
    }
    let mut cursor = load_cursor(&hub.home);
    loop {
        let polled = tokio::select! {
            result = telegram.poll(cursor) => result,
            _ = stop.wait_for(|stopped| *stopped) => return,
        };
        let wait = match polled {
            Ok(polled) => {
                hub.set_state(LoopState::Polling);
                backoff = hub.wiring.backoff_initial;
                for item in &polled.items {
                    handle(&hub, &telegram, item).await;
                }
                if let Some(next) = polled.next.filter(|next| Some(*next) != cursor) {
                    remember(&hub, next);
                    cursor = Some(next);
                }
                None
            }
            Err(error) => match failure(&hub, &error, &mut backoff) {
                Next::Wait(wait) => Some(wait),
                Next::Quit => return,
            },
        };
        if let Some(wait) = wait
            && let Woke::Stopped = nap(&hub, wait, &mut stop).await
        {
            return;
        }
        if *stop.borrow() {
            return;
        }
    }
}

/// 받은 자리를 쓴다(다음 받기 전에 — 같은 글을 두 번 처리하지 않게).
fn remember(hub: &Messenger, next: Cursor) {
    if let Err(error) = save_cursor(&hub.home, next) {
        tracing::warn!(error = %error.kind(), "메신저 받은 자리를 쓰지 못했습니다");
    }
}

/// 실패 종류대로: 429는 그만큼, 토큰 거절은 멈춤(알림 한 번), 그 밖은 물러서기(두 배, 상한까지).
fn failure(hub: &Arc<Messenger>, error: &Error, backoff: &mut Duration) -> Next {
    match error {
        Error::RateLimited { retry_after_secs } => {
            Next::Wait(Duration::from_secs(*retry_after_secs))
        }
        Error::Unauthorized => {
            hub.set_state(LoopState::Unauthorized);
            tracing::warn!("텔레그램이 봇 토큰을 거절해 메신저 받기를 멈춥니다");
            hub.linked.notice_later(
                NoticeKind::Anomaly,
                UNAUTHORIZED_TITLE.to_string(),
                UNAUTHORIZED_BODY.to_string(),
                None,
            );
            Next::Quit
        }
        other => {
            hub.set_state(LoopState::Error);
            tracing::warn!(error = %other, "메신저 받기가 실패해 물러서며 다시 합니다");
            let wait = *backoff;
            *backoff = (*backoff * 2).min(hub.wiring.backoff_max);
            Next::Wait(wait.min(hub.wiring.backoff_max))
        }
    }
}

/// 받은 글 하나: 판정 → 코드 답 · 명령 답 · 무시.
async fn handle(hub: &Arc<Messenger>, telegram: &Telegram, item: &Inbound) {
    let now = hub.now();
    let judged = hub.with_gate(|gate| gate.judge(item, now));
    let reply = match judged {
        Ok(Judgement::PairingIssued { code }) => {
            format!("{MESSENGER_PAIRING_REPLY}{}", code.as_str())
        }
        Ok(Judgement::Allowed) => {
            let bot = hub.bot();
            super::commands::answer(hub, &item.text, bot.as_deref()).await
        }
        Ok(Judgement::PairingPending | Judgement::Ignored { .. }) => return,
        Err(error) => {
            tracing::warn!(%error, "메신저 글을 판정하지 못했습니다");
            return;
        }
    };
    let text = match OutText::redacted(&reply, &Hidden(Arc::clone(&hub.secrets))) {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(%error, "가리지 못해 메신저 답을 보내지 않습니다");
            return;
        }
    };
    if let Err(error) = telegram.send(&item.chat, &text).await {
        tracing::warn!(%error, "메신저 답을 보내지 못했습니다");
    }
}
