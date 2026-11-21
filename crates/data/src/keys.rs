//! 자료 키를 주는 곳. 테스트는 가짜를 끼우고, 조립 단계는 [`pi_secrets::SecretStore`]를 넣는다.

use pi_secrets::{Credentials, SecretStore, SecretString};

use crate::news::provider_of;
use crate::{DataError, NewsError, NewsKeySource, NewsKeys, NewsProviderId, SourceName};

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

/// 보관소의 공급자 키(`read_provider`)를 [`NewsKeys::provider_keys`] 선언으로 읽는다.
///
/// - 칸이 모두 (8자 이상으로) 적혀 있으면 `Ok(Some(..))`, 하나라도 없거나 짧으면 `Ok(None)`
///   (공급자는 [`NewsError::NoKey`]로 답한다).
/// - 읽지 못했으면(입출력 · 손상 · 선언 이름 규칙 위반) [`NewsError::KeyUnreadable`]. 오류에 값 · 경로를 싣지 않는다.
///   오류의 공급자는 그 선언을 쓰는 공급자다(아는 선언이 아니면 [`NewsProviderId::Naver`]).
impl NewsKeySource for SecretStore {
    fn read_news_keys(&self, keys: NewsKeys) -> Result<Option<Credentials>, NewsError> {
        self.read_provider(keys.provider_keys())
            .map_err(|_| NewsError::KeyUnreadable {
                provider: provider_of(keys).unwrap_or(NewsProviderId::Naver),
            })
    }
}

#[cfg(test)]
mod tests {
    use pi_secrets::ENV_FILE;

    use super::*;
    use crate::NAVER_NEWS_KEYS;

    /// 가짜 값(실제 키가 아니다).
    const FAKE_ID: &str = "fake-id-00000001";
    const FAKE_SECRET: &str = "fake-secret-0001";

    fn store_with(lines: &str) -> (tempfile::TempDir, SecretStore) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(ENV_FILE), lines).unwrap();
        let store = SecretStore::open(dir.path()).unwrap();
        (dir, store)
    }

    fn line(field: &str, value: &str) -> String {
        let name = NAVER_NEWS_KEYS.provider_keys().env_name(field).unwrap();
        format!("{name}={value}\n")
    }

    #[test]
    fn 보관소에서_뉴스_키를_읽는다() {
        let lines = line("client_id", FAKE_ID) + &line("client_secret", FAKE_SECRET);
        let (_dir, store) = store_with(&lines);
        let creds = store.read_news_keys(NAVER_NEWS_KEYS).unwrap().unwrap();
        assert_eq!(
            creds.get("client_id").map(SecretString::expose),
            Some(FAKE_ID)
        );
        assert_eq!(
            creds.get("client_secret").map(SecretString::expose),
            Some(FAKE_SECRET)
        );
    }

    #[test]
    fn 칸이_모자라면_키가_없는_것이다() {
        let (_dir, store) = store_with(&line("client_id", FAKE_ID));
        assert!(matches!(store.read_news_keys(NAVER_NEWS_KEYS), Ok(None)));
        let (_empty, store) = store_with("");
        assert!(matches!(store.read_news_keys(NAVER_NEWS_KEYS), Ok(None)));
    }

    #[test]
    fn 읽지_못하면_키_없음이_아니라_읽기_오류다() {
        let dir = tempfile::tempdir().unwrap();
        let store = SecretStore::open(dir.path()).unwrap();
        std::fs::create_dir(dir.path().join(ENV_FILE)).unwrap();
        assert!(matches!(
            store.read_news_keys(NAVER_NEWS_KEYS),
            Err(NewsError::KeyUnreadable {
                provider: NewsProviderId::Naver
            })
        ));
    }
}
