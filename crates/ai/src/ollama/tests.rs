//! 잠긴 테스트가 덮지 않는 곳(ai.md 10차 6 · 10-1)의 단위 테스트. 가짜 서버만 쓴다.

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use super::*;
use crate::embedding::EmbedPurpose;

fn provider_at(base_url: &str, max_inputs: usize) -> OllamaEmbeddingProvider {
    let config = OllamaConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(5),
        max_retries: 2,
        retry_base_delay: Duration::from_millis(1),
        max_inputs_per_request: max_inputs,
        max_input_chars: 100,
    };
    let limits = AnthropicLimits {
        max_retry_delay: Duration::from_millis(5),
        retry_after_unit: Duration::from_millis(1),
        jitter_seed: Some(3),
        ..AnthropicLimits::default()
    };
    OllamaEmbeddingProvider::with_limits(config, limits).unwrap()
}

fn request(inputs: &[&str]) -> EmbeddingRequest {
    EmbeddingRequest {
        model: "siheom".to_string(),
        inputs: inputs.iter().map(|s| (*s).to_string()).collect(),
        purpose: EmbedPurpose::Query,
        dimensions: None,
    }
}

/// 받은 입력 개수만큼, 첫 글의 글자 수 길이의 벡터를 돌려준다. 모델 이름은 첫 글이다.
struct ByFirstInput;

impl Respond for ByFirstInput {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        let inputs = body["input"].as_array().cloned().unwrap_or_default();
        let first = inputs.first().and_then(Value::as_str).unwrap_or("");
        let vectors = vec![vec![0.5; first.chars().count()]; inputs.len()];
        ResponseTemplate::new(200).set_body_json(json!({
            "model": first,
            "embeddings": vectors,
            "prompt_eval_count": 3
        }))
    }
}

async fn mount(server: &MockServer, verb: &str, at: &str, respond: impl Respond + 'static) {
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(respond)
        .mount(server)
        .await;
}

async fn count(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

#[test]
fn 주소는_루프백_호스트만_받는다() {
    for ok in [
        "http://127.0.0.1:11434",
        "http://LOCALHOST:11434/",
        "https://localhost:11434",
        "http://[::1]:11434",
    ] {
        assert!(is_loopback_base_url(ok), "{ok}");
    }
    for bad in [
        "http://127.0.0.2:11434",
        "http://127.0.0.1:11434@example.com",
        "http://localhost.example.com",
        "ftp://127.0.0.1",
        "127.0.0.1:11434",
        "",
    ] {
        assert!(!is_loopback_base_url(bad), "{bad}");
    }
}

#[tokio::test]
async fn localhost_주소는_루프백으로_가서_닿는다() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/tags",
        ResponseTemplate::new(200).set_body_json(json!({"models": [{"name": "siheom:latest"}]})),
    )
    .await;
    let base_url = format!("http://localhost:{}", server.address().port());

    provider_at(&base_url, 4).check("siheom").await.unwrap();

    assert_eq!(count(&server).await, 1);
}

#[tokio::test]
async fn 나눠_보내면_토큰은_합이고_모델은_첫_조각의_것이다() {
    let server = MockServer::start().await;
    mount(&server, "POST", "/api/embed", ByFirstInput).await;

    let response = provider_at(&server.uri(), 2)
        .embed(request(&["가나", "다", "라마", "바", "사아"]))
        .await
        .unwrap();

    assert_eq!(count(&server).await, 3);
    assert_eq!(response.vectors.len(), 5);
    assert_eq!(response.usage.total_tokens, 9);
    assert_eq!(response.model, "가나");
}

#[tokio::test]
async fn 조각마다_벡터_길이가_다르면_읽지_못한_응답이다() {
    let server = MockServer::start().await;
    mount(&server, "POST", "/api/embed", ByFirstInput).await;

    let err = provider_at(&server.uri(), 1)
        .embed(request(&["가나", "다라마"]))
        .await
        .unwrap_err();

    assert_eq!(err, AiError::InvalidResponse);
}

#[tokio::test]
async fn 개수_상한이_0이어도_하나씩_보낸다() {
    let server = MockServer::start().await;
    mount(&server, "POST", "/api/embed", ByFirstInput).await;

    let response = provider_at(&server.uri(), 0)
        .embed(request(&["가", "나"]))
        .await
        .unwrap();

    assert_eq!(response.vectors.len(), 2);
    assert_eq!(count(&server).await, 2);
}

#[tokio::test]
async fn 한도_초과는_다시_시도하고_끝까지_실패하면_한도_초과다() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/api/embed",
        ResponseTemplate::new(429).set_body_json(json!({"error": "too many requests"})),
    )
    .await;

    let err = provider_at(&server.uri(), 4)
        .embed(request(&["가"]))
        .await
        .unwrap_err();

    assert_eq!(err, AiError::RateLimited { retry_after: None });
    assert_eq!(count(&server).await, 3);
}

#[tokio::test]
async fn 길이_0을_달라고_하면_요청하지_않는다() {
    let server = MockServer::start().await;
    mount(&server, "POST", "/api/embed", ByFirstInput).await;
    let mut req = request(&["가"]);
    req.dimensions = Some(0);

    let err = provider_at(&server.uri(), 4).embed(req).await.unwrap_err();

    assert!(matches!(err, AiError::InvalidRequest { .. }), "{err:?}");
    assert_eq!(count(&server).await, 0);
}

#[tokio::test]
async fn check는_다시_시도하지_않는다() {
    let server = MockServer::start().await;
    mount(&server, "GET", "/api/tags", ResponseTemplate::new(503)).await;

    let err = provider_at(&server.uri(), 4)
        .check("siheom")
        .await
        .unwrap_err();

    assert_eq!(err, AiError::ProviderUnavailable { status: 503 });
    assert_eq!(count(&server).await, 1);
}
