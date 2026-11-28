//! (23차, 백로그 U 2번) 있는 저장소 파일을 **읽기만** 하는 손잡이. 진단(`price-inha doctor`)처럼 서버를 띄우지 않는
//! 명령이 쓴다: 판을 올리지 않고, 파일 · 곁 파일 · 권한을 바꾸지 않고, 없는 파일을 만들지 않는다.

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::{Store, StoreError, schema};

impl Store {
    /// 있는 저장소 파일을 읽기 전용으로 연다.
    ///
    /// - [`Store::open`]과 달리 판을 올리지 않는다. 판이 [`crate::SCHEMA_VERSION`]보다 낮으면 그대로 열고, 그 사실은
    ///   [`Store::info`]의 `schema_version`으로 알린다(부르는 쪽이 "띄울 때 올린다"고 안내한다). 아는 판보다 새
    ///   파일도 연다(읽기만 하므로).
    /// - 파일 · 곁 파일(`-wal` · `-shm`) · 권한을 바꾸지 않는다. 곁 파일이 없으면(깨끗이 닫힌 저장소) 곁 파일도
    ///   만들지 않는다. 곁 파일이 있으면(도는 서버) 그 서버가 쓴 것까지 읽는다. 쓰기 잠금을 잡지 않으므로 도는
    ///   서버를 막지 않는다.
    /// - 없는 파일은 만들지 않고 [`StoreError::FileAccess`]. 이 저장소의 파일이 아니면 [`StoreError::Incompatible`].
    /// - 이 손잡이로 쓰는 API를 부르면 SQLite의 읽기 전용 오류([`StoreError::Sqlite`])이고 아무것도 바뀌지 않는다.
    ///   판이 낮은 파일에서는 그 판에 없는 표를 읽는 API도 오류다.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Store, StoreError> {
        let path = path.as_ref();
        let (conn, _version) = schema::open_read_only(path)?;
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        Ok(Store {
            conn: Arc::new(Mutex::new(conn)),
            path: Arc::new(path),
            paper_tail: Arc::new(Mutex::new(None)),
        })
    }

    /// `PRAGMA integrity_check`가 ok인가(읽기만 한다). 깨졌으면 거짓, 읽지 못하면 오류.
    pub fn integrity_ok(&self) -> Result<bool, StoreError> {
        self.read(schema::integrity_ok)
    }
}
