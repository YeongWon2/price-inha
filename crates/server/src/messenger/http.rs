//! (39차) 메신저 경로: 상태 읽기 · 토큰 등록 · 지우기 · 페어링 승인 · 빼기(계약은 `crate::messenger` 문서).
//!
//! 상태 읽기는 세션이 있어야 열리는 경로에, 나머지는 같은 컴퓨터에서만 열리는 경로(`GateBuilder::local_routes`)에
//! 단다(원격은 관문이 403).

/// 상태 읽기 `GET`(세션).
pub const MESSENGER_PATH: &str = "/api/messenger";
/// 텔레그램 봇 토큰 등록 `POST` `{"token"}` · 지우기 `DELETE`(같은 컴퓨터에서만).
pub const MESSENGER_TELEGRAM_PATH: &str = "/api/messenger/telegram";
/// 페어링 승인 `POST` `{"code"}`(같은 컴퓨터에서만).
pub const MESSENGER_PAIRING_APPROVE_PATH: &str = "/api/messenger/pairing/approve";
/// 허용된 사람 빼기 `POST`(같은 컴퓨터에서만).
pub const MESSENGER_PAIRING_REVOKE_PATH: &str = "/api/messenger/pairing/revoke";

/// 틀린 페어링 코드의 답(400, 남은 횟수를 싣지 않는다).
pub const MESSENGER_WRONG_CODE: &str = "코드가 맞지 않습니다";
/// 틀린 코드가 쌓여 잠겼을 때의 답(423, 풀리는 시각은 `locked_until`).
pub const MESSENGER_LOCKED: &str = "틀린 코드가 많아 잠시 승인을 받지 않습니다";
/// 이미 허용된 사람이 있을 때의 답(409).
pub const MESSENGER_OWNER_EXISTS: &str = "이미 연결된 사람이 있습니다 — 먼저 빼세요";
/// 시연 모드에서 등록하려 할 때의 답(409).
pub const MESSENGER_DEMO_REFUSED: &str = "시연 모드에서는 메신저를 쓰지 않습니다";
