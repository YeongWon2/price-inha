//! 자료 어댑터 trait과 가짜 어댑터.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

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
        lock(&self.results).insert(kind, result);
    }

    /// 받은 요청들(받은 순서).
    pub fn calls(&self) -> Vec<FactRequest> {
        lock(&self.calls).clone()
    }
}

#[async_trait]
impl DataAdapter for FakeDataAdapter {
    async fn fetch(&self, request: &FactRequest) -> Result<Fetched, DataError> {
        lock(&self.calls).push(request.clone());
        let kind = request.kind();
        lock(&self.results)
            .get(&kind)
            .cloned()
            .unwrap_or(Ok(Fetched::Missing(kind)))
    }
}

/// 잠금. 다른 스레드가 잠근 채 멈췄어도 안의 값을 그대로 쓴다(가짜라 값이 깨질 일이 없다).
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
