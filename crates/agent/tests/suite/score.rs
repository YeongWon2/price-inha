//! 점수 JSON과 합의 점수(결정 15번, E27).

use pi_agent::{SCORE_MAX, SCORE_MIN, consensus, exceeds_threshold, parse_score};
use pi_store::RoleScore;

fn scored(role: &str, score: i32) -> RoleScore {
    RoleScore {
        role: role.into(),
        score,
        reason: "근거".into(),
    }
}

// 검증: V3 결정 15 (점수 JSON 읽기)
#[test]
fn 점수_답을_읽는다() {
    assert_eq!((SCORE_MIN, SCORE_MAX), (0, 100));
    let text = "제 점수는\n```json\n{\"score\": 72, \"reason\": \"  실적이 좋다 \"}\n```";
    assert_eq!(parse_score(text), Some((72, "실적이 좋다".into())));
    assert_eq!(
        parse_score("{\"score\": 0, \"reason\": \"나쁘다\"}"),
        Some((0, "나쁘다".into()))
    );
    assert_eq!(
        parse_score("{\"score\": 100, \"reason\": \"최고\"}"),
        Some((100, "최고".into()))
    );
}

// 검증: E6 결정 15 (점수 JSON이 깨진 경우를 표로)
#[test]
fn 틀린_점수_답은_표대로_거부한다() {
    let cases = [
        ("객체 없음", "점수는 70입니다"),
        ("범위 아래", "{\"score\": -1, \"reason\": \"x\"}"),
        ("범위 위", "{\"score\": 101, \"reason\": \"x\"}"),
        ("글자 점수", "{\"score\": \"70\", \"reason\": \"x\"}"),
        ("소수 점수", "{\"score\": 70.5, \"reason\": \"x\"}"),
        ("근거 없음", "{\"score\": 70}"),
        ("근거 공백", "{\"score\": 70, \"reason\": \"  \"}"),
        ("근거가 글이 아님", "{\"score\": 70, \"reason\": 3}"),
        ("깨진 JSON", "{\"score\": 70, \"reason\": \"x\""),
    ];
    for (why, text) in cases {
        assert_eq!(parse_score(text), None, "{why}");
    }
}

// 검증: E27 (합의 점수 = 담당 점수의 합)
#[test]
fn 합의_점수는_담당_점수를_더한_값이다() {
    assert_eq!(
        consensus(&[scored("scout", 70), scored("risk", 60)]),
        Some(130)
    );
    assert_eq!(consensus(&[scored("scout", 0)]), Some(0));
    assert_eq!(consensus(&[]), None);
}

// 검증: E27 (기준을 넘어야 한다, 같으면 못 넘음)
#[test]
fn 기준을_넘어야_통과한다() {
    assert!(exceeds_threshold(151, 150));
    assert!(!exceeds_threshold(150, 150));
    assert!(!exceeds_threshold(149, 150));
    assert!(exceeds_threshold(0, -1));
}
