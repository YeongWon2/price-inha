//! 21차 남은 후보 — 돈 · 안전(`docs/dev/modules/server.md` "21차 남은 후보 — 돈 · 안전")의 테스트.
//!
//! 잠긴 테스트 파일을 고치지 않으려고 따로 둔 최상위 테스트 바이너리다. 서버는 임시 보관 폴더 ·
//! `127.0.0.1:0`으로 띄우고, 시세 · AI · 자료 · GitHub는 시연용 가짜이거나 로컬 가짜 서버뿐이다.
//! 실제 증권사 · AI를 부르지 않는다.
//!
//! - `widen`: 1번(실전중 템플릿의 위험을 키우는 값 바꾸기는 승인 · 냉각을 거친다, 줄이기 · 모의중은 바로).
//! - `feed_reject`: 3번 앞(증권사가 거절한 보유 종목도 이상 감지에 남는다).
//! - `answer_503`: 4번(승인 답의 오류를 버리지 않는다: 503 · 5xx).
//! - `day_once`: 5번(단계가 실패해도 day_start · day_close는 한 번만).
//! - `backup_looks`: 6번 뒤(외형 저장이 백업에 든다).
//! - 2번(최고 평가액은 저장된 뒤에 "남김") · 3번 뒤(멈춤 중 비정상 바퀴를 세지 않는다)는 저장 실패 · 바퀴 수를
//!   공개 길로 만들 수 없어 `src/trading/money_safety.rs`의 단위 시험이 본다.
//! - 6번 앞(경로 표)은 `tests/web/screen-paths.test.ts`다.

mod answer_503;
mod backup_looks;
mod common;
mod day_once;
mod feed_reject;
mod widen;
