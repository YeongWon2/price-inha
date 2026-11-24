//! 사무실 외형 표(15차 3번, server 21차 6번): 담당 → 고른 외형 id.
//!
//! 값 검사는 server가 한다. 저장소는 빈 글자만 거부한다. 저장소 파일 안에 있으므로 백업 ·
//! 되살리기에 그대로 따라간다.

use std::collections::BTreeMap;

use rusqlite::params;

use crate::model::UnixMillis;
use crate::{Store, StoreError};

impl Store {
    /// 담당별로 고른 외형 id 전부(담당 이름 순서).
    pub fn office_looks(&self) -> Result<BTreeMap<String, String>, StoreError> {
        self.read(|conn| {
            let mut stmt = conn.prepare("SELECT agent, look FROM office_looks ORDER BY agent")?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
            Ok(rows)
        })
    }

    /// 담당의 외형을 정한다(이미 있으면 바꾼다). 담당이나 외형이 빈 글자면 거부하고
    /// 아무것도 바꾸지 않는다.
    pub fn set_office_look(
        &self,
        agent: &str,
        look: &str,
        at: UnixMillis,
    ) -> Result<(), StoreError> {
        if agent.trim().is_empty() || look.trim().is_empty() {
            return Err(StoreError::Incompatible(
                "담당과 외형은 빈 글자일 수 없다".to_owned(),
            ));
        }
        self.write(|conn| {
            conn.execute(
                "INSERT INTO office_looks (agent, look, at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (agent) DO UPDATE SET look = excluded.look, at = excluded.at",
                params![agent, look, at.0],
            )?;
            Ok(())
        })
    }
}
