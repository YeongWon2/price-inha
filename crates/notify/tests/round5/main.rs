//! notify 5차 시험(백로그 Q · 4차 검토 참고): `release_held`를 동시에 두 번 불러도 미룬 푸시는 한 번씩만 간다.
//! 공용 도우미는 `tests/suite/common.rs`를 그대로 쓴다(그 폴더는 고치지 않는다). 푸시 서비스는 로컬 가짜 서버다.

#[path = "../suite/common.rs"]
pub mod common;

mod release_twice;
