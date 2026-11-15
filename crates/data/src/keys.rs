//! 자료 키를 주는 곳. 테스트는 가짜를 끼우고, 조립 단계는 [`pi_secrets::SecretStore`]를 넣는다.

use pi_secrets::{SecretStore, SecretString};

use crate::SourceName;

/// 출처별 자료 키를 주는 곳. 요청할 때마다 묻는다(나중에 넣은 키도 바로 쓴다).
pub trait KeySource: Send + Sync {
    /// 그 출처의 키. 아직 넣지 않았으면 `None`.
    fn key(&self, source: SourceName) -> Option<SecretString>;
}

/// 보관소의 출처별 칸(`KeySlot::{Dart, Ecos, Fred}`)을 [`SecretStore::read_slot`]으로 읽는다.
/// 칸의 `api_key` 값을 쓴다. 읽기 오류는 키가 없는 것으로 본다.
impl KeySource for SecretStore {
    fn key(&self, source: SourceName) -> Option<SecretString> {
        let field = source.slot().fields().first()?;
        let values = self.read_slot(source.slot()).ok()??;
        values.get(field).cloned()
    }
}
