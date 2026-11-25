//! server 41차 시험: 메신저(텔레그램)에서 팀 대화 · 전체 정지 알림(결정 26 · 24 · 25 · additions.md V87~V91 · 33차 V58 ·
//! V59). 바깥(증권사 · 텔레그램 · AI · 자료 · 푸시 서비스 · GitHub)은 로컬 가짜 서버뿐이다.
//!
//! - `bench`: 발판이 지금 구현에서 선다 — 팀을 잇고 운용 중으로 남음 · 논의를 돌리면 `office.talk`가 나옴 · 사무실 팀 방
//!   멘션에 그 담당이 답함 · 사무실 공지에 여섯 담당이 확인 답 · 지금 알림이 메신저로 감 · 최대 손실 전체 정지가 됨
//!   (41차 "발판 먼저"). 이 파일의 시험은 구현 전에도 통과하는 것이 정상이다.
//! - `flow`: 팀 논의가 허용된 대화로 "이름: 글"로 가린 채 흐르고 묶음 창 안의 글은 한 메시지 · 허용된 사람이 없으면 ·
//!   "팀 논의"를 끄면 · 조용한 시간이면 가지 않음(승인 요청은 감).
//! - `ask`: 메신저에서 `@담당`으로 묻기 — 지목한 담당만 답하고 답이 메신저와 `office.chat` 양쪽에 · 둘이면 차례로 ·
//!   이름 · id · 조사 · 없는 이름 · 지목 없음(AI 호출 0) · 하루 상한 · 길이 · 간격 · 허용 안 된 사람 · "메신저에서" 표시.
//! - `notice`: `/notice`로 저장 · 여섯 확인 · 메신저로 묶은 답 · 빈 글 안내.
//! - `commands`: 명령 다섯은 그대로 · 담당 답 글에 `/stop`이 들어 있어도 정지하지 않음.
//! - `stop_notice`: 전체 정지 알림 — 내가 정지(화면 · 메신저) · 최대 손실 각각 한 번 · 까닭 글 · 폰과 메신저(설정대로) ·
//!   조용한 시간에도 감.
//! - `settings`: 알림 설정 응답에 "팀 논의" 종류(폰 칸 없음 · 잠기지 않음 · 켜짐) · 바꾸기 · 다시 켜도 남음.
//!
//! 화면은 `tests/web/setup-notify-talk.test.ts` · `account-stop-text.test.ts` · `office-chat-via.test.ts` ·
//! `setup-messenger.test.ts`(vitest)와 `tests/e2e/notify-talk.spec.ts` · `stop.spec.ts`가 본다.

mod ask;
mod bench;
mod commands;
mod common;
mod flow;
mod notice;
mod settings;
mod stop_notice;
