//! pi-paper 테스트 묶음. 지시서 `docs/dev/modules/paper.md`의 "만드는 것"을 항목마다 확인한다.
//! 모든 테스트는 공개 API로만 본다. 시각 · 시세 · 달력은 테스트가 정해 준다.
//!
//! 2차 작업(지시서 "2차 작업" 1~14)의 테스트: `fair`(1~4 유리하게 치우친 체결 막기),
//! `flow`(6~9 실제와 다르게 끝나는 흐름 없애기), `resource`(10~12 되살리기와 자원),
//! `duty`(13 조립하는 쪽의 의무), `defaults`(5 기본값 없는 숫자), `invariant`(6 · 7 속성 테스트).
//!
//! 4차 작업(지시서 "4차 작업" 1~5)의 테스트: `round4`.

mod support;

mod accept;
mod defaults;
mod duty;
mod fair;
mod fill;
mod flow;
mod guard;
mod invariant;
mod manage;
mod resource;
mod restore;
mod round4;
mod settle;
mod timeout;
