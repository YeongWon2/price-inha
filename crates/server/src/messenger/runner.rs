//! (39차) 받기 루프: long polling → `Gatekeeper::judge` → 페어링 코드 답 · 명령 실행. 오류는 종류대로 쉬고
//! 물러서며, 토큰이 거절되면 멈춘다(계약은 `crate::messenger` 문서).

/// 처음 말을 건 사람에게 보내는 답의 앞부분. 뒤에 코드가 붙는다.
pub const MESSENGER_PAIRING_REPLY: &str =
    "이 코드를 이 컴퓨터의 설정 화면이나 `price-inha setup messenger`에 넣으세요: ";
