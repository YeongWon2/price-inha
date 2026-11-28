//! (48차, 보안 검토 5번 · 결정 36 · E56) 시연에서 확인 없이 받아 둔 키의 표시와, 시연이 아닌 실행의 첫 확인.
//!
//! 시연(`run --demo` · `price-inha demo`)의 키 등록은 바깥에 닿지 않고 받아 둔다(47차). 그 키는 확인을 거치지
//! 않았으므로, 받아 둔 칸 이름만(값 · 길이 · 일부 없이) 보관 폴더의 [`UNVERIFIED_KEYS_FILE`](0600, 원자적 쓰기)에
//! 적는다. 시연이 아닌 실행은 켜질 때 표시된 칸마다 지금의 키 확인([`crate::KeyVerifier`])을 돌리고, 확인되기
//! 전에는 그 키로 바깥(시세 · AI · 자료 · 뉴스 · 백업)에 묻지 않는다.

/// 보관 폴더의 표시 파일 이름: `{"keys": [{"item", "state", "announced"}]}`(값은 적지 않는다).
pub const UNVERIFIED_KEYS_FILE: &str = "unverified-keys.json";
/// 표시 상태: 아직 확인하지 못했다(확인 대기 — 쓰지 않고 다음 기회에 다시 확인한다).
pub const UNVERIFIED_PENDING: &str = "pending";
/// 표시 상태: 바깥이 거절해 그 키를 지웠다(다시 등록할 때까지 안내한다).
pub const UNVERIFIED_REMOVED: &str = "removed";
/// 표시 상태: 바깥이 거절했으나 비밀 보관소에 그 칸을 지우는 길이 없어 남긴 채 쓰지 않는다.
pub const UNVERIFIED_HELD: &str = "held";
