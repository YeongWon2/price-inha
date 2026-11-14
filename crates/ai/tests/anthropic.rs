//! pi-ai 공개 API 테스트. docs/dev/modules/ai.md 의 "만드는 것"을 확인한다.
//!
//! 로컬 가짜 서버(wiremock)만 쓴다. 실제 AI 서버를 부르지 않는다.
//! 요청 · 응답 · 오류의 모양은 공식 문서(API overview · Create a Message ·
//! Errors · Rate limits · List Models)를 확인하고 맞췄다.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use pi_ai::{
    AiError, AiProvider, AnthropicConfig, AnthropicProvider, CompletionRequest, Credential,
    CredentialSource, Message, Role, Secret, StopReason,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 진짜 키처럼 보이지 않는 시험용 글자.
const TEST_API_KEY: &str = "SIHEOM-AI-KEY-0001";
const TEST_OAUTH_TOKEN: &str = "SIHEOM-OAUTH-TOKEN-0001";
const TEST_MODEL: &str = "siheom-model-a";

// ---------- 도우미 ----------

/// 늘 같은 자격(또는 없음)을 주는 시험용 자격 자리.
struct FixedSource(Option<Credential>);

#[async_trait]
impl CredentialSource for FixedSource {
    async fn current(&self) -> Option<Credential> {
        self.0.clone()
    }
}

fn api_key() -> Credential {
    Credential::ApiKey(Secret::new(TEST_API_KEY))
}

fn oauth_token() -> Credential {
    Credential::OAuthToken(Secret::new(TEST_OAUTH_TOKEN))
}

/// 가짜 서버를 보는 설정. 다시 시도 간격은 1ms.
fn config_for(base_url: &str) -> AnthropicConfig {
    AnthropicConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(5),
        max_retries: 2,
        retry_base_delay: Duration::from_millis(1),
    }
}

fn provider_with(config: AnthropicConfig, credential: Option<Credential>) -> AnthropicProvider {
    AnthropicProvider::new(config, Arc::new(FixedSource(credential)))
}

fn request_for(model: &str) -> CompletionRequest {
    CompletionRequest {
        model: model.to_string(),
        system: Some("너는 시험용 도우미다.".to_string()),
        messages: vec![
            Message {
                role: Role::User,
                content: "첫 질문".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "첫 답".to_string(),
            },
            Message {
                role: Role::User,
                content: "둘째 질문".to_string(),
            },
        ],
        max_tokens: 321,
    }
}

/// 공식 문서의 Message 응답 모양.
fn ok_body(text: &str, stop_reason: &str, input_tokens: u64, output_tokens: u64) -> Value {
    json!({
        "id": "msg_siheom_0001",
        "type": "message",
        "role": "assistant",
        "model": TEST_MODEL,
        "content": [{"type": "text", "text": text, "citations": null}],
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "stop_details": null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0,
            "service_tier": "standard"
        }
    })
}

/// 공식 문서의 오류 응답 모양.
fn error_body(error_type: &str, message: &str) -> Value {
    json!({
        "type": "error",
        "error": {"type": error_type, "message": message},
        "request_id": "req_siheom_0001"
    })
}

async fn mount_messages(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(response)
        .mount(server)
        .await;
}

async fn mount_models(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(response)
        .mount(server)
        .await;
}

fn ok_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(ok_body("안녕하세요.", "end_turn", 12, 34))
}

fn models_ok_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "data": [],
        "has_more": false,
        "first_id": null,
        "last_id": null
    }))
}

async fn received(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.unwrap()
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request.headers.get(name).map(|v| v.to_str().unwrap())
}

fn body_json(request: &Request) -> Value {
    serde_json::from_slice(&request.body).unwrap()
}

/// 본문의 글 자리는 문자열이거나 text 블록 배열이다(둘 다 문서에 있다).
fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| {
                assert_eq!(b["type"], "text");
                b["text"].as_str().unwrap().to_string()
            })
            .collect(),
        other => panic!("글 자리가 문자열도 배열도 아니다: {other}"),
    }
}

