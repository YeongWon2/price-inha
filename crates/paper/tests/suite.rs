//! pi-paper 테스트 묶음. 지시서 `docs/dev/modules/paper.md`의 "만드는 것"을 항목마다 확인한다.
//! 모든 테스트는 공개 API로만 본다. 시각 · 시세 · 달력은 테스트가 정해 준다.

mod support;

mod accept;
mod defaults;
mod fill;
mod guard;
mod invariant;
mod manage;
mod restore;
mod settle;
mod timeout;
