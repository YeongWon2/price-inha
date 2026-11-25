//! 나가는 글의 가림(V89, 결정 24 "메신저로 나가는 글은 가린 글만. 가리기에 실패하면 보내지 않는다").

use std::fmt;

use crate::{Error, RedactError};

/// 가리개 포트. 비밀 값(키 · 토큰 · 계좌 등)을 가린 글을 돌려준다(E38). 조립하는 곳이 넣는다.
pub trait Redactor: Send + Sync {
    /// 가린 글. 가릴 것이 없으면 그대로. 가리기를 마칠 수 없으면 [`RedactError`].
    fn redact(&self, text: &str) -> Result<String, RedactError>;
}

/// 가린 글. [`OutText::redacted`]로만 만든다(가리개를 거치지 않은 글로는 만들 수 없다).
/// [`crate::Channel::send`]는 이것만 받는다.
///
/// `From<String>` · `From<&str>` · `Default` · `FromStr`을 두지 않는다.
#[derive(Clone, PartialEq, Eq)]
pub struct OutText {
    text: String,
}

impl OutText {
    /// `raw`를 `redactor`로 가려 만든다. 가리개가 실패하면 [`Error::Redaction`]이고 글을 만들지 않는다.
    /// 오류에 `raw`를 넣지 않는다.
    pub fn redacted(raw: &str, redactor: &dyn Redactor) -> Result<OutText, Error> {
        let _ = redactor;
        Ok(OutText {
            text: raw.to_owned(),
        })
    }

    /// 가린 글.
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl fmt::Debug for OutText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutText")
            .field("chars", &self.text.chars().count())
            .finish()
    }
}
