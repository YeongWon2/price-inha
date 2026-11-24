//! 31차(`docs/dev/modules/server.md` "31차 설정 화면 보강")의 Rust 쪽 시험.
//!
//! - `status`: 1번 (가) · (나)(R18-4 · C32 — 설정 상태 `GET /api/setup`에 켤 때 정한 시장과 담당마다 지금 쓰는
//!   모델 이름이 실린다, 팀이 없으면 모델 목록은 비어 있다).
//! - `doctor`: 1번 (라)(V78 · C32 — 진단 읽기 `GET /api/setup/doctor`의 모양 · 항목 순서 · 상태 값 · 시연 모드의 값,
//!   등록한 키 값이 응답 어디에도 없음, 원격은 세션 없이 401, 읽어도 바깥 주소로 요청이 나가지 않음).
//! - `sound`: 31차 고침(결정 22 — 알림음 설정을 서버가 둔다. `GET` · `POST /api/setup/sound`의 기본 켜짐 · 바꾸기 ·
//!   다시 만들어도 남음 · 참 · 거짓이 아니면 400 · 원격은 세션 없이 401 · 응답에 키 값 없음).
//!
//! 서버는 임시 보관 폴더 · `127.0.0.1:0` · 실행 파일 조립(`run_assembly`, 시연)으로 띄운다(round26의 도우미를
//! 본뜸). 바깥 주소 자리는 모두 받은 요청을 세는 로컬 가짜 서버다. 화면 동작의 시험은 vitest
//! (`tests/web/setup-extras.test.ts` · `sound-pref.test.ts`)에 있다.

mod common;
mod doctor;
mod sound;
mod status;
