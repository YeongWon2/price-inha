//! 매매 근거 읽기(C6 · V18)와 근거가 다 있는지 판정(V16).

use pi_store::{Rationale, RationaleField, Trade};

/// 쉬운 말 근거 요약(C6 · V18). AI를 쓰지 않는 고정 문장 틀이다(결정 2026-11-16 28번).
///
/// - 같은 매매 값이면 언제나 같은 글자를 준다.
/// - 근거 칸(가설 · 계기 · 들어간 값 · 손절값 · 목표값 · 기대값), 담당별 점수와 그 까닭, 합의 점수,
///   쓴 자료 이름과 알게 된 시각이 들어간다. 시각은 한국 시간 `YYYY-MM-DD HH:MM`이다.
/// - 빈 칸은 "비어 있음"처럼 쉬운 말로 적는다(`None` · `Some(` 같은 개발 글자가 새지 않는다).
pub fn plain_summary(trade: &Trade) -> String {
    todo!()
}

/// 비어 있는 근거 칸들(V16). 칸이 `None`이거나 글자 칸이 비었거나 공백뿐이면 빈 것이다.
/// [`RationaleField`] 선언 순서(가설 · 계기 · 들어간 값 · 손절값 · 목표값 · 기대값)로 준다.
pub fn missing_fields(rationale: &Rationale) -> Vec<RationaleField> {
    todo!()
}
