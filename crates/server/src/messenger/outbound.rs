//! (39차) 나가는 알림: 폰 푸시로 가는 알림을 허용된 사람의 대화에도 보낸다. 가린 글(`OutText::redacted`)만 보내고,
//! 가리기에 실패하면 보내지 않는다. 보내기 실패는 팀의 일을 막지 않는다.

/// 승인 요청 알림에 붙는 거절 안내의 머리(뒤에 짧은 번호).
pub const MESSENGER_REJECT_HINT: &str = "거절: /reject ";
/// 거절 안내 뒤에 붙는 말.
pub const MESSENGER_WEB_ONLY: &str = " · 승인은 웹에서";
