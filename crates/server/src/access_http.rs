//! (7차) 접속 키 조립: 발급 · 잠금 풀기 · 접속 기록 보기.

/// `POST`: 새 접속 키를 만든다(그 컴퓨터에서만).
pub const ACCESS_KEY_PATH: &str = "/api/access/key";
/// `POST`: 막힌 기기를 푼다(그 컴퓨터에서만).
pub const ACCESS_UNLOCK_PATH: &str = "/api/access/unlock";
/// `GET`: 접속 기록과 지금 잠긴 기기(세션).
pub const ACCESS_LOG_PATH: &str = "/api/access/log";
