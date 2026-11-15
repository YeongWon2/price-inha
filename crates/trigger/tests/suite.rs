//! pi-trigger 테스트 묶음. 지시서 `docs/dev/modules/trigger.md`의 "만드는 것" 1~5를 항목마다 확인한다.
//! 모든 테스트는 공개 API로만 본다. 시각 · 시세 · 달력은 테스트가 정해 주고, 실제로 잠들지 않는다.
//!
//! - `schedule`: 1 일정 시각 · 유예 · 오류 지연 표(순수 계산)
//! - `condition`: 2 시세 조건과 다시 걸기(순수 계산)
//! - `queue`: 5 깨우기 대기열(대상마다 20 + 20, 중복 · 교체)
//! - `store`: 3 영수증 `(작업, 예정 시각)`과 실행 중은 하나
//! - `engine`: 1 · 3 · 5 tick 순서, 묶어 깨우기, 설정 확인
//! - `restart`: 3 다시 켠 뒤(E16), 놓친 회차와 장 밖(E18)
//! - `price`: 2 시세 조건 · 다시 걸기 · 늦은 시세(N6)
//! - `failure`: 4 물러서기와 10번이면 끄기
//! - `timer`: 60초 상한 타이머, 시계가 뒤로 감 · 절전에서 깸, 되풀이
//!
//! 2차(1차 검토 되돌림):
//! - `unit_write`: 2차 1 상태 옮기기 + 영수증 열기를 한 단위로(포트)
//! - `inflight`: 2차 1 · 2 중간에 실패하는 저장소, 깨우기를 띄워 두고 tick 잠금을 놓기(N6 계속)
//! - `revive`: 2차 3 다시 켤 때 영수증 없는 옮겨진 회차 되살리기
//!
//! 3차(2차 검토의 참고):
//! - `round3`: 3차 1~6 패닉한 깨우기 · 기다리지 않는 가져가기와 run의 비우기 · 중간에 빠진 tick의 되살릴 회차 ·
//!   실행 중인 작업의 대기 계산 · 되살린 회차의 `pending_slot` · 시세를 받는 동안 쓰기 잠금

mod support;

mod condition;
mod engine;
mod failure;
mod inflight;
mod price;
mod queue;
mod restart;
mod revive;
mod round3;
mod schedule;
mod store;
mod timer;
mod unit_write;
