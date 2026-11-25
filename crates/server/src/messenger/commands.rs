//! (39차) 허용된 사람의 명령을 실행한다: 상태 · 승인 목록 · 거절 · 전체 정지 · 도움말. 승인 · 재개 · 규칙 변경 ·
//! 실전 승격은 하지 않는다(결정 24). 계산 · 요약은 서버가 한다.

use pi_approval::{ApprovalView, Subject};
use pi_channel::{Command, parse_command};
use pi_store::{ApprovalStatus, TeamState};
use rust_decimal::Decimal;

use super::hub::Messenger;

/// 짧은 번호: 승인 번호의 앞 이만큼.
pub const MESSENGER_SHORT_ID_LEN: usize = 6;
/// `/status` 덩이의 기다리는 승인 수 줄 머리(뒤에 공백과 수).
pub const MESSENGER_STATUS_PENDING: &str = "기다리는 승인";
/// `/approvals` 끝 줄의 머리(뒤에 링크 또는 `price-inha open`).
pub const MESSENGER_APPROVE_ON_WEB_LINK: &str = "승인은 웹에서: ";
/// 기다리는 승인이 없을 때.
pub const MESSENGER_NO_APPROVALS: &str = "기다리는 승인이 없습니다";
/// 거절했을 때.
pub const MESSENGER_REJECTED: &str = "거절했습니다";
/// 그 짧은 번호의 기다리는 승인이 없을 때.
pub const MESSENGER_NO_SUCH_APPROVAL: &str = "그 번호의 기다리는 승인이 없습니다";
/// 이미 끝난 승인일 때.
pub const MESSENGER_APPROVAL_DONE: &str = "이미 끝난 승인입니다";
/// (40차 3번) `/stop`으로 전체 정지했을 때.
pub const MESSENGER_STOPPED: &str = "전체 정지했습니다. 다시 시작은 웹에서만 합니다";
/// (40차 3번) `/stop`을 받았는데 이미 전체 정지일 때.
pub const MESSENGER_ALREADY_STOPPED: &str = "이미 전체 정지입니다. 다시 시작은 웹에서만 합니다";
/// 승인을 뜻하는 글에 대한 답.
pub const MESSENGER_APPROVE_ON_WEB: &str = "승인은 웹에서만 받습니다";
/// 도움말. (41차 3 · 4번) `/notice`와 `@담당` 안내([`MESSENGER_ASK_HINT`]와 같은 글)가 든다.
pub const MESSENGER_HELP: &str = "/status 팀 상태 · /approvals 기다리는 승인 · /reject <번호> 거절 · /stop 전체 정지 · /notice <글> 전체 공지 · /help 도움말 · 담당에게 물으려면 @전략 처럼 이름을 붙여 보내세요";
/// 모르는 글에 대한 도움말 한 줄.
pub const MESSENGER_UNKNOWN: &str = "모르는 명령입니다 — /help";
/// (41차 3번) 담당에게 묻는 법 안내. 지목이 없는 글의 답([`MESSENGER_UNKNOWN`] 다음 줄)과 도움말에 든다.
pub const MESSENGER_ASK_HINT: &str = "담당에게 물으려면 @전략 처럼 이름을 붙여 보내세요";
/// (41차 3번) 묻는 글이 사무실 보내기 간격(`crate::OFFICE_SEND_INTERVAL`) 안에 또 왔을 때.
pub const MESSENGER_ASK_TOO_FAST: &str = "너무 자주 보냈습니다 — 잠시 뒤에 다시 보내 주세요";
/// (41차 3번) 묻는 글이 사무실 글 길이 상한(`crate::OFFICE_TEXT_MAX`자)을 넘었을 때.
pub const MESSENGER_ASK_TOO_LONG: &str = "글이 너무 깁니다 — 500자 안으로 보내 주세요";
/// (41차 3번) 하루 AI 호출 상한에 걸려 담당이 답하지 못할 때(AI를 부르지 않는다).
pub const MESSENGER_ASK_CAPPED: &str = "오늘 AI 호출 상한에 닿아 담당이 답할 수 없습니다";
/// (41차 4번) `/notice` 뒤에 글이 없을 때(공지를 만들지 않는다).
pub const MESSENGER_NOTICE_EMPTY: &str = "공지할 글을 /notice 뒤에 적어 주세요";
/// (41차 4번) 공지 확인 답들을 묶은 메시지의 머리(뒤에 담당마다 `"<표시 이름>: <확인 답>"` 줄).
pub const MESSENGER_NOTICE_ACKS: &str = "공지를 남겼습니다. 담당들의 확인:";
/// (41차 3번) 메신저에서 온 사무실 팀 방 글 이벤트(`crate::KIND_OFFICE_MESSAGE`)의 `"via"` 값.
pub const MESSENGER_VIA: &str = "messenger";

/// 승인 웹 길(메신저 글에는 바깥 주소를 만들어 넣지 않는다 — 이 컴퓨터에서 여는 명령).
const OPEN_HINT: &str = "price-inha open";
/// 계산이 실패했을 때.
const READ_FAILED: &str = "지금 상태를 읽지 못했습니다 — 잠시 뒤에 다시 해 주세요";

/// 허용된 사람의 글 하나에 대한 답(가리기 전 글). 승인 · 재개 · 규칙 변경 · 실전 승격은 하지 않는다.
pub(crate) async fn answer(hub: &Messenger, text: &str, bot: Option<&str>) -> String {
    match parse_command(text, bot) {
        Command::Status => status(hub).await.unwrap_or_else(|| READ_FAILED.to_string()),
        Command::Approvals => approvals(hub).unwrap_or_else(|| READ_FAILED.to_string()),
        Command::Reject { id } => reject(hub, &id).await,
        Command::Stop => stop(hub),
        Command::Help => MESSENGER_HELP.to_string(),
        Command::ApproveNotHere => MESSENGER_APPROVE_ON_WEB.to_string(),
        Command::Unknown(_) => MESSENGER_UNKNOWN.to_string(),
    }
}

