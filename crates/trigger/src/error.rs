//! 오류.

use thiserror::Error;

/// 이 모듈의 오류. 종류는 구현(N1 · N2)에서 더한다.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {}
