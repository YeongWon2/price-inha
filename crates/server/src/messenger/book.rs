//! (39차) 페어링 장부 · 받은 자리 파일. 홈 폴더에 0600 · 원자적 쓰기로 둔다(저장소 판을 올리지 않는다).
//! 장부는 `pi_channel::PairingBook`을 구현한다. 토큰은 여기 두지 않는다.

/// 페어링 장부 파일(허용된 사람 · 기다리는 요청 · 코드를 준 기록 · 틀린 횟수 · 잠금).
pub const MESSENGER_BOOK_FILE: &str = "messenger-pairing.json";
/// 받은 자리(`pi_channel::Cursor`) 파일. 다시 켜도 이어 받는다.
pub const MESSENGER_CURSOR_FILE: &str = "messenger-cursor.json";
