//! 사무실 외형 표(15차 3번, server 21차 6번): 담당 → 고른 외형 id.
//!
//! 값 검사는 server가 한다. 저장소는 빈 글자만 거부한다. 저장소 파일 안에 있으므로 백업 ·
//! 되살리기에 그대로 따라간다.

use std::collections::BTreeMap;

use crate::model::UnixMillis;
use crate::{Store, StoreError};

impl Store {
    /// 담당별로 고른 외형 id 전부(담당 이름 순서).
    pub fn office_looks(&self) -> Result<BTreeMap<String, String>, StoreError> {
        let _ = self;
        Ok(BTreeMap::new())
    }

    /// 담당의 외형을 정한다(이미 있으면 바꾼다). 담당이나 외형이 빈 글자면 거부하고
    /// 아무것도 바꾸지 않는다.
    pub fn set_office_look(
        &self,
        agent: &str,
        look: &str,
        at: UnixMillis,
    ) -> Result<(), StoreError> {
        let _ = (self, agent, look, at);
        Err(StoreError::Incompatible(
            "set_office_look 미구현".to_owned(),
        ))
    }
}