fn assert_no_secret(label: &str, text: &str) {
    assert!(
        !text.contains(TEST_API_KEY),
        "{label}에 시험용 API 키가 있다"
    );
    assert!(
        !text.contains(TEST_OAUTH_TOKEN),
        "{label}에 시험용 OAuth 토큰이 있다"
    );
}

/// 오류의 문장과 `Debug`에 시험용 자격 글자가 없는지 본다.
fn assert_error_is_clean(err: &AiError) {
    assert_no_secret("오류 문장", &err.to_string());
    assert_no_secret("오류 Debug", &format!("{err:?}"));
}

/// 가짜 서버가 늘 `response`로 답할 때의 오류와 받은 요청 수.
async fn fail_with(
    response: ResponseTemplate,
    credential: Credential,
    max_retries: u32,
) -> (AiError, usize) {
    let server = MockServer::start().await;
    mount_messages(&server, response).await;
    let mut config = config_for(&server.uri());
    config.max_retries = max_retries;
    let provider = provider_with(config, Some(credential));
    let err = provider
        .complete(request_for(TEST_MODEL))
        .await
        .unwrap_err();
    let count = received(&server).await.len();
    (err, count)
}

/// 아무도 듣지 않는 로컬 주소.
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

// ---------- 정상 응답 ----------

// 검증: A11, D4
#[tokio::test]
async fn 정상_응답에서_글과_멈춘_이유와_토큰_수를_읽는다() {
    let server = MockServer::start().await;
    mount_messages(
        &server,
        ResponseTemplate::new(200).set_body_json(ok_body("안녕하세요.", "end_turn", 12, 34)),
    )
    .await;
    let provider = provider_with(config_for(&server.uri()), Some(api_key()));

    let response = provider.complete(request_for(TEST_MODEL)).await.unwrap();

    assert_eq!(response.text, "안녕하세요.");
    assert_eq!(response.stop_reason, StopReason::EndTurn);
    assert_eq!(response.usage.input_tokens, 12);
    assert_eq!(response.usage.output_tokens, 34);
}

// 검증: A11
#[tokio::test]
async fn 멈춘_이유를_문서의_값대로_구분한다() {
    let cases = [
        ("end_turn", StopReason::EndTurn),
        ("max_tokens", StopReason::MaxTokens),
        ("stop_sequence", StopReason::StopSequence),
        ("tool_use", StopReason::ToolUse),
        ("pause_turn", StopReason::PauseTurn),
        ("refusal", StopReason::Refusal),
        (
            "model_context_window_exceeded",
            StopReason::ModelContextWindowExceeded,
        ),
    ];
    for (wire, expected) in cases {
        let server = MockServer::start().await;
        mount_messages(
            &server,
            ResponseTemplate::new(200).set_body_json(ok_body("글", wire, 1, 2)),
        )
        .await;
        let provider = provider_with(config_for(&server.uri()), Some(api_key()));

        let response = provider.complete(request_for(TEST_MODEL)).await.unwrap();

        assert_eq!(response.stop_reason, expected, "stop_reason {wire}");
    }
}

// 검증: A11
#[tokio::test]
async fn 모르는_멈춘_이유는_받은_글자_그대로_담는다() {
    let server = MockServer::start().await;
    mount_messages(
        &server,
        ResponseTemplate::new(200).set_body_json(ok_body("글", "siheom_new_reason", 1, 2)),
    )
    .await;
    let provider = provider_with(config_for(&server.uri()), Some(api_key()));

    let response = provider.complete(request_for(TEST_MODEL)).await.unwrap();

    assert_eq!(
        response.stop_reason,
        StopReason::Other("siheom_new_reason".to_string())
    );
}

// 검증: A11
#[tokio::test]
async fn 글은_text_블록만_순서대로_이어_붙인다() {
    let mut body = ok_body("", "end_turn", 5, 6);
    body["content"] = json!([
        {"type": "thinking", "thinking": "", "signature": "siheom-signature"},
        {"type": "text", "text": "앞 글. "},
        {"type": "text", "text": "뒤 글."}
    ]);
    let server = MockServer::start().await;
    mount_messages(&server, ResponseTemplate::new(200).set_body_json(body)).await;
    let provider = provider_with(config_for(&server.uri()), Some(api_key()));

    let response = provider.complete(request_for(TEST_MODEL)).await.unwrap();

    assert_eq!(response.text, "앞 글. 뒤 글.");
}

