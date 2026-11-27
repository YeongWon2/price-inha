//! server 65차 시험: 운영 다듬기 · 새 판 알리기 · 같은 집 다른 기기 Ollama(결정 34).
//!
//! - `bench`: 발판이 지금 구현에서 서는가.
//! - `service_text`: systemd `$` · `%` 이스케이프, Windows 인자 끝 `\` 따옴표.
//! - `service_notes`: 환경 변수로 준 `--bind` · `--port` 안내, macOS에서 계속 죽는 서비스 알림.
//! - `log_file`: 기록 파일 돌려 쓰기(크기 상한).
//! - `update`: 새 판 알리기만(내려받지 않음, 시연 · 시험은 바깥에 묻지 않음).
//! - `ollama_lan`: 사설망 Ollama는 켜야만, 밖으로 보낸다는 줄.
//!
//! 실제 증권사 · AI · GitHub · 다른 기기를 부르지 않는다(로컬 가짜 서버 · 가짜 바깥 명령 실행기).

mod bench;
mod common;
mod log_file;
mod ollama_lan;
mod service_notes;
mod service_text;
mod update;
