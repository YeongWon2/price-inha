//! 판 번호가 붙은 JSON 직렬화. 저장은 store 가 한다.
//!
//! 모양: `{"version": 1, "style": "<스타일>", "params": { <RiskParams 칸들> }}`.

use crate::error::Error;
use crate::params::{RiskParams, Style};

/// 지금 쓰는 판 번호.
pub const FORMAT_VERSION: u32 = 1;

/// 템플릿 하나: 고른 스타일과 그 사본 위에서 맞춘 값.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// 고른 스타일.
    pub style: Style,
    /// 템플릿 값.
    pub params: RiskParams,
}

impl Template {
    /// 스타일의 기본값으로 새 템플릿을 만든다(C4). `params == preset(style)`.
    pub fn new(style: Style) -> Self {
        let _ = style;
        todo!()
    }

    /// 판 번호가 붙은 JSON 글.
    pub fn to_json(&self) -> String {
        todo!()
    }

    /// JSON 글을 읽는다. 모르는 판 번호는 [`Error::UnsupportedVersion`], 모양이 틀리면
    /// [`Error::Json`], 상한을 넘는 값이 들어 있으면 [`Error::OverCap`] 이다.
    pub fn from_json(text: &str) -> Result<Self, Error> {
        let _ = text;
        todo!()
    }
}
