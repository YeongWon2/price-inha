//! pi-ai 7차: Anthropic 공급자. docs/dev/modules/ai.md "7차 작업" 1 · 2번.
//!
//! 로컬 가짜 서버(wiremock)만 쓴다. 실제 AI 서버를 부르지 않는다.
//! 가리기 자체는 조립이 pi-secrets로 끼우므로, 여기서는 시험용 [`Redactor`]로 "자리"만 확인한다:
//! 나가는 글이 모두 그 자리를 거치는가, 거치지 않아야 할 것(모델 이름 · 최대 토큰 · 인증 헤더)은 그대로인가.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use pi_ai::{
    ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES, AiProvider, AnthropicConfig, AnthropicLimits,
    AnthropicProvider, CompletionRequest, Credential, CredentialSource, Message, NoRedaction,
    Redactor, Role, Secret,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// 진짜 키처럼 보이지 않는 시험용 글자.
const TEST_API_KEY: &str = "SIHEOM-AI-KEY-0701";
/// 글에 섞여 들어가는 시험용 비밀 글자.
const TEST_SECRET: &str = "SIHEOM-GARIM-0716";
/// 시험용 가리기가 바꿔 넣는 글자.
const MASK: &str = "[가림]";
const TEST_MODEL: &str = "siheom-model-g";

// ---------- 도우미 ----------

struct FixedSource(Option<Credential>);

#[async_trait]
impl CredentialSource for FixedSource {
    async fn current(&self) -> Option<Credential> {
        self.0.clone()
    }
}

fn source() -> Arc<dyn CredentialSource> {
    Arc::new(FixedSource(Some(Credential::ApiKey(Secret::new(
        TEST_API_KEY,
    )))))
}

/// 시험용 가리기: 비밀 글자와 시험용 키 글자를 [`MASK`]로 바꾸고, 불린 횟수를 센다.
#[derive(Default)]
struct MaskSecrets {
    calls: AtomicUsize,
}

impl Redactor for MaskSecrets {
    fn redact(&self, text: &str) -> String {
        self.calls.fetch_add(1, Ordering::SeqCst);
        text.replace(TEST_SECRET, MASK).replace(TEST_API_KEY, MASK)
    }
}

fn config_for(base_url: &str) -> AnthropicConfig {
    AnthropicConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(5),
        max_retries: 2,
        retry_base_delay: Duration::from_millis(1),
    }
}

fn fast_limits() -> AnthropicLimits {
    AnthropicLimits {
        connect_timeout: Duration::from_secs(2),
        max_response_bytes: ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES,
        max_retry_delay: Duration::from_secs(60),
        retry_after_unit: Duration::from_millis(1),
        jitter_seed: Some(7),
    }
}

fn plain_provider(server: &MockServer) -> AnthropicProvider {
    AnthropicProvider::with_limits(config_for(&server.uri()), fast_limits(), source()).unwrap()
}

fn masked_provider(server: &MockServer, redactor: Arc<MaskSecrets>) -> AnthropicProvider {
    plain_provider(server).with_redactor(redactor)
}

fn msg(role: Role, content: &str) -> Message {
    Message {
        role,
        content: content.to_string(),
    }
}

/// 시스템 글과 대화 글 모두에 비밀 글자가 섞인 요청.
fn leaky_request() -> CompletionRequest {
    CompletionRequest {
        model: TEST_MODEL.to_string(),
        system: Some(format!("너는 분석 담당이다. 참고: {TEST_SECRET}")),
        messages: vec![
            msg(Role::User, &format!("이 값을 봐: {TEST_SECRET}")),
            msg(Role::Assistant, &format!("받았다 {TEST_SECRET} 끝")),
            msg(Role::User, &format!("키는 {TEST_API_KEY} 이다")),
        ],
        max_tokens: 321,
    }
}

/// 공식 문서(Create a Message)의 성공 응답 모양.
fn ok_body(model: &str) -> Value {
    json!({
        "id": "msg_siheom_0701",
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [{"type": "text", "text": "안녕하세요."}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {"input_tokens": 3, "output_tokens": 4}
    })
}

/// 공식 문서(Errors)의 오류 응답 모양.
fn error_body(error_type: &str) -> Value {
    json!({
        "type": "error",
        "error": {"type": error_type, "message": "잠깐"},
        "request_id": "req_siheom_0701"
    })
}

async fn mount_ok(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(TEST_MODEL)))
        .mount(server)
        .await;
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

