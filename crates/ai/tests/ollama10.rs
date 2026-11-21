//! 10차: 로컬 Ollama 임베딩 공급자(ai.md 10차 4~11).
//!
//! 요청 · 응답 모양의 근거는 Ollama 공식 API 문서(`docs/api.md`의 "Generate Embeddings" ·
//! "List Local Models", `docs/api/errors.mdx`)다. 실제 Ollama를 부르지 않는다:
//! `wiremock`을 루프백에 띄운다.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use pi_ai::{
    ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES, AiError, AnthropicLimits, DEFAULT_MAX_RETRIES,
    EmbedPurpose, EmbeddingProvider, EmbeddingRequest, OLLAMA_DEFAULT_BASE_URL, OllamaConfig,
    OllamaEmbeddingProvider, Redactor,
};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const MODEL: &str = "bge-m3";

/// 가짜 서버를 보는 설정. 첫 간격 1ms, 다시 시도 2번, 한 요청 64개, 입력 하나 8,192자.
fn config_for(base_url: &str) -> OllamaConfig {
    OllamaConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(5),
        max_retries: 2,
        retry_base_delay: Duration::from_millis(1),
        max_inputs_per_request: 64,
        max_input_chars: 8_192,
    }
}

/// 시험용 연결 제한: 간격 위 끝을 줄이고 씨앗을 고정한다.
fn fast_limits() -> AnthropicLimits {
    AnthropicLimits {
        connect_timeout: Duration::from_secs(2),
        max_response_bytes: ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES,
        max_retry_delay: Duration::from_millis(20),
        retry_after_unit: Duration::from_millis(1),
        jitter_seed: Some(11),
    }
}

fn provider(config: OllamaConfig) -> OllamaEmbeddingProvider {
    OllamaEmbeddingProvider::with_limits(config, fast_limits()).unwrap()
}

fn request(inputs: &[&str]) -> EmbeddingRequest {
    EmbeddingRequest {
        model: MODEL.to_string(),
        inputs: inputs.iter().map(|s| (*s).to_string()).collect(),
        purpose: EmbedPurpose::Document,
        dimensions: None,
    }
}

/// 공식 문서(Generate Embeddings)의 성공 응답 모양.
fn embed_body(vectors: &[Vec<f32>]) -> Value {
    json!({
        "model": MODEL,
        "embeddings": vectors,
        "total_duration": 14_143_917,
        "load_duration": 1_019_500,
        "prompt_eval_count": 8
    })
}

async fn mount_embed(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/api/embed"))
        .respond_with(response)
        .mount(server)
        .await;
}

/// 받은 `input`마다 `[번호, 1.0]` 벡터를 돌려준다(`글<번호>`에서 번호를 읽는다).
struct EchoIndex;

impl Respond for EchoIndex {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        let vectors: Vec<Vec<f32>> = body["input"]
            .as_array()
            .map(|inputs| {
                inputs
                    .iter()
                    .map(|text| {
                        let n: f32 = text
                            .as_str()
                            .and_then(|t| t.strip_prefix("글"))
                            .and_then(|t| t.parse().ok())
                            .unwrap_or(-1.0);
                        vec![n, 1.0]
                    })
                    .collect()
            })
            .unwrap_or_default();
        ResponseTemplate::new(200).set_body_json(json!({
            "model": MODEL,
            "embeddings": vectors,
            "prompt_eval_count": 1
        }))
    }
}

async fn received_bodies(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

async fn count(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

/// 아무것도 듣지 않는 루프백 주소.
async fn closed_base_url() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// 연결을 받아 요청 머리를 조금 읽은 뒤 답 없이 끊는 서버. 받은 연결 수를 센다.
async fn dropping_server() -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = accepted.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            counter.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut chunk = [0u8; 4096];
                let _ = stream.read(&mut chunk).await;
                drop(stream);
            });
        }
    });
    (format!("http://127.0.0.1:{}", addr.port()), accepted)
}

/// `SECRET`을 `***`로 바꾸는 시험용 가리개.
struct MaskSecret;

impl Redactor for MaskSecret {
    fn redact(&self, text: &str) -> String {
        text.replace("SECRET", "***")
    }
}

// ---------- 설정 · 주소 (V23) ----------