// 검증: A11
#[tokio::test]
async fn 성공인데_읽을_수_없는_응답은_오류다() {
    let server = MockServer::start().await;
    mount_messages(
        &server,
        ResponseTemplate::new(200).set_body_string("이것은 JSON이 아니다"),
    )
    .await;
    let provider = provider_with(config_for(&server.uri()), Some(api_key()));

    let err = provider
        .complete(request_for(TEST_MODEL))
        .await
        .unwrap_err();

    assert_eq!(err, AiError::InvalidResponse);
}

// ---------- 나가는 요청 ----------

// 검증: A11, D4
#[tokio::test]
async fn 요청은_messages_경로로_필수_헤더와_함께_나간다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let provider = provider_with(config_for(&server.uri()), Some(api_key()));

    provider.complete(request_for(TEST_MODEL)).await.unwrap();

    let requests = received(&server).await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method.as_str(), "POST");
    assert_eq!(request.url.path(), "/v1/messages");
    assert_eq!(request.url.query(), None);
    assert_eq!(header(request, "anthropic-version"), Some("2026-10-21"));
    assert_eq!(
        header(request, "anthropic-version"),
        Some(pi_ai::ANTHROPIC_VERSION)
    );
    let content_type = header(request, "content-type").unwrap();
    assert!(
        content_type.starts_with("application/json"),
        "content-type: {content_type}"
    );
}

// 검증: A11
#[tokio::test]
async fn 요청_본문에_모델과_최대_토큰과_시스템_글과_대화가_담긴다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let provider = provider_with(config_for(&server.uri()), Some(api_key()));

    provider.complete(request_for(TEST_MODEL)).await.unwrap();

    let requests = received(&server).await;
    let body = body_json(&requests[0]);
    assert_eq!(body["model"], TEST_MODEL);
    assert_eq!(body["max_tokens"], 321);
    assert_eq!(text_of(&body["system"]), "너는 시험용 도우미다.");
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(text_of(&messages[0]["content"]), "첫 질문");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(text_of(&messages[1]["content"]), "첫 답");
    assert_eq!(messages[2]["role"], "user");
    assert_eq!(text_of(&messages[2]["content"]), "둘째 질문");
    // 스트리밍은 나중 작업이다.
    assert_ne!(body.get("stream"), Some(&Value::Bool(true)));
    // API 버전 2026-10-21에서 없어진 값은 보내지 않는다.
    for removed in ["temperature", "top_p", "top_k"] {
        assert!(body.get(removed).is_none(), "{removed}를 보냈다");
    }
}

// 검증: A11
#[tokio::test]
async fn 시스템_글이_없으면_system을_보내지_않는다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let provider = provider_with(config_for(&server.uri()), Some(api_key()));
    let mut request = request_for(TEST_MODEL);
    request.system = None;

    provider.complete(request).await.unwrap();

    let requests = received(&server).await;
    let body = body_json(&requests[0]);
    assert!(body.get("system").is_none(), "system: {:?}", body["system"]);
}

// 검증: A11
#[tokio::test]
async fn 모델_이름은_받은_값_그대로_나간다() {
    for model in ["siheom-model-a", "siheom-model-b-20990101"] {
        let server = MockServer::start().await;
        mount_messages(&server, ok_response()).await;
        let provider = provider_with(config_for(&server.uri()), Some(api_key()));

        provider.complete(request_for(model)).await.unwrap();

        let requests = received(&server).await;
        assert_eq!(body_json(&requests[0])["model"], model);
    }
}

// 검증: A11
#[tokio::test]
async fn 기본_주소_끝에_빗금이_있어도_같은_경로로_나간다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let base_url = format!("{}/", server.uri());
    let provider = provider_with(config_for(&base_url), Some(api_key()));

    provider.complete(request_for(TEST_MODEL)).await.unwrap();

    let requests = received(&server).await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/v1/messages");
}

