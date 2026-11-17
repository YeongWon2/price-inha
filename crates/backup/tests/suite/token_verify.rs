//! GitHub 토큰 확인: `GET {api_base}/user` 한 번, 다시 시도 없음, 리다이렉트 안 따라감.

use pi_backup::verify_token;
use pi_secrets::{SecretString, VerifyFailure};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "fake-verify-token-for-tests";

fn token() -> SecretString {
    SecretString::new(TOKEN)
}

async fn server_answering(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

// 검증: E4, D7
#[tokio::test]
async fn 두백_번이면_받아들이고_토큰을_머리에_실어_한_번만_보낸다() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let found = verify_token(&server.uri(), &token()).await;

    assert_eq!(found, Ok(()));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// 검증: E4
#[tokio::test]
async fn 끝에_빗금이_있는_주소도_같게_쓴다() {
    let server = server_answering(200).await;

    let found = verify_token(&format!("{}/", server.uri()), &token()).await;

    assert_eq!(found, Ok(()));
}

// 검증: E4
#[tokio::test]
async fn 사백일과_사백삼은_거절이고_다시_시도하지_않는다() {
    for status in [401, 403] {
        let server = server_answering(status).await;

        let found = verify_token(&server.uri(), &token()).await;

        assert_eq!(found, Err(VerifyFailure::Rejected), "{status}");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}

// 검증: E4
#[tokio::test]
async fn 그_밖의_응답은_닿지_않음이고_다시_시도하지_않는다() {
    for status in [404, 429, 500, 502, 503] {
        let server = server_answering(status).await;

        let found = verify_token(&server.uri(), &token()).await;

        assert_eq!(found, Err(VerifyFailure::Unreachable), "{status}");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}

// 검증: E4
#[tokio::test]
async fn 리다이렉트는_따라가지_않고_닿지_않음이다() {
    let target = server_answering(200).await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", format!("{}/user", target.uri())),
        )
        .mount(&server)
        .await;

    let found = verify_token(&server.uri(), &token()).await;

    assert_eq!(found, Err(VerifyFailure::Unreachable));
    assert!(target.received_requests().await.unwrap().is_empty());
}

// 검증: E4
#[tokio::test]
async fn 서버가_닫혀_있으면_닿지_않음이다() {
    // 아무도 듣지 않는 포트. MockServer 는 drop 하면 풀로 돌아가 다른 시험이 다시 쓰므로 닫힌 서버가 아니다.
    let base = "http://127.0.0.1:1";

    let found = verify_token(base, &token()).await;

    assert_eq!(found, Err(VerifyFailure::Unreachable));
}

// 검증: E4, P22
#[tokio::test]
async fn 실패_값은_토큰도_주소도_담지_않는다() {
    let server = server_answering(500).await;
    let base = server.uri();

    let found = verify_token(&base, &token()).await.unwrap_err();

    let text = format!("{found:?} {found}");
    assert!(!text.contains(TOKEN));
    assert!(!text.contains(&base));
}
