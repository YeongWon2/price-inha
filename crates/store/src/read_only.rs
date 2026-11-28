//! (23차, 백로그 U 2번) 있는 저장소 파일을 **읽기만** 하는 손잡이. 진단(`price-inha doctor`)처럼 서버를 띄우지 않는
//! 명령이 쓴다: 판을 올리지 않고, 파일 · 곁 파일 · 권한을 바꾸지 않고, 없는 파일을 만들지 않는다.

use std::path::Path;

use crate::{Store, StoreError};

impl Store {
    /// 있는 저장소 파일을 읽기 전용으로 연다.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Store, StoreError> {
        let _ = path.as_ref();
        todo!()
    }

    /// `PRAGMA integrity_check`가 ok인가(읽기만 한다).
    pub fn integrity_ok(&self) -> Result<bool, StoreError> {
        todo!()
    }
}