// ---------- 자격 (D4) ----------

// 검증: D4
#[tokio::test]
async fn api_키는_x_api_key_헤더로_나간다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let provider = provider_with(config_for(&server.uri()), Some(api_key()));

    provider.complete(request_for(TEST_MODEL)).await.unwrap();

    let requests = received(&server).await;
    assert_eq!(header(&requests[0], "x-api-key"), Some(TEST_API_KEY));
    assert_eq!(header(&requests[0], "authorization"), None);
}

// 검증: D4
#[tokio::test]
async fn oauth_토큰은_authorization_bearer_헤더로_나간다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let provider = provider_with(config_for(&server.uri()), Some(oauth_token()));

    provider.complete(request_for(TEST_MODEL)).await.unwrap();

    let requests = received(&server).await;
    let expected = format!("Bearer {TEST_OAUTH_TOKEN}");
    assert_eq!(
        header(&requests[0], "authorization"),
        Some(expected.as_str())
    );
    assert_eq!(header(&requests[0], "x-api-key"), None);
}

/// 부를 때마다 차례로 다른 자격을 주는 자리.
struct SwitchingSource {
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl CredentialSource for SwitchingSource {
    async fn current(&self) -> Option<Credential> {
        let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n == 0 {
            Some(api_key())
        } else {
            Some(oauth_token())
        }
    }
}

// 검증: D4
#[tokio::test]
async fn 자격은_호출마다_자격_자리에서_새로_읽는다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let source = Arc::new(SwitchingSource {
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let provider = AnthropicProvider::new(config_for(&server.uri()), source);

    provider.complete(request_for(TEST_MODEL)).await.unwrap();
    provider.complete(request_for(TEST_MODEL)).await.unwrap();

    let requests = received(&server).await;
    assert_eq!(requests.len(), 2);
    assert_eq!(header(&requests[0], "x-api-key"), Some(TEST_API_KEY));
    assert_eq!(header(&requests[1], "x-api-key"), None);
    let expected = format!("Bearer {TEST_OAUTH_TOKEN}");
    assert_eq!(
        header(&requests[1], "authorization"),
        Some(expected.as_str())
    );
}

// 검증: D4
#[tokio::test]
async fn 공급자는_trait_객체로_쓸_수_있다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let provider: Arc<dyn AiProvider> = Arc::new(provider_with(
        config_for(&server.uri()),
        Some(oauth_token()),
    ));

    let response = provider.complete(request_for(TEST_MODEL)).await.unwrap();

    assert_eq!(response.text, "안녕하세요.");
}

// ---------- 자격 없음 (V31) ----------

// 검증: V31
#[tokio::test]
async fn 자격이_없으면_요청을_보내지_않고_설정_안_됨_오류를_낸다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let provider = provider_with(config_for(&server.uri()), None);

    let err = provider
        .complete(request_for(TEST_MODEL))
        .await
        .unwrap_err();

    assert_eq!(err, AiError::NotConfigured);
    assert_eq!(received(&server).await.len(), 0);
}

// 검증: V31
#[tokio::test]
async fn 자격을_넣은_뒤에는_같은_요청이_된다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;

    let before = provider_with(config_for(&server.uri()), None);
    assert_eq!(
        before.complete(request_for(TEST_MODEL)).await.unwrap_err(),
        AiError::NotConfigured
    );
    assert_eq!(received(&server).await.len(), 0);

    let after = provider_with(config_for(&server.uri()), Some(api_key()));
    after.complete(request_for(TEST_MODEL)).await.unwrap();
    assert_eq!(received(&server).await.len(), 1);
}

// ---------- 오류 구분 ----------

// 검증: A11
#[tokio::test]
async fn 상태_401은_인증_실패다() {
    let (err, _) = fail_with(
        ResponseTemplate::new(401)
            .set_body_json(error_body("authentication_error", "invalid x-api-key")),
        api_key(),
        2,
    )
    .await;

    assert_eq!(err, AiError::Auth { status: 401 });
}