fn body_text(request: &Request) -> String {
    String::from_utf8_lossy(&request.body).into_owned()
}

/// 본문의 `system`을 글 하나로 모은다(문자열이든 블록 배열이든).
fn system_text(body: &Value) -> String {
    match &body["system"] {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        other => panic!("system 칸이 없다: {other}"),
    }
}

/// 본문의 `messages[i].content`를 글 하나로 모은다(문자열이든 블록 배열이든).
fn message_text(body: &Value, i: usize) -> String {
    match &body["messages"][i]["content"] {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        other => panic!("messages[{i}].content가 없다: {other}"),
    }
}

// ---------- 1. 나가는 글 가리기 자리 ----------

// 검증: E38 V47
#[tokio::test]
async fn 가리기를_끼우면_시스템_글과_대화_글의_비밀이_가려져_나간다() {
    let server = MockServer::start().await;
    mount_ok(&server).await;
    let redactor = Arc::new(MaskSecrets::default());
    let provider = masked_provider(&server, redactor.clone());

    let response = provider.complete(leaky_request()).await.unwrap();
    assert_eq!(response.text, "안녕하세요.");

    let requests = received(&server).await;
    assert_eq!(requests.len(), 1);
    let raw = body_text(&requests[0]);
    assert!(!raw.contains(TEST_SECRET), "본문에 비밀 글자: {raw}");
    assert!(!raw.contains(TEST_API_KEY), "본문에 키 글자: {raw}");

    let body = body_json(&requests[0]);
    assert_eq!(
        system_text(&body),
        format!("너는 분석 담당이다. 참고: {MASK}")
    );
    assert_eq!(message_text(&body, 0), format!("이 값을 봐: {MASK}"));
    assert_eq!(message_text(&body, 1), format!("받았다 {MASK} 끝"));
    assert_eq!(message_text(&body, 2), format!("키는 {MASK} 이다"));
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][1]["role"], "assistant");
    assert_eq!(body["messages"][2]["role"], "user");
    assert!(
        redactor.calls.load(Ordering::SeqCst) >= 1,
        "가리기를 부르지 않았다"
    );
}

// 검증: E38 V47 D4
#[tokio::test]
async fn 가리기는_모델_이름_최대_토큰_인증_헤더를_바꾸지_않는다() {
    let server = MockServer::start().await;
    mount_ok(&server).await;
    let provider = masked_provider(&server, Arc::new(MaskSecrets::default()));

    provider.complete(leaky_request()).await.unwrap();

    let requests = received(&server).await;
    assert_eq!(requests.len(), 1);
    let body = body_json(&requests[0]);
    assert_eq!(body["model"], TEST_MODEL);
    assert_eq!(body["max_tokens"], 321);
    // 가리기가 키 글자를 가리더라도 인증 헤더에는 진짜 자격이 그대로 간다.
    assert_eq!(header(&requests[0], "x-api-key"), Some(TEST_API_KEY));
}

// 검증: E38 V47
#[tokio::test]
async fn 다시_시도로_다시_보내는_요청도_가려져_나간다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(529).set_body_json(error_body("overloaded_error")))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body(TEST_MODEL)))
        .with_priority(2)
        .mount(&server)
        .await;
    let provider = masked_provider(&server, Arc::new(MaskSecrets::default()));

    provider.complete(leaky_request()).await.unwrap();

    let requests = received(&server).await;
    assert_eq!(requests.len(), 2, "한 번 다시 시도해야 한다");
    for (i, request) in requests.iter().enumerate() {
        let raw = body_text(request);
        assert!(
            !raw.contains(TEST_SECRET),
            "{i}번째 본문에 비밀 글자: {raw}"
        );
        assert!(!raw.contains(TEST_API_KEY), "{i}번째 본문에 키 글자: {raw}");
        assert!(raw.contains(MASK), "{i}번째 본문이 가려지지 않았다: {raw}");
    }
}

