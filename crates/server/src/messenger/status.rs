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

/// 받기 루프의 지금 상태(토큰 · 시연 여부는 따로 본다).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopState {
    /// 돌지 않는다.
    Off,
    /// 받는 중.
    Polling,
    /// 토큰이 거절돼 멈췄다.
    Unauthorized,
    /// 오류(물러서며 다시 하는 중 · 주소를 읽을 수 없음).
    Error,
}

impl LoopState {
    /// 상태 글자.
    pub(crate) fn word(self) -> &'static str {
        match self {
            LoopState::Off => MESSENGER_STATE_OFF,
            LoopState::Polling => MESSENGER_STATE_POLLING,
            LoopState::Unauthorized => MESSENGER_STATE_UNAUTHORIZED,
            LoopState::Error => MESSENGER_STATE_ERROR,
        }
    }
}

/// 보낸 사람 ID의 뒤 4자리(이벤트에 싣는 것은 이것뿐이다).
pub(crate) fn sender_tail(sender: &str) -> String {
    let chars: Vec<char> = sender.chars().collect();
    chars[chars.len().saturating_sub(4)..].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 뒤_4자리만_남긴다() {
        assert_eq!(sender_tail("82345678"), "5678");
        assert_eq!(sender_tail("12"), "12");
    }
}