// 검증: A11
#[tokio::test]
async fn 상태_403은_인증_실패다() {
    let (err, _) = fail_with(
        ResponseTemplate::new(403).set_body_json(error_body("permission_error", "not allowed")),
        api_key(),
        2,
    )
    .await;

    assert_eq!(err, AiError::Auth { status: 403 });
}

// 검증: A11
#[tokio::test]
async fn 상태_429는_한도_초과이고_retry_after_값을_담는다() {
    let (err, count) = fail_with(
        ResponseTemplate::new(429)
            .insert_header("retry-after", "7")
            .set_body_json(error_body("rate_limit_error", "rate limited")),
        api_key(),
        0,
    )
    .await;

    assert_eq!(
        err,
        AiError::RateLimited {
            retry_after: Some(Duration::from_secs(7))
        }
    );
    assert_eq!(count, 1);
}

// 검증: A11
#[tokio::test]
async fn 상태_429에_retry_after가_없으면_값이_없다() {
    let (err, _) = fail_with(
        ResponseTemplate::new(429).set_body_json(error_body("rate_limit_error", "rate limited")),
        api_key(),
        0,
    )
    .await;

    assert_eq!(err, AiError::RateLimited { retry_after: None });
}

// 검증: A11
#[tokio::test]
async fn 상태_400은_잘못된_요청이고_공급자의_설명을_담는다() {
    let (err, _) = fail_with(
        ResponseTemplate::new(400).set_body_json(error_body(
            "invalid_request_error",
            "max_tokens: must be greater than 0",
        )),
        api_key(),
        2,
    )
    .await;

    assert_eq!(
        err,
        AiError::InvalidRequest {
            message: "max_tokens: must be greater than 0".to_string()
        }
    );
}

// 검증: A11
#[tokio::test]
async fn 상태_5xx와_529는_공급자_장애다() {
    let cases = [
        (500, "api_error"),
        (502, "api_error"),
        (503, "api_error"),
        (504, "timeout_error"),
        (529, "overloaded_error"),
    ];
    for (status, error_type) in cases {
        let (err, _) = fail_with(
            ResponseTemplate::new(status).set_body_json(error_body(error_type, "server trouble")),
            api_key(),
            0,
        )
        .await;

        assert_eq!(err, AiError::ProviderUnavailable { status });
    }
}

// 검증: A11
#[tokio::test]
async fn 오류_본문이_json이_아니어도_상태로_구분한다() {
    let (err, _) = fail_with(
        ResponseTemplate::new(503).set_body_string("<html>bad gateway</html>"),
        api_key(),
        0,
    )
    .await;

    assert_eq!(err, AiError::ProviderUnavailable { status: 503 });
}

// 검증: A11
#[tokio::test]
async fn 설정한_시간_안에_응답이_없으면_시간_초과다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response().set_delay(Duration::from_secs(3))).await;
    let mut config = config_for(&server.uri());
    config.timeout = Duration::from_millis(150);
    config.max_retries = 0;
    let provider = provider_with(config, Some(api_key()));

    let started = Instant::now();
    let err = provider
        .complete(request_for(TEST_MODEL))
        .await
        .unwrap_err();

    assert_eq!(err, AiError::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "시간 제한이 설정값을 따르지 않는다: {:?}",
        started.elapsed()
    );
}

// 검증: A11
#[tokio::test]
async fn 서버에_닿지_못하면_연결_실패다() {
    let mut config = config_for(&dead_base_url());
    config.max_retries = 0;
    let provider = provider_with(config, Some(api_key()));

    let err = provider
        .complete(request_for(TEST_MODEL))
        .await
        .unwrap_err();

    assert_eq!(err, AiError::Connection);
}

// ---------- 다시 시도 ----------

// 검증: A11
#[tokio::test]
async fn 한도_초과는_정한_횟수만큼_다시_시도한다() {
    let (err, count) = fail_with(
        ResponseTemplate::new(429).set_body_json(error_body("rate_limit_error", "rate limited")),
        api_key(),
        2,
    )
    .await;

    assert_eq!(err, AiError::RateLimited { retry_after: None });
    assert_eq!(count, 3, "첫 요청 1번 + 다시 시도 2번");
}

