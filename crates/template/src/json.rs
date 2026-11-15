//! 판 번호가 붙은 JSON 직렬화. 저장은 store 가 한다.
//!
//! 모양: `{"version": 1, "style": "<스타일>", "params": { <RiskParams 칸들> }}`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::caps::check;
use crate::error::Error;
use crate::params::{RiskParams, Style, preset};

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
        Template {
            style,
            params: preset(style),
        }
    }

    /// 판 번호가 붙은 JSON 글.
    pub fn to_json(&self) -> String {
        let doc = DocRef {
            version: FORMAT_VERSION,
            style: self.style,
            params: &self.params,
        };
        // 이 구조는 문자열 키 맵과 직렬화가 늘 되는 값만 담아 실패하지 않는다.
        serde_json::to_string(&doc).unwrap_or_default()
    }

    /// JSON 글을 읽는다. 모르는 판 번호는 [`Error::UnsupportedVersion`], 모양이 틀리면
    /// [`Error::Json`], 상한을 넘는 값이 들어 있으면 [`Error::OverCap`] 이다.
    /// 아래쪽 경계 · 모양을 어긴 값([`crate::adjust`] 와 같은 규칙)도 [`Error::OverCap`] 이며
    /// [`crate::OverCap::invalid`] 에 담긴다.
    pub fn from_json(text: &str) -> Result<Self, Error> {
        let value: Value = serde_json::from_str(text).map_err(json_error)?;
        let version = value
            .get("version")
            .ok_or_else(|| Error::Json("판 번호(version)가 없음".to_owned()))?;
        let version: u32 = serde_json::from_value(version.clone()).map_err(json_error)?;
        if version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion { found: version });
        }
        let doc: Doc = serde_json::from_value(value).map_err(json_error)?;
        check(&doc.params)?;
        Ok(Template {
            style: doc.style,
            params: doc.params,
        })
    }
}

/// 쓰는 모양.
#[derive(Serialize)]
struct DocRef<'a> {
    version: u32,
    style: Style,
    params: &'a RiskParams,
}

/// 읽는 모양. 판 번호는 먼저 따로 읽는다.
#[derive(Deserialize)]
struct Doc {
    style: Style,
    params: RiskParams,
}

fn json_error(e: serde_json::Error) -> Error {
    Error::Json(e.to_string())
}
