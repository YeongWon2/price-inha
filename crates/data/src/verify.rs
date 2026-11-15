//! 확인기: 출처별로 그 키로 닿는지 확인한다.
//!
//! `pi-secrets`의 `Verifier::verify_slot` 기본은 언제나 거부다. 조립(server)은 [`verify_slot`]으로
//! 자료 출처 칸(`KeySlot::{Dart, Ecos, Fred}`)의 `verify_slot`을 구현한다.
//! 요청은 한 번만 보낸다(다시 시도하지 않는다). 받은 값은 쓰지 않고 키를 받아들였는지만 본다.

use pi_secrets::{Credentials, KeySlot, SecretString, VerifyFailure};

use crate::http::Endpoint;
use crate::{DataConfig, DataError, SourceName, dart, ecos, fred};

/// 그 출처가 이 키를 받아들이는지 한 번 확인한다.
///
/// - DART: 회사를 정하지 않은 공시검색 1건. `000` · `013`(없음)이면 받아들인 것이다.
/// - ECOS: 통계표 목록 1행. `INFO-100`이면 [`DataError::KeyRejected`].
/// - FRED: 계열 `DGS10`의 정보. 잘못된 키는 HTTP 400([`DataError::Rejected`]).
///
/// 기본 주소 규칙 · 시간 한도 · 오류 구분은 [`crate::DataClient`]와 같다.
pub async fn verify_key(
    config: &DataConfig,
    source: SourceName,
    key: &SecretString,
) -> Result<(), DataError> {
    let base = match source {
        SourceName::Dart => &config.dart_base_url,
        SourceName::Ecos => &config.ecos_base_url,
        SourceName::Fred => &config.fred_base_url,
    };
    let endpoint = Endpoint::new(source, base, config.timeout)?;
    let key = key.expose();
    match source {
        SourceName::Dart => dart::probe(&endpoint, key).await,
        SourceName::Ecos => ecos::probe(&endpoint, key).await,
        SourceName::Fred => fred::probe(&endpoint, key).await,
    }
}

/// `Verifier::verify_slot`을 이것으로 구현한다.
///
/// 자료 출처 칸이 아니거나 `api_key` 칸이 없으면 [`VerifyFailure::Unreachable`](확인할 줄 모름 = 저장하지 않음).
/// 키 거절 · 요청 거절은 [`VerifyFailure::Rejected`], 그 밖(닿지 않음 · 시간 초과 · 한도 · 형식)은
/// [`VerifyFailure::Unreachable`].
pub async fn verify_slot(
    config: &DataConfig,
    slot: KeySlot,
    values: &Credentials,
) -> Result<(), VerifyFailure> {
    let source = SourceName::from_slot(slot).ok_or(VerifyFailure::Unreachable)?;
    let field = slot.fields().first().ok_or(VerifyFailure::Unreachable)?;
    let key = values.get(field).ok_or(VerifyFailure::Unreachable)?;
    verify_key(config, source, key)
        .await
        .map_err(|err| failure(&err))
}