// 검증: V23
#[test]
fn 기본_설정은_내_컴퓨터의_ollama를_본다() {
    let config = OllamaConfig::default();

    assert_eq!(OLLAMA_DEFAULT_BASE_URL, "http://127.0.0.1:11434");
    assert_eq!(config.base_url, OLLAMA_DEFAULT_BASE_URL);
    assert_eq!(config.timeout, Duration::from_secs(60));
    assert_eq!(config.max_retries, DEFAULT_MAX_RETRIES);
    assert_eq!(config.max_inputs_per_request, 64);
    assert_eq!(config.max_input_chars, 8_192);
    assert!(OllamaEmbeddingProvider::new(config).is_ok());
}

// 검증: V23
#[test]
fn 루프백이_아닌_주소는_만들_때_거부한다() {
    for base_url in [
        "http://192.168.0.2:11434",
        "https://example.com",
        "http://localhost.example.com:11434",
    ] {
        let config = OllamaConfig {
            base_url: base_url.to_string(),
            ..OllamaConfig::default()
        };
        assert_eq!(
            OllamaEmbeddingProvider::new(config.clone()).err(),
            Some(AiError::InsecureBaseUrl),
            "{base_url}"
        );
        assert_eq!(
            OllamaEmbeddingProvider::with_limits(config, fast_limits()).err(),
            Some(AiError::InsecureBaseUrl),
            "{base_url}"
        );
    }
}

// 검증: V23
#[test]
fn 루프백_주소는_받는다() {
    for base_url in [
        "http://127.0.0.1:11434",
        "http://localhost:11434",
        "http://[::1]:11434",
    ] {
        let config = OllamaConfig {
            base_url: base_url.to_string(),
            ..OllamaConfig::default()
        };
        assert!(OllamaEmbeddingProvider::new(config).is_ok(), "{base_url}");
    }
}

// 검증: V23
#[tokio::test]
async fn 리다이렉트를_따라가지_않는다() {
    let server = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    mount_embed(
        &elsewhere,
        ResponseTemplate::new(200).set_body_json(embed_body(&[vec![0.5]])),
    )
    .await;
    mount_embed(
        &server,
        ResponseTemplate::new(307)
            .insert_header("location", format!("{}/api/embed", elsewhere.uri())),
    )
    .await;

    let result = provider(config_for(&server.uri()))
        .embed(request(&["가"]))
        .await;

    assert!(result.is_err());
    assert_eq!(count(&elsewhere).await, 0);
}

// ---------- 요청 모양 ----------

// 검증: D10, V23
#[tokio::test]
async fn 요청은_api_embed로_인증_없이_model과_input만_보낸다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(embed_body(&[vec![0.1, 0.2], vec![0.3, 0.4]])),
    )
    .await;

    let response = provider(config_for(&server.uri()))
        .embed(request(&["첫 글", "둘째 글"]))
        .await
        .unwrap();

    assert_eq!(response.vectors, vec![vec![0.1, 0.2], vec![0.3, 0.4]]);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let sent = &requests[0];
    assert_eq!(sent.method.as_str(), "POST");
    assert_eq!(sent.url.path(), "/api/embed");
    assert!(sent.headers.get("authorization").is_none());
    assert!(sent.headers.get("x-api-key").is_none());
    let content_type = sent.headers.get("content-type").unwrap().to_str().unwrap();
    assert!(
        content_type.starts_with("application/json"),
        "{content_type}"
    );
    let body: Value = serde_json::from_slice(&sent.body).unwrap();
    assert_eq!(body["model"], MODEL);
    assert_eq!(body["input"], json!(["첫 글", "둘째 글"]));
    let object = body.as_object().unwrap();
    assert!(!object.contains_key("truncate"));
    assert!(!object.contains_key("dimensions"));
}

// 검증: D10
#[tokio::test]
async fn dimensions가_있으면_그_칸을_보낸다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(embed_body(&[vec![0.1, 0.2, 0.3]])),
    )
    .await;
    let mut req = request(&["가"]);
    req.dimensions = Some(3);

    let response = provider(config_for(&server.uri()))
        .embed(req)
        .await
        .unwrap();

    assert_eq!(response.vectors, vec![vec![0.1, 0.2, 0.3]]);
    let bodies = received_bodies(&server).await;
    assert_eq!(bodies[0]["dimensions"], json!(3));
}

// 검증: D10
#[tokio::test]
async fn 색인_글과_질의는_같은_본문으로_보낸다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(embed_body(&[vec![0.5]])),
    )
    .await;
    let provider = provider(config_for(&server.uri()));
    let mut query = request(&["같은 글"]);
    query.purpose = EmbedPurpose::Query;

    provider.embed(request(&["같은 글"])).await.unwrap();
    provider.embed(query).await.unwrap();

    let bodies = received_bodies(&server).await;
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], bodies[1]);
}

