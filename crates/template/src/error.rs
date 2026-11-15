//! 오류.

use std::fmt;

use rust_decimal::Decimal;
use thiserror::Error;

use crate::caps::CappedField;

/// 상한을 넘은 칸 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Violation {
    /// 넘은 칸.
    pub field: CappedField,
    /// 넣으려던 값(퍼센트).
    pub requested: Decimal,
    /// 그 칸의 절대 상한(퍼센트).
    pub cap: Decimal,
}

/// 원칙 상한을 넘는 값(E5). 넘은 칸을 [`CappedField`] 순서대로 모두 담는다.
///
/// 화면에 보이는 글에는 칸마다 상한이 `"<상한>%"` 모양으로 들어간다(V28: 3%를 넣으면 "2%"가 보인다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverCap {
    /// 넘은 칸들. 비어 있지 않다.
    pub violations: Vec<Violation>,
}

impl fmt::Display for OverCap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!()
    }
}

impl std::error::Error for OverCap {}

/// 이 모듈의 오류.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// 원칙 상한을 넘는 값.
    #[error(transparent)]
    OverCap(#[from] OverCap),
    /// 모르는 판 번호.
    #[error("모르는 템플릿 판 번호: {found}")]
    UnsupportedVersion {
        /// 읽은 판 번호.
        found: u32,
    },
    /// JSON 모양이 틀림.
    #[error("템플릿 JSON 을 읽지 못함: {0}")]
    Json(String),
}
