//! server 51차(차수 표 21번) 시험: 하루 AI 잔량(C22 · M13 · N14 · E52 · V64)과 논의 진행 단계(C21 · E51 · V63).
//!
//! - `scaffold`: 발판이 지금 구현에서 서는가(서버가 뜨고 시연 논의가 돌아 `office.talk`가 나온다).
//! - `budget`: `GET /api/team/budget` 모양 · 상한 없음 · 예비분 · 모드(정리 · 팀장만) · 담당별 사용 · 다시 켜도 그날
//!   것은 남고 다음 날 새로 센다 · `team.ai_used` 이벤트에 `members` · `mode`가 실린다 · 공지 확인 답도 담당별로 센다.
//! - `progress`: 논의가 도는 동안 `office.progress` · `GET /api/office`의 `progress` 칸 · 끝나면 마지막 상태를 한 번
//!   내고 비운다 · 가짜 AI가 한 번 실패하면 다시 시도 1/3이 보이고 논의는 이어진다.

mod budget;
mod common;
mod progress;
mod scaffold;
