//! 자료 어댑터 trait과 가짜 어댑터.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::{DataError, FactKind, FactRequest, Fetched};

/// 자료 어댑터(A10). 판단 쪽은 이 trait만 안다.
#[async_trait]
pub trait DataAdapter: Send + Sync {
    /// 자료를 가져온다(C11). 비어 있으면 `Ok(Fetched::Missing(종류))`, 실패는 [`DataError`](E14).
    async fn fetch(&self, request: &FactRequest) -> Result<Fetched, DataError>;
}

/// 정해진 결과를 돌려주는 가짜 어댑터. 정하지 않은 종류는 `Missing(종류)`.
#[derive(Debug, Default)]
pub struct FakeDataAdapter {
    results: Mutex<HashMap<FactKind, Result<Fetched, DataError>>>,
    calls: Mutex<Vec<FactRequest>>,
}

impl FakeDataAdapter {
    /// 빈 가짜.
    pub fn new() -> Self {
        Self::default()
    }

    /// 그 종류의 요청에 돌려줄 결과를 정한다. 다시 정하면 바뀐다.
    pub fn set(&self, kind: FactKind, result: Result<Fetched, DataError>) {
        let _ = (kind, result);
        todo!()
    }

    /// 받은 요청들(받은 순서).
    pub fn calls(&self) -> Vec<FactRequest> {
        todo!()
    }
}

#[async_trait]
impl DataAdapter for FakeDataAdapter {
    async fn fetch(&self, request: &FactRequest) -> Result<Fetched, DataError> {
        let _ = (request, &self.results, &self.calls);
        todo!()
    }
}