// ---------- 가리기 (E38 · V47) ----------

// 검증: E38, V47
#[tokio::test]
async fn 보내는_글은_가린_뒤_나간다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(embed_body(&[vec![0.1], vec![0.2]])),
    )
    .await;
    let provider = provider(config_for(&server.uri())).with_redactor(Arc::new(MaskSecret));

    provider
        .embed(request(&["키는 SECRET 이다", "둘째 SECRET"]))
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let raw = String::from_utf8_lossy(&requests[0].body).to_string();
    assert!(!raw.contains("SECRET"), "{raw}");
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["input"], json!(["키는 *** 이다", "둘째 ***"]));
}

// 검증: E38, V47
#[tokio::test]
async fn 길이는_가린_뒤_글자_수로_잰다_정확히_상한이면_보낸다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(embed_body(&[vec![0.1]])),
    )
    .await;
    let mut config = config_for(&server.uri());
    config.max_input_chars = 10;
    let provider = provider(config).with_redactor(Arc::new(MaskSecret));

    // 가리기 전 13자, 가린 뒤 "***가나다라마바사" = 10자(바이트로는 더 길다).
    let response = provider
        .embed(request(&["SECRET가나다라마바사"]))
        .await
        .unwrap();

    assert_eq!(response.vectors.len(), 1);
    assert_eq!(count(&server).await, 1);
}

// 검증: E38, V47
#[tokio::test]
async fn 가린_뒤_상한보다_한_글자_크면_요청하지_않는다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(embed_body(&[vec![0.1]])),
    )
    .await;
    let mut config = config_for(&server.uri());
    config.max_input_chars = 10;
    let provider = provider(config).with_redactor(Arc::new(MaskSecret));

    let err = provider
        .embed(request(&["짧은 글", "SECRET가나다라마바사아"]))
        .await
        .unwrap_err();

    assert!(matches!(err, AiError::InvalidRequest { .. }), "{err:?}");
    assert!(!err.to_string().contains("SECRET"), "{err}");
    assert!(!format!("{err:?}").contains("SECRET"));
    assert_eq!(count(&server).await, 0);
}

// 검증: D10
#[tokio::test]
async fn 빈_글은_요청하지_않고_거부한다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(embed_body(&[vec![0.1]])),
    )
    .await;

    let err = provider(config_for(&server.uri()))
        .embed(request(&["글", ""]))
        .await
        .unwrap_err();

    assert!(matches!(err, AiError::InvalidRequest { .. }), "{err:?}");
    assert_eq!(count(&server).await, 0);
}

// ---------- 나누기 ----------

// 검증: D10
#[tokio::test]
async fn 상한보다_많은_입력은_나눠_차례로_보내고_순서대로_이어_붙인다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/embed"))
        .respond_with(EchoIndex)
        .mount(&server)
        .await;
    let inputs: Vec<String> = (0..150).map(|i| format!("글{i}")).collect();
    let req = EmbeddingRequest {
        model: MODEL.to_string(),
        inputs,
        purpose: EmbedPurpose::Document,
        dimensions: None,
    };

    let response = provider(config_for(&server.uri()))
        .embed(req)
        .await
        .unwrap();

    let bodies = received_bodies(&server).await;
    let sizes: Vec<usize> = bodies
        .iter()
        .map(|b| b["input"].as_array().unwrap().len())
        .collect();
    assert_eq!(sizes, vec![64, 64, 22]);
    assert_eq!(bodies[1]["input"][0], "글64");
    assert_eq!(response.vectors.len(), 150);
    for (i, vector) in response.vectors.iter().enumerate() {
        assert_eq!(vector, &vec![i as f32, 1.0], "{i}번째");
    }
}

// 검증: D10
#[tokio::test]
async fn 한_조각이_다시_시도_끝까지_실패하면_전체가_실패다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/embed"))
        .respond_with(EchoIndex)
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/embed"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({"error": "잠깐"})))
        .with_priority(2)
        .mount(&server)
        .await;
    let inputs: Vec<String> = (0..150).map(|i| format!("글{i}")).collect();
    let req = EmbeddingRequest {
        model: MODEL.to_string(),
        inputs,
        purpose: EmbedPurpose::Document,
        dimensions: None,
    };

    let err = provider(config_for(&server.uri()))
        .embed(req)
        .await
        .unwrap_err();

    assert_eq!(err, AiError::ProviderUnavailable { status: 500 });
    // 첫 조각 1번 + 둘째 조각 (첫 요청 1번 + 다시 시도 2번). 셋째 조각은 보내지 않는다.
    assert_eq!(count(&server).await, 4);
}

