//! 증권사 자격. `pi-secrets`와 잇는 일은 조립 단계에서 한다.

use std::fmt;

/// 증권사 client_id와 client_secret. `Debug`는 값을 가린다.
#[derive(Clone)]
pub struct Credentials {
    client_id: String,
    client_secret: String,
}

impl Credentials {
    /// 자격을 만든다.
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        todo!()
    }

    /// client_id.
    pub fn client_id(&self) -> &str {
        todo!()
    }

    /// client_secret. 토큰 발급 요청 본문에만 쓴다.
    pub fn client_secret(&self) -> &str {
        todo!()
    }
}

impl fmt::Debug for Credentials {
    /// client_id와 client_secret 글자를 내보내지 않는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

/// 자격을 주는 곳. 요청이 필요할 때마다 묻는다.
pub trait CredentialSource: Send + Sync {
    /// 증권사 자격. 아직 키를 넣지 않았으면 `None`(V31).
    fn credentials(&self) -> Option<Credentials>;
}
