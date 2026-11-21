//! 9차(`docs/dev/modules/server.md` "9차 작업": 홈서버 · 미니 PC · VPN)의 테스트.
//!
//! 잠긴 테스트 파일을 고치지 않으려고 따로 둔 최상위 테스트 바이너리다. 실제 `launchctl` · `systemctl` ·
//! `loginctl` · `schtasks`를 부르지 않는다(가짜 [`price_inha::CommandRunner`]와 임시 폴더).
//! `live` · `logs`와 `binary`의 뒤쪽 시험은 "9차 보강 (검토 되돌림)" 1~3번이다.
//! 실제 증권사 · AI · GitHub 주소를 쓰지 않는다. 시험의 키 값은 모두 가짜 글자다.

mod access;
mod binary;
mod command;
mod common;
mod doctor;
mod live;
mod logs;
mod restore;
mod service;
