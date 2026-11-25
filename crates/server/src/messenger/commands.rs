//! (39차) 허용된 사람의 명령을 실행한다: 상태 · 승인 목록 · 거절 · 전체 정지 · 도움말. 승인 · 재개 · 규칙 변경 ·
//! 실전 승격은 하지 않는다(결정 24). 계산 · 요약은 서버가 한다.

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
/// 전체 정지했을 때.
pub const MESSENGER_STOP_LATER: &str = "정지는 아직 메신저에서 할 수 없습니다";
/// 승인을 뜻하는 글에 대한 답.
pub const MESSENGER_APPROVE_ON_WEB: &str = "승인은 웹에서만 받습니다";
/// 도움말.
pub const MESSENGER_HELP: &str =
    "/status 팀 상태 · /approvals 기다리는 승인 · /reject <번호> 거절 · /help 도움말";
/// 모르는 글에 대한 도움말 한 줄.
pub const MESSENGER_UNKNOWN: &str = "모르는 명령입니다 — /help";