/// 팀 상태 글(계좌 화면과 같은 글).
fn team_text(state: TeamState) -> &'static str {
    match state {
        TeamState::Operating => "운용 중",
        TeamState::Resting => "쉼",
        TeamState::DailyLimitHalt => "하루 한도 멈춤",
        TeamState::AnomalyHalt => "이상 감지 멈춤",
        TeamState::FullStop => "전체 정지",
    }
}

/// 손익 글(원, 부호 붙임).
fn pnl_text(pnl: Decimal) -> String {
    let rounded = pnl.round();
    if rounded.is_sign_negative() && !rounded.is_zero() {
        format!("{rounded}원")
    } else {
        format!("+{}원", rounded.abs())
    }
}

async fn status(hub: &Messenger) -> Option<String> {
    let team = hub.store.team_state().ok()?;
    let pnl = hub.trading.day_pnl().await.ok()?;
    let held = hub.trading.holdings_count().await.ok()?;
    let pending = hub.trading.pending_approvals().ok()?.len();
    Some(format!(
        "팀 상태: {}\n오늘 손익: {}\n보유 {held}\n{MESSENGER_STATUS_PENDING} {pending}",
        team_text(team),
        pnl_text(pnl)
    ))
}

/// (40차 3번 · 결정 25) `/stop`: [`crate::TEAM_STOP_PATH`]와 같은 함수로 전체 정지한다. 다시 시작은 웹에서만.
fn stop(hub: &Messenger) -> String {
    match hub.trading.owner_stop() {
        Ok(Some(_)) => MESSENGER_STOPPED.to_string(),
        Ok(None) => MESSENGER_ALREADY_STOPPED.to_string(),
        Err(error) => {
            tracing::warn!(%error, "메신저 /stop으로 전체 정지하지 못했습니다");
            READ_FAILED.to_string()
        }
    }
}

/// 짧은 번호.
fn short_of(number: &str) -> String {
    number.chars().take(MESSENGER_SHORT_ID_LEN).collect()
}

/// 한 줄 요약: 승인 요청 알림의 요약과 같다(주문이면 종목 번호가 앞에 든다).
fn summary_of(view: &ApprovalView) -> String {
    let reason = view.reason.replace('\n', " ");
    match &view.subject {
        Subject::Order(order) => format!("{} · {reason}", order.symbol),
        _ => reason,
    }
}

fn approvals(hub: &Messenger) -> Option<String> {
    let pending = hub.trading.pending_approvals().ok()?;
    if pending.is_empty() {
        return Some(MESSENGER_NO_APPROVALS.to_string());
    }
    let mut lines: Vec<String> = pending
        .iter()
        .map(|(view, _)| format!("{} {}", short_of(view.number.as_str()), summary_of(view)))
        .collect();
    lines.push(format!("{MESSENGER_APPROVE_ON_WEB_LINK}{OPEN_HINT}"));
    Some(lines.join("\n"))
}

/// 짧은 번호가 가리키는 승인 번호들(그 결과 상태의 것만).
fn numbers_with(hub: &Messenger, status: ApprovalStatus, short: &str) -> Vec<String> {
    hub.store
        .approvals_in_status(status)
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            row.summary
                .get("number")
                .and_then(serde_json::Value::as_str)
        })
        .filter(|number| short_of(number) == short)
        .map(str::to_string)
        .collect()
}

/// 지금 승인 길의 거절과 같은 일. 기다리는 것이 하나일 때만 거절한다.
async fn reject(hub: &Messenger, short: &str) -> String {
    if short.chars().count() != MESSENGER_SHORT_ID_LEN {
        return MESSENGER_NO_SUCH_APPROVAL.to_string();
    }
    let Ok(pending) = hub.trading.pending_approvals() else {
        return READ_FAILED.to_string();
    };
    let matching: Vec<&ApprovalView> = pending
        .iter()
        .map(|(view, _)| view)
        .filter(|view| short_of(view.number.as_str()) == short)
        .collect();
    let [view] = matching.as_slice() else {
        let done = [
            ApprovalStatus::Approved,
            ApprovalStatus::Rejected,
            ApprovalStatus::NoAnswer,
            ApprovalStatus::Expired,
        ];
        let finished = matching.is_empty()
            && done
                .into_iter()
                .any(|status| !numbers_with(hub, status, short).is_empty());
        return if finished {
            MESSENGER_APPROVAL_DONE
        } else {
            MESSENGER_NO_SUCH_APPROVAL
        }
        .to_string();
    };
    let number = view.number.as_str();
    match hub
        .trading
        .answer(number, view.fingerprint.as_str(), "reject")
        .await
    {
        Ok(answered) if answered.reason == Some("already_resolved") => {
            MESSENGER_APPROVAL_DONE.to_string()
        }
        Ok(_) => {
            tracing::info!(short, "메신저(허용된 사람)가 승인 요청을 거절했습니다");
            MESSENGER_REJECTED.to_string()
        }
        Err(error) => {
            tracing::warn!(%error, "메신저의 거절을 처리하지 못했습니다");
            READ_FAILED.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 손익은_부호를_붙여_원으로_보인다() {
        assert_eq!(pnl_text(Decimal::new(12345, 1)), "+1234원");
        assert_eq!(pnl_text(Decimal::new(-500, 0)), "-500원");
        assert_eq!(pnl_text(Decimal::ZERO), "+0원");
    }

    #[test]
    fn 짧은_번호는_앞_여섯_자다() {
        assert_eq!(short_of("abcdef123"), "abcdef");
    }
}
