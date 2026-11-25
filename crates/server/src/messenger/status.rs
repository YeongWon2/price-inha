//! (39차) 메신저 상태 · 이벤트 글자.

/// 상태: 꺼짐(토큰 없음 · 시연 모드).
pub const MESSENGER_STATE_OFF: &str = "off";
/// 상태: 받는 중.
pub const MESSENGER_STATE_POLLING: &str = "polling";
/// 상태: 토큰이 거절돼 멈춤.
pub const MESSENGER_STATE_UNAUTHORIZED: &str = "unauthorized";
/// 상태: 오류(물러서며 다시 하는 중).
pub const MESSENGER_STATE_ERROR: &str = "error";

/// 이벤트 종류.
pub const KIND_MESSENGER: &str = "messenger";
/// 이벤트 `action`: 토큰을 저장했다(새로 · 바꿈).
pub const MESSENGER_ACTION_TOKEN_SET: &str = "token_set";
/// 이벤트 `action`: 토큰을 지웠다.
pub const MESSENGER_ACTION_TOKEN_REMOVED: &str = "token_removed";
/// 이벤트 `action`: 페어링을 승인했다.
pub const MESSENGER_ACTION_PAIRED: &str = "paired";
/// 이벤트 `action`: 허용된 사람을 뺐다.
pub const MESSENGER_ACTION_REVOKED: &str = "revoked";
