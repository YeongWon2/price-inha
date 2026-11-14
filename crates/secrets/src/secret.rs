//! 가려진 값 타입: [`SecretString`]과 칸 묶음 [`Credentials`].

use std::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::REDACTED;

/// 비밀 값 하나를 담는 타입.
///
/// - `Debug` · `Display`는 [`REDACTED`]만 찍는다. 값 · 값의 일부 · 길이가 나오지 않는다.
/// - `Serialize`를 구현하지 않는다(응답 · 기록에 실수로 실리지 않게).
/// - 꺼내는 길은 [`SecretString::expose`] 하나다.
/// - 버려질 때 메모리를 지운다.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretString {
    inner: String,
}

impl SecretString {
    /// 값을 감싼다. 길이 · 글자 검사는 [`SecretStore::register`](crate::SecretStore::register)가 한다.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            inner: value.into(),
        }
    }

    /// 값을 꺼낸다. 실제 서버에 보내는 자리에서만 쓴다.
    /// 꺼낸 글자를 기록 · 오류 · 응답에 넣지 않는다.
    pub fn expose(&self) -> &str {
        &self.inner
    }
}

impl fmt::Debug for SecretString {
    /// [`REDACTED`]만 찍는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl fmt::Display for SecretString {
    /// [`REDACTED`]만 찍는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

/// 한 종류의 칸 이름 → 비밀 값 묶음. 등록할 때 넘기고, 읽을 때 돌려받는다.
///
/// `Debug`는 칸 이름과 [`REDACTED`]만 찍는다. `Serialize`를 구현하지 않는다.
#[derive(Clone, Default)]
pub struct Credentials {
    entries: Vec<(String, SecretString)>,
}

impl Credentials {
    /// 빈 묶음.
    pub fn new() -> Self {
        Self::default()
    }

    /// 칸 하나를 더한다. 같은 칸을 다시 주면 나중 값이 남는다.
    /// 칸 이름이 그 종류에 맞는지는 [`SecretStore::register`](crate::SecretStore::register)가 본다.
    #[must_use]
    pub fn with(mut self, field: &str, value: impl Into<String>) -> Self {
        self.set(field, SecretString::new(value));
        self
    }

    /// 칸의 값. 없는 칸이면 `None`.
    pub fn get(&self, field: &str) -> Option<&SecretString> {
        self.entries
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, value)| value)
    }

    /// 들어 있는 칸 이름들(넣은 순서).
    pub fn field_names(&self) -> Vec<&str> {
        self.entries.iter().map(|(name, _)| name.as_str()).collect()
    }

    /// 칸 하나를 넣는다. 이미 있으면 그 자리의 값을 바꾼다(값을 다시 복사하지 않는다).
    pub(crate) fn set(&mut self, field: &str, value: SecretString) {
        match self.entries.iter_mut().find(|(name, _)| name == field) {
            Some((_, slot)) => *slot = value,
            None => self.entries.push((field.to_owned(), value)),
        }
    }

    /// 들어 있는 값들(넣은 순서).
    pub(crate) fn values(&self) -> impl Iterator<Item = &SecretString> {
        self.entries.iter().map(|(_, value)| value)
    }
}

impl fmt::Debug for Credentials {
    /// 칸 이름과 [`REDACTED`]만 찍는다.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.entries.iter().map(|(name, value)| (name, value)))
            .finish()
    }
}
