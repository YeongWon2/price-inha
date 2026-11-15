//! 오류.

use thiserror::Error;

/// 이 모듈의 오류.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {}
