//! 담당 점수와 합의 점수(만드는 것 3-1 · 3-2, 결정 15번). 원본에 없는 우리 것이다.
//!
//! 점수 턴의 답은 `{"score": <정수>, "reason": "<근거>"}` 객체 하나다(앞뒤 산문 · 코드펜스 허용).

use pi_store::RoleScore;

/// 담당 점수의 가장 작은 값.
pub const SCORE_MIN: i32 = 0;
/// 담당 점수의 가장 큰 값.
pub const SCORE_MAX: i32 = 100;

/// 점수 턴의 답을 읽는다. 다음이면 `None`(그 담당은 점수를 내지 못한 것이다):
/// `score` 칸이 있는 객체가 없다, `score`가 정수가 아니거나 [`SCORE_MIN`]..=[`SCORE_MAX`] 밖이다,
/// `reason`이 없거나 글이 아니거나 공백뿐이다.
/// 되면 `(점수, 앞뒤 공백을 뗀 근거)`.
pub fn parse_score(text: &str) -> Option<(i32, String)> {
    let _ = text;
    todo!()
}

/// 합의 점수: 담당 점수를 모두 더한 값("담당들이 매긴 점수를 합쳐", spec 조건표).
/// 점수가 하나도 없으면 `None`(합의 불가).
pub fn consensus(scores: &[RoleScore]) -> Option<i32> {
    let _ = scores;
    todo!()
}

/// 합의 점수가 템플릿 기준을 넘는가. 넘어야(`consensus > required`) 참이다.
/// 같으면 넘지 못한 것이다(E27, 보수적으로 읽음).
pub fn exceeds_threshold(consensus: i32, required: i32) -> bool {
    let _ = (consensus, required);
    todo!()
}