// 검증: A11
#[tokio::test]
async fn 공급자_장애는_정한_횟수만큼_다시_시도한다() {
    for status in [500, 529] {
        let (err, count) = fail_with(
            ResponseTemplate::new(status).set_body_json(error_body("api_error", "server trouble")),
            api_key(),
            2,
        )
        .await;

        assert_eq!(err, AiError::ProviderUnavailable { status });
        assert_eq!(count, 3, "상태 {status}: 첫 요청 1번 + 다시 시도 2번");
    }
}

// 검증: A11
#[tokio::test]
async fn 다시_시도_횟수는_설정값을_따른다() {
    for (max_retries, expected) in [(0, 1), (1, 2), (4, 5)] {
        let (_, count) = fail_with(
            ResponseTemplate::new(500).set_body_json(error_body("api_error", "server trouble")),
            api_key(),
            max_retries,
        )
        .await;

        assert_eq!(count, expected, "max_retries {max_retries}");
    }
}

// 검증: A11
#[tokio::test]
async fn 인증_실패와_잘못된_요청은_다시_시도하지_않는다() {
    let cases = [
        (401, "authentication_error"),
        (403, "permission_error"),
        (400, "invalid_request_error"),
    ];
    for (status, error_type) in cases {
        let (_, count) = fail_with(
            ResponseTemplate::new(status).set_body_json(error_body(error_type, "no")),
            api_key(),
            2,
        )
        .await;

        assert_eq!(count, 1, "상태 {status}는 한 번만 보낸다");
    }
}

// 검증: A11
#[tokio::test]
async fn 다시_시도해서_성공하면_응답을_돌려준다() {
    for status in [429, 529] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(status).set_body_json(error_body("api_error", "잠깐")),
            )
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ok_response())
            .with_priority(2)
            .mount(&server)
            .await;
        let provider = provider_with(config_for(&server.uri()), Some(api_key()));

        let response = provider.complete(request_for(TEST_MODEL)).await.unwrap();

        assert_eq!(response.text, "안녕하세요.");
        assert_eq!(received(&server).await.len(), 2, "상태 {status}");
    }
}

// 검증: A11
#[tokio::test]
async fn 다시_시도는_설정한_간격만큼_기다린_뒤에_한다() {
    let server = MockServer::start().await;
    mount_messages(
        &server,
        ResponseTemplate::new(500).set_body_json(error_body("api_error", "server trouble")),
    )
    .await;
    let mut config = config_for(&server.uri());
    config.max_retries = 2;
    config.retry_base_delay = Duration::from_millis(60);
    let provider = provider_with(config, Some(api_key()));

    let started = Instant::now();
    provider
        .complete(request_for(TEST_MODEL))
        .await
        .unwrap_err();
    let elapsed = started.elapsed();

    // 두 번 기다리고 간격은 줄지 않으므로 적어도 기본 간격의 두 배다.
    assert!(
        elapsed >= Duration::from_millis(120),
        "기다린 시간 {elapsed:?}"
    );
}

// 검증: A11
#[tokio::test]
async fn 간격을_줄이면_다시_시도가_곧바로_끝난다() {
    let started = Instant::now();
    let (_, count) = fail_with(
        ResponseTemplate::new(500).set_body_json(error_body("api_error", "server trouble")),
        api_key(),
        2,
    )
    .await;

    assert_eq!(count, 3);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "간격 1ms인데 오래 걸렸다: {:?}",
        started.elapsed()
    );
}

// 검증: A11
#[test]
fn 기본_설정은_기본_주소와_다시_시도_2번이다() {
    let config = AnthropicConfig::default();

    assert_eq!(config.base_url, pi_ai::ANTHROPIC_DEFAULT_BASE_URL);
    assert_eq!(config.max_retries, 2);
    assert_eq!(config.max_retries, pi_ai::DEFAULT_MAX_RETRIES);
    assert!(config.timeout > Duration::ZERO);
}

