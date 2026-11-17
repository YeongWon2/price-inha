//! 판 번호가 붙은 JSON 직렬화. 저장은 store 가 한다.
//!
//! 모양: `{"version": 1, "style": "<스타일>", "params": { <RiskParams 칸들> }}`.

use rust_decimal::Decimal;
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
        let doc: Doc =
            serde_json::from_value(fill_promotion_drawdown(value)).map_err(json_error)?;
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

/// 옛 JSON 에서 `params.promotion.max_drawdown_pct` 칸이 빠졌으면 평가가 쓰는 값
/// (지금 최대 낙폭 한도의 절반)으로 채운다. 값이 있으면 그대로 둔다.
/// 한도(`params.max_drawdown_pct`)가 없거나 읽히지 않으면 손대지 않아 평소 오류가 난다.
fn fill_promotion_drawdown(mut value: Value) -> Value {
    let limit = value
        .pointer("/params/max_drawdown_pct")
        .and_then(|v| serde_json::from_value::<Decimal>(v.clone()).ok());
    let promotion = value
        .pointer_mut("/params/promotion")
        .and_then(Value::as_object_mut);
    if let (Some(limit), Some(promotion)) = (limit, promotion)
        && !promotion.contains_key("max_drawdown_pct")
        && let Ok(half) = serde_json::to_value(limit / Decimal::TWO)
    {
        promotion.insert("max_drawdown_pct".to_owned(), half);
    }
    value
}

fn json_error(e: serde_json::Error) -> Error {
    Error::Json(e.to_string())
}
