//! 자료 키를 주는 곳. 테스트는 가짜를 끼우고, 조립 단계는 [`pi_secrets::SecretStore`]를 넣는다.

use pi_secrets::{SecretStore, SecretString};

use crate::{DataError, SourceName};

/// 출처별 자료 키를 주는 곳. 요청할 때마다 묻는다(나중에 넣은 키도 바로 쓴다).
pub trait KeySource: Send + Sync {
    /// 그 출처의 키. 아직 넣지 않았으면 `None`.
    fn key(&self, source: SourceName) -> Option<SecretString>;

    /// 그 출처의 키를 읽기 오류와 함께 준다. [`crate::DataClient`]는 이것으로 키를 묻는다.
    ///
    /// - 키가 있으면 `Ok(Some(..))`, 넣지 않았으면 `Ok(None)`([`DataError::NoKey`]가 된다).
    /// - 읽지 못했으면(입출력 · 손상) [`DataError::KeyUnreadable`]. 키가 없는 것으로 묻지 않는다.
    ///
    /// 기본은 [`KeySource::key`]를 그대로 쓴다(읽기 오류가 없는 곳).
    fn read_key(&self, source: SourceName) -> Result<Option<SecretString>, DataError> {
        Ok(self.key(source))
    }
}

/// 보관소의 출처별 칸(`KeySlot::{Dart, Ecos, Fred}`)을 [`SecretStore::read_slot`]으로 읽는다.
/// 칸의 `api_key` 값을 쓴다.
impl KeySource for SecretStore {
    /// 읽기 오류는 `None`이다. 오류를 구분하려면 [`KeySource::read_key`]를 쓴다.
    fn key(&self, source: SourceName) -> Option<SecretString> {
        let field = source.slot().fields().first()?;
        let values = self.read_slot(source.slot()).ok()??;
        values.get(field).cloned()
    }

    /// 읽기 오류(입출력 · 손상)는 [`DataError::KeyUnreadable`]이다. 오류에 값 · 경로를 싣지 않는다.
    fn read_key(&self, source: SourceName) -> Result<Option<SecretString>, DataError> {
        let Some(field) = source.slot().fields().first() else {
            return Ok(None);
        };
        let values = self
            .read_slot(source.slot())
            .map_err(|_| DataError::KeyUnreadable { origin: source })?;
        Ok(values.and_then(|values| values.get(field).cloned()))
    }
}