// ---------- 자격이 새지 않는다 (E38) ----------

// 검증: E38, D4
#[test]
fn 자격의_debug_출력에_값이_없다() {
    let secret = Secret::new(TEST_API_KEY);
    assert_no_secret("Secret Debug", &format!("{secret:?}"));
    assert_no_secret("Secret Debug(#)", &format!("{secret:#?}"));
    assert_no_secret("Credential Debug", &format!("{:?}", api_key()));
    assert_no_secret("Credential Debug", &format!("{:?}", oauth_token()));
    // 값은 꺼내 달라고 할 때만 나온다.
    assert_eq!(secret.expose(), TEST_API_KEY);
}

// 검증: E38
#[tokio::test]
async fn 요청_본문과_주소에_자격_글자가_없다() {
    for credential in [api_key(), oauth_token()] {
        let server = MockServer::start().await;
        mount_messages(&server, ok_response()).await;
        let provider = provider_with(config_for(&server.uri()), Some(credential));

        provider.complete(request_for(TEST_MODEL)).await.unwrap();

        let requests = received(&server).await;
        let request = &requests[0];
        assert_no_secret("요청 본문", &String::from_utf8_lossy(&request.body));
        assert_no_secret("요청 주소", request.url.as_str());
        // 인증 헤더가 아닌 헤더에도 없다.
        for (name, value) in request.headers.iter() {
            if name.as_str() == "x-api-key" || name.as_str() == "authorization" {
                continue;
            }
            assert_no_secret(name.as_str(), &String::from_utf8_lossy(value.as_bytes()));
        }
    }
}

// 검증: E38
#[tokio::test]
async fn 응답_값의_debug_출력에_자격_글자가_없다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let provider = provider_with(config_for(&server.uri()), Some(api_key()));

    let response = provider.complete(request_for(TEST_MODEL)).await.unwrap();

    assert_no_secret("응답 Debug", &format!("{response:?}"));
}

// 검증: E38
#[tokio::test]
async fn 공급자가_오류_설명에_키를_되돌려_보내도_오류에_담지_않는다() {
    let cases = [
        (400, "invalid_request_error"),
        (401, "authentication_error"),
        (403, "permission_error"),
        (429, "rate_limit_error"),
        (500, "api_error"),
        (529, "overloaded_error"),
    ];
    for (credential, secret) in [(api_key(), TEST_API_KEY), (oauth_token(), TEST_OAUTH_TOKEN)] {
        for (status, error_type) in cases {
            let echoed = format!("bad credential {secret} in request");
            let (err, _) = fail_with(
                ResponseTemplate::new(status).set_body_json(error_body(error_type, &echoed)),
                credential.clone(),
                0,
            )
            .await;

            assert_error_is_clean(&err);
        }
    }
}

// 검증: E38
#[tokio::test]
async fn 잘못된_요청의_설명에서_키_자리만_가리고_나머지는_남긴다() {
    let echoed = format!("header value {TEST_API_KEY} is malformed");
    let (err, _) = fail_with(
        ResponseTemplate::new(400).set_body_json(error_body("invalid_request_error", &echoed)),
        api_key(),
        0,
    )
    .await;

    assert_error_is_clean(&err);
    match err {
        AiError::InvalidRequest { message } => {
            assert!(message.starts_with("header value "), "설명: {message}");
            assert!(message.ends_with(" is malformed"), "설명: {message}");
        }
        other => panic!("잘못된 요청이 아니다: {other:?}"),
    }
}

