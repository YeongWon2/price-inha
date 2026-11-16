//! data 모듈 테스트 묶음(`tests/suite/`). 공용 도우미(`common`)가 한 번만 컴파일되도록 한 바이너리로 묶는다.
//! 로컬 가짜 서버(wiremock)만 쓴다.

mod common;

mod client;
mod dart;
mod ecos;
mod fake;
mod fred;
mod second;
mod third;