fn failure(err: &DataError) -> VerifyFailure {
    match err {
        DataError::KeyRejected { .. } | DataError::Rejected { .. } => VerifyFailure::Rejected,
        _ => VerifyFailure::Unreachable,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use wiremock::matchers::{method, path, path_regex, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const KEY: &str = "test-probe-key-not-real-01";

    fn config(server: &MockServer) -> DataConfig {
        DataConfig::new(server.uri(), server.uri(), server.uri())
            .with_timeout(Duration::from_secs(5))
    }

    fn creds(slot: KeySlot) -> Credentials {
        Credentials::new().with(slot.fields()[0], KEY)
    }

    async fn reply(server: &MockServer, route: &str, template: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path_regex(route))
            .respond_with(template)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn dart는_없음도_키를_받아들인_것이다() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/list.json"))
            .and(query_param("crtfc_key", KEY))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "013"})))
            .expect(1)
            .mount(&server)
            .await;

        assert_eq!(
            verify_slot(&config(&server), KeySlot::Dart, &creds(KeySlot::Dart)).await,
            Ok(())
        );
    }

    #[tokio::test]
    async fn dart_키_오류는_거절이다() {
        let server = MockServer::start().await;
        let body = json!({"status": "010", "message": "등록되지 않은 키"});
        reply(
            &server,
            "^/api/list.json$",
            ResponseTemplate::new(200).set_body_json(body),
        )
        .await;

        assert_eq!(
            verify_slot(&config(&server), KeySlot::Dart, &creds(KeySlot::Dart)).await,
            Err(VerifyFailure::Rejected)
        );
    }

    #[tokio::test]
    async fn ecos는_경로의_키로_확인하고_키_무효는_거절이다() {
        let server = MockServer::start().await;
        let route = format!("^/api/StatisticTableList/{KEY}/json/kr/1/1$");
        let body =
            json!({"RESULT": {"CODE": "INFO-100", "MESSAGE": "인증키가 유효하지 않습니다."}});
        reply(
            &server,
            &route,
            ResponseTemplate::new(200).set_body_json(body),
        )
        .await;

        assert_eq!(
            verify_slot(&config(&server), KeySlot::Ecos, &creds(KeySlot::Ecos)).await,
            Err(VerifyFailure::Rejected)
        );
    }

    #[tokio::test]
    async fn ecos_정상_목록이면_받아들인다() {
        let server = MockServer::start().await;
        let body = json!({"StatisticTableList": {"list_total_count": 1, "row": [{}]}});
        reply(
            &server,
            "^/api/StatisticTableList/.*$",
            ResponseTemplate::new(200).set_body_json(body),
        )
        .await;

        assert_eq!(
            verify_slot(&config(&server), KeySlot::Ecos, &creds(KeySlot::Ecos)).await,
            Ok(())
        );
    }

    #[tokio::test]
    async fn fred는_200이면_받아들이고_400이면_거절이다() {
        let ok = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/fred/series"))
            .and(query_param("api_key", KEY))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"seriess": []})))
            .mount(&ok)
            .await;
        let bad = MockServer::start().await;
        reply(&bad, "^/fred/series$", ResponseTemplate::new(400)).await;

        let values = creds(KeySlot::Fred);
        assert_eq!(
            verify_slot(&config(&ok), KeySlot::Fred, &values).await,
            Ok(())
        );
        assert_eq!(
            verify_slot(&config(&bad), KeySlot::Fred, &values).await,
            Err(VerifyFailure::Rejected)
        );
    }

    #[tokio::test]
    async fn 닿지_않으면_닿지_않음이고_자료_칸이_아니면_요청하지_않는다() {
        let server = MockServer::start().await;
        let values = creds(KeySlot::Fred);
        assert_eq!(
            verify_slot(&config(&server), KeySlot::BackupGithubToken, &values).await,
            Err(VerifyFailure::Unreachable)
        );
        assert!(
            server
                .received_requests()
                .await
                .unwrap_or_default()
                .is_empty()
        );

        // wiremock은 버린 서버를 모아 두고 계속 듣게 하므로, 듣는 곳이 없는 포트를 따로 얻는다.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .map(|addr| addr.port())
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let gone = DataConfig::new(&base, &base, &base);
        assert_eq!(
            verify_slot(&gone, KeySlot::Fred, &values).await,
            Err(VerifyFailure::Unreachable)
        );
    }

    #[tokio::test]
    async fn https가_아닌_주소로는_보내지_않는다() {
        let plain = DataConfig::new("http://192.0.2.1", "http://192.0.2.1", "http://192.0.2.1");
        let key = SecretString::new(KEY.to_owned());
        for source in SourceName::ALL {
            assert_eq!(
                verify_key(&plain, source, &key).await,
                Err(DataError::InsecureBaseUrl { origin: source })
            );
        }
    }
}