// 검증: E38
#[tokio::test]
async fn 시간_초과와_연결_실패와_설정_안_됨_오류에도_자격_글자가_없다() {
    // 시간 초과
    let server = MockServer::start().await;
    mount_messages(&server, ok_response().set_delay(Duration::from_secs(3))).await;
    let mut config = config_for(&server.uri());
    config.timeout = Duration::from_millis(100);
    config.max_retries = 0;
    let err = provider_with(config, Some(api_key()))
        .complete(request_for(TEST_MODEL))
        .await
        .unwrap_err();
    assert_eq!(err, AiError::Timeout);
    assert_error_is_clean(&err);

    // 연결 실패
    let mut config = config_for(&dead_base_url());
    config.max_retries = 0;
    let err = provider_with(config, Some(oauth_token()))
        .complete(request_for(TEST_MODEL))
        .await
        .unwrap_err();
    assert_eq!(err, AiError::Connection);
    assert_error_is_clean(&err);

    // 설정 안 됨
    let err = provider_with(config_for(&dead_base_url()), None)
        .complete(request_for(TEST_MODEL))
        .await
        .unwrap_err();
    assert_eq!(err, AiError::NotConfigured);
    assert_error_is_clean(&err);
    assert!(err.to_string().contains("설정"), "문장: {err}");
}

// ---------- 닿는지 확인 (verify) ----------

// 검증: D4, V31
#[tokio::test]
async fn verify는_돈이_들지_않는_요청_한_번으로_키가_닿는지_확인한다() {
    let server = MockServer::start().await;
    mount_models(&server, models_ok_response()).await;
    // 자격 자리가 비어 있어도, 넘겨받은 자격으로 확인한다.
    let provider = provider_with(config_for(&server.uri()), None);

    provider.verify(&api_key()).await.unwrap();

    let requests = received(&server).await;
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method.as_str(), "GET");
    assert_eq!(request.url.path(), "/v1/models");
    assert_eq!(header(request, "x-api-key"), Some(TEST_API_KEY));
    assert_eq!(header(request, "authorization"), None);
    assert_eq!(header(request, "anthropic-version"), Some("2026-10-21"));
    assert_no_secret("확인 요청 주소", request.url.as_str());
    assert_no_secret("확인 요청 본문", &String::from_utf8_lossy(&request.body));
}

// 검증: D4
#[tokio::test]
async fn verify는_oauth_토큰도_같은_자리에서_확인한다() {
    let server = MockServer::start().await;
    mount_models(&server, models_ok_response()).await;
    let provider = provider_with(config_for(&server.uri()), None);

    provider.verify(&oauth_token()).await.unwrap();

    let requests = received(&server).await;
    assert_eq!(requests.len(), 1);
    let expected = format!("Bearer {TEST_OAUTH_TOKEN}");
    assert_eq!(
        header(&requests[0], "authorization"),
        Some(expected.as_str())
    );
    assert_eq!(header(&requests[0], "x-api-key"), None);
}

// 검증: E38
#[tokio::test]
async fn verify는_닿지_않는_키를_인증_실패로_알린다() {
    for status in [401, 403] {
        let server = MockServer::start().await;
        let echoed = format!("invalid key {TEST_API_KEY}");
        mount_models(
            &server,
            ResponseTemplate::new(status)
                .set_body_json(error_body("authentication_error", &echoed)),
        )
        .await;
        let provider = provider_with(config_for(&server.uri()), None);

        let err = provider.verify(&api_key()).await.unwrap_err();

        assert_eq!(err, AiError::Auth { status });
        assert_error_is_clean(&err);
        assert_eq!(received(&server).await.len(), 1);
    }
}

// 검증: A11
#[tokio::test]
async fn verify는_공급자_장애를_인증_실패와_구분하고_한_번만_보낸다() {
    let server = MockServer::start().await;
    mount_models(
        &server,
        ResponseTemplate::new(529).set_body_json(error_body("overloaded_error", "overloaded")),
    )
    .await;
    let provider = provider_with(config_for(&server.uri()), None);

    let err = provider.verify(&api_key()).await.unwrap_err();

    assert_eq!(err, AiError::ProviderUnavailable { status: 529 });
    assert_eq!(received(&server).await.len(), 1);
}

// 검증: A11
#[tokio::test]
async fn verify는_서버에_닿지_못하면_연결_실패다() {
    let provider = provider_with(config_for(&dead_base_url()), None);

    let err = provider.verify(&api_key()).await.unwrap_err();

    assert_eq!(err, AiError::Connection);
}