// 검증: E38 V47
#[tokio::test]
async fn 아무것도_가리지_않는_자리를_끼우면_글이_그대로_나간다() {
    let server = MockServer::start().await;
    mount_ok(&server).await;
    let provider = plain_provider(&server).with_redactor(Arc::new(NoRedaction));
    let request = CompletionRequest {
        model: TEST_MODEL.to_string(),
        system: Some("시스템 글 그대로".to_string()),
        messages: vec![msg(Role::User, "대화 글 그대로")],
        max_tokens: 64,
    };

    provider.complete(request).await.unwrap();

    let requests = received(&server).await;
    assert_eq!(requests.len(), 1);
    let body = body_json(&requests[0]);
    assert_eq!(system_text(&body), "시스템 글 그대로");
    assert_eq!(message_text(&body, 0), "대화 글 그대로");
}

// 검증: E38 V47
#[tokio::test]
async fn 가리기를_끼운_공급자는_비밀이_없는_글도_가리기를_거친다() {
    let server = MockServer::start().await;
    mount_ok(&server).await;
    let redactor = Arc::new(MaskSecrets::default());
    let provider = masked_provider(&server, redactor.clone());
    let request = CompletionRequest {
        model: TEST_MODEL.to_string(),
        system: None,
        messages: vec![msg(Role::User, "평범한 질문")],
        max_tokens: 64,
    };

    provider.complete(request).await.unwrap();

    assert!(
        redactor.calls.load(Ordering::SeqCst) >= 1,
        "비밀이 없어도 나가는 글은 가리기를 거친다"
    );
    let requests = received(&server).await;
    assert_eq!(message_text(&body_json(&requests[0]), 0), "평범한 질문");
}

// ---------- 2. 요청별 모델 · 최대 토큰 ----------

// 검증: D24
#[tokio::test]
async fn 한_공급자에서_요청마다_다른_모델과_최대_토큰이_그대로_실린다() {
    let server = MockServer::start().await;
    mount_ok(&server).await;
    let provider = plain_provider(&server);
    let ask = |model: &str, max_tokens: u32| CompletionRequest {
        model: model.to_string(),
        system: Some(format!("{model} 담당")),
        messages: vec![msg(Role::User, "질문")],
        max_tokens,
    };

    let (a, b) = tokio::join!(
        provider.complete(ask("siheom-model-analyst", 111)),
        provider.complete(ask("siheom-model-trader", 2222)),
    );
    a.unwrap();
    b.unwrap();

    let mut seen: Vec<(String, u64)> = received(&server)
        .await
        .iter()
        .map(|r| {
            let body = body_json(r);
            (
                body["model"].as_str().unwrap().to_string(),
                body["max_tokens"].as_u64().unwrap(),
            )
        })
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        vec![
            ("siheom-model-analyst".to_string(), 111),
            ("siheom-model-trader".to_string(), 2222),
        ]
    );
}

// 검증: D24 E38
#[tokio::test]
async fn 가리기를_끼워도_요청마다_모델과_최대_토큰이_따로_실린다() {
    let server = MockServer::start().await;
    mount_ok(&server).await;
    let provider = masked_provider(&server, Arc::new(MaskSecrets::default()));
    let ask = |model: &str, max_tokens: u32| CompletionRequest {
        model: model.to_string(),
        system: None,
        messages: vec![msg(Role::User, &format!("{model}: {TEST_SECRET}"))],
        max_tokens,
    };

    provider
        .complete(ask("siheom-model-analyst", 111))
        .await
        .unwrap();
    provider
        .complete(ask("siheom-model-risk", 4096))
        .await
        .unwrap();

    let requests = received(&server).await;
    assert_eq!(requests.len(), 2);
    let first = body_json(&requests[0]);
    let second = body_json(&requests[1]);
    assert_eq!(first["model"], "siheom-model-analyst");
    assert_eq!(first["max_tokens"], 111);
    assert_eq!(
        message_text(&first, 0),
        format!("siheom-model-analyst: {MASK}")
    );
    assert_eq!(second["model"], "siheom-model-risk");
    assert_eq!(second["max_tokens"], 4096);
    assert_eq!(
        message_text(&second, 0),
        format!("siheom-model-risk: {MASK}")
    );
}