// 검증: D10
#[tokio::test]
async fn 빈_입력_목록은_요청하지_않고_빈_응답이다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(embed_body(&[vec![0.1]])),
    )
    .await;

    let response = provider(config_for(&server.uri()))
        .embed(request(&[]))
        .await
        .unwrap();

    assert!(response.vectors.is_empty());
    assert_eq!(response.usage.total_tokens, 0);
    assert_eq!(count(&server).await, 0);
}

// ---------- 응답 읽기 ----------

// 검증: D10
#[tokio::test]
async fn 응답의_model과_prompt_eval_count를_읽는다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(json!({
            "model": "bge-m3:latest",
            "embeddings": [[0.25, -0.5]],
            "prompt_eval_count": 8
        })),
    )
    .await;

    let response = provider(config_for(&server.uri()))
        .embed(request(&["가"]))
        .await
        .unwrap();

    assert_eq!(response.model, "bge-m3:latest");
    assert_eq!(response.usage.total_tokens, 8);
    assert_eq!(response.vectors, vec![vec![0.25, -0.5]]);
}

// 검증: D10
#[tokio::test]
async fn 응답에_model과_prompt_eval_count가_없으면_요청_모델과_0이다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(json!({"embeddings": [[0.25]]})),
    )
    .await;

    let response = provider(config_for(&server.uri()))
        .embed(request(&["가"]))
        .await
        .unwrap();

    assert_eq!(response.model, MODEL);
    assert_eq!(response.usage.total_tokens, 0);
}

// 검증: D10
#[tokio::test]
async fn 어긋난_응답은_읽지_못한_응답이다() {
    let cases: Vec<(&str, Value, Option<u32>)> = vec![
        ("개수 모자람", json!({"embeddings": [[0.1, 0.2]]}), None),
        (
            "길이 다른 벡터",
            json!({"embeddings": [[0.1, 0.2], [0.3]]}),
            None,
        ),
        (
            "dimensions와 다른 길이",
            json!({"embeddings": [[0.1, 0.2], [0.3, 0.4]]}),
            Some(3),
        ),
        (
            "숫자 아닌 값",
            json!({"embeddings": [[0.1, "가"], [0.3, 0.4]]}),
            None,
        ),
        ("embeddings 없음", json!({"model": MODEL}), None),
    ];
    for (name, body, dimensions) in cases {
        let server = MockServer::start().await;
        mount_embed(&server, ResponseTemplate::new(200).set_body_json(body)).await;
        let mut req = request(&["가", "나"]);
        req.dimensions = dimensions;

        let err = provider(config_for(&server.uri()))
            .embed(req)
            .await
            .unwrap_err();

        assert_eq!(err, AiError::InvalidResponse, "{name}");
        assert_eq!(count(&server).await, 1, "{name}: 다시 시도하지 않는다");
    }
}

// 검증: D10
#[tokio::test]
async fn 응답_크기_상한을_넘으면_읽지_못한_응답이다() {
    let server = MockServer::start().await;
    let big: Vec<f32> = vec![0.125; 2_000];
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(embed_body(&[big])),
    )
    .await;
    let mut limits = fast_limits();
    limits.max_response_bytes = 1_000;
    let provider = OllamaEmbeddingProvider::with_limits(config_for(&server.uri()), limits).unwrap();

    let err = provider.embed(request(&["가"])).await.unwrap_err();

    assert_eq!(err, AiError::InvalidResponse);
}

// ---------- 오류 · 다시 시도 ----------

// 검증: D10, V23
#[tokio::test]
async fn ollama가_꺼져_있으면_연결_실패다() {
    let base_url = closed_base_url().await;

    let err = provider(config_for(&base_url))
        .embed(request(&["가"]))
        .await
        .unwrap_err();

    assert_eq!(err, AiError::Connection);
}

// 검증: D10
#[tokio::test]
async fn 연결_실패는_정한_횟수만큼_다시_시도한다() {
    let (base_url, accepted) = dropping_server().await;

    let err = provider(config_for(&base_url))
        .embed(request(&["가"]))
        .await
        .unwrap_err();

    assert_eq!(err, AiError::Connection);
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        3,
        "첫 요청 1번 + 다시 시도 2번"
    );
}

// 검증: D10
#[tokio::test]
async fn 모델이_없으면_다시_시도하지_않고_ollama_pull을_안내한다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(404).set_body_json(json!({"error": "not found"})),
    )
    .await;

    let err = provider(config_for(&server.uri()))
        .embed(request(&["가"]))
        .await
        .unwrap_err();

    assert!(matches!(err, AiError::InvalidRequest { .. }), "{err:?}");
    let message = err.to_string();
    assert!(message.contains("ollama pull bge-m3"), "{message}");
    assert_eq!(count(&server).await, 1);
}

