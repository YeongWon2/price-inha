//! 확인기 trait과 등록 전 칸 검사.

use std::future::Future;

use crate::kind::Entry;
use crate::{Credentials, KeyKind, KeySlot, MIN_SECRET_CHARS, SecretsError, VerifyFailure};

/// 그 키로 실제로 닿는지 확인한다 (C3 · E4).
///
/// 실제 증권사 · AI에 닿는 확인기는 `pi-market` · `pi-ai`가 구현한다. 이 모듈에는 trait만 있다.
/// `async fn verify(&self, kind, values) -> Result<(), VerifyFailure>`로 구현하면 된다.
pub trait Verifier: Send + Sync {
    /// `Ok(())`면 저장한다. `Err`면 저장하지 않는다.
    ///
    /// `values`는 칸 검사(빠진 칸 · 짧은 값 · 못 쓰는 글자)를 지난 것만 온다.
    fn verify(
        &self,
        kind: KeyKind,
        values: &Credentials,
    ) -> impl Future<Output = Result<(), VerifyFailure>> + Send;

    /// 6차 칸([`KeySlot`])을 확인한다. `Ok(())`면 저장한다. `Err`면 저장하지 않는다.
    ///
    /// `values`는 칸 검사를 지난 것만 온다. 기본 구현은 언제나 [`VerifyFailure::Unreachable`]이다
    /// (이 칸을 확인할 줄 모르는 확인기로는 저장되지 않는다). 그래서 이 메서드를 더해도 이미 있는
    /// 확인기 구현은 그대로 컴파일된다.
    fn verify_slot(
        &self,
        _slot: KeySlot,
        _values: &Credentials,
    ) -> impl Future<Output = Result<(), VerifyFailure>> + Send {
        async { Err(VerifyFailure::Unreachable) }
    }
}

/// 키 파일에 쓸 수 없는 글자인가: 줄바꿈을 비롯한 제어 문자(탭은 받는다)와 유니코드 줄 · 문단 나눔.
fn is_forbidden(c: char) -> bool {
    (c.is_control() && c != '\t') || matches!(c, '\u{2028}' | '\u{2029}')
}

/// 등록 전 칸 검사. 확인기를 부르기 전에 한다.
///
/// 순서: 그 묶음에 없는 칸 → (칸마다, [`KeyKind::fields`] · [`KeySlot::fields`] 순서로) 빠진 칸 → 못 쓰는 글자 → 짧은 값.
/// 오류에는 `&'static str` 칸 이름만 싣는다.
pub(crate) fn check_fields(
    entry: impl Into<Entry>,
    values: &Credentials,
) -> Result<(), SecretsError> {
    let fields = entry.into().fields();
    if values
        .field_names()
        .iter()
        .any(|name| !fields.contains(name))
    {
        return Err(SecretsError::UnknownField);
    }
    for &field in fields {
        let value = values
            .get(field)
            .ok_or(SecretsError::MissingField { field })?
            .expose();
        if value.chars().any(is_forbidden) {
            return Err(SecretsError::InvalidValue { field });
        }
        if value.chars().count() < MIN_SECRET_CHARS {
            return Err(SecretsError::TooShort {
                field,
                min: MIN_SECRET_CHARS,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 탭은_받고_다른_제어_문자와_줄_나눔은_거부한다() {
        let ok = Credentials::new().with("api_key", "탭\t든\t값\t0001");
        assert!(check_fields(KeyKind::Data, &ok).is_ok());
        for bad in [
            "널\0든-값-0001",
            "줄\u{2028}나눔-값-0001",
            "넬\u{85}든-값-0001",
        ] {
            let values = Credentials::new().with("api_key", bad);
            assert!(matches!(
                check_fields(KeyKind::Data, &values),
                Err(SecretsError::InvalidValue { field: "api_key" })
            ));
        }
    }

    #[test]
    fn 글자_수는_바이트가_아니라_글자로_센다() {
        let seven = Credentials::new().with("api_key", "가나다라마바사");
        assert!(matches!(
            check_fields(KeyKind::Data, &seven),
            Err(SecretsError::TooShort {
                field: "api_key",
                min: 8
            })
        ));
        let eight = Credentials::new().with("api_key", "가나다라마바사아");
        assert!(check_fields(KeyKind::Data, &eight).is_ok());
    }
}
