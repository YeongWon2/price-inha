//! notify 모듈 테스트 묶음(`tests/suite/`). 공용 도우미(`common`)가 한 번만 컴파일되도록 한 바이너리로 묶는다.
//! 푸시 서비스는 로컬 가짜 서버(127.0.0.1)다. 바깥 서비스 주소를 쓰지 않는다.

mod common;

mod crypto;
mod kind;
mod link;
mod notices;
mod phone;
mod quiet;
mod quiet_send;
mod redact;
mod send;
mod send_fail;
mod ssrf;
mod subs;
mod vapid;
