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

mod support;

mod condition;
mod engine;
mod failure;
mod price;
mod queue;
mod restart;
mod schedule;
mod store;
mod timer;