// 검증: D10
#[tokio::test]
async fn 잘못된_요청은_다시_시도하지_않는다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(400).set_body_json(json!({"error": "invalid input"})),
    )
    .await;

    let err = provider(config_for(&server.uri()))
        .embed(request(&["가"]))
        .await
        .unwrap_err();

    assert!(matches!(err, AiError::InvalidRequest { .. }), "{err:?}");
    assert_eq!(count(&server).await, 1);
}

// 검증: D10
#[tokio::test]
async fn 장애는_다시_시도하고_끝까지_실패하면_공급자_장애다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(503).set_body_json(json!({"error": "busy"})),
    )
    .await;

    let err = provider(config_for(&server.uri()))
        .embed(request(&["가"]))
        .await
        .unwrap_err();

    assert_eq!(err, AiError::ProviderUnavailable { status: 503 });
    assert_eq!(count(&server).await, 3, "첫 요청 1번 + 다시 시도 2번");
}

// 검증: D10
#[tokio::test]
async fn 장애_뒤에_살아나면_다시_시도로_성공한다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/embed"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    mount_embed(
        &server,
        ResponseTemplate::new(200).set_body_json(embed_body(&[vec![0.7]])),
    )
    .await;

    let response = provider(config_for(&server.uri()))
        .embed(request(&["가"]))
        .await
        .unwrap();

    assert_eq!(response.vectors, vec![vec![0.7]]);
    assert_eq!(count(&server).await, 2);
}

// 검증: D10
#[tokio::test]
async fn 시간_안에_답이_없으면_시간_초과다() {
    let server = MockServer::start().await;
    mount_embed(
        &server,
        ResponseTemplate::new(200)
            .set_body_json(embed_body(&[vec![0.1]]))
            .set_delay(Duration::from_secs(2)),
    )
    .await;
    let mut config = config_for(&server.uri());
    config.timeout = Duration::from_millis(100);
    config.max_retries = 0;

    let err = provider(config).embed(request(&["가"])).await.unwrap_err();

    assert_eq!(err, AiError::Timeout);
}

// ---------- check ----------

fn tags_body(names: &[&str]) -> Value {
    let models: Vec<Value> = names
        .iter()
        .map(|name| json!({"name": name, "model": name, "size": 1, "digest": "siheom"}))
        .collect();
    json!({ "models": models })
}

async fn mount_tags(server: &MockServer, names: &[&str]) {
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tags_body(names)))
        .mount(server)
        .await;
}

// 검증: D10, V23
#[tokio::test]
async fn check는_latest_꼬리가_붙은_모델도_찾는다() {
    let server = MockServer::start().await;
    mount_tags(&server, &["llama3.2:latest", "bge-m3:latest"]).await;
    let provider = provider(config_for(&server.uri()));

    provider.check("bge-m3").await.unwrap();
    provider.check("bge-m3:latest").await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert!(!requests.is_empty());
    for sent in &requests {
        assert_eq!(sent.method.as_str(), "GET");
        assert_eq!(sent.url.path(), "/api/tags");
    }
}

// 검증: D10
#[tokio::test]
async fn check는_이름이_같은_모델만_찾는다() {
    let server = MockServer::start().await;
    mount_tags(&server, &["bge-m3-large:latest", "llama3.2:latest"]).await;

    let err = provider(config_for(&server.uri()))
        .check("bge-m3")
        .await
        .unwrap_err();

    assert!(matches!(err, AiError::InvalidRequest { .. }), "{err:?}");
    assert!(err.to_string().contains("ollama pull bge-m3"), "{err}");
}

// 검증: D10
#[tokio::test]
async fn check는_빈_목록이면_모델_없음이다() {
    let server = MockServer::start().await;
    mount_tags(&server, &[]).await;

    let err = provider(config_for(&server.uri()))
        .check("bge-m3")
        .await
        .unwrap_err();

    assert!(matches!(err, AiError::InvalidRequest { .. }), "{err:?}");
}

// 검증: D10
#[tokio::test]
async fn check는_ollama가_꺼져_있으면_연결_실패다() {
    let base_url = closed_base_url().await;

    let err = provider(config_for(&base_url))
        .check("bge-m3")
        .await
        .unwrap_err();

    assert_eq!(err, AiError::Connection);
}
