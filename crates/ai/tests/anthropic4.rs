//! pi-ai 4차: Anthropic 공급자. docs/dev/modules/ai.md "4차 작업" 3 · 5번.
//!
//! 로컬 가짜 서버(wiremock, 또는 길이 없이 답하는 작은 TCP 서버)만 쓴다. 실제 AI 서버를 부르지 않는다.
//! 3번(429에 `Retry-After`가 없을 때)은 결정(`docs/dev/decisions.md` 2026-11-15 조정자 추천)대로
//! 원본(Hermes)의 물러서기를 따른다. 조사 문서(`docs/research/agent-runtime-deep.md` 4절 표)가 확인한
//! 것은 "Retry-After가 없으면 물러서기, 식은 `min(2초 × 2^(n-1), 60초) + [0, 0.5 × 간격]`"까지이고,
//! 적응형 물러서기(`adaptive_rate_limit_backoff`)의 식은 "확인하지 못한 것"이다. 그래서 확인된
//! 아래 끝(늘어나는 간격)만 시험한다.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use pi_ai::{
    ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES, AiError, AiProvider, AnthropicConfig, AnthropicLimits,
    AnthropicProvider, CompletionRequest, Credential, CredentialSource, Message, Role, Secret,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 진짜 키처럼 보이지 않는 시험용 글자.
const TEST_API_KEY: &str = "SIHEOM-AI-KEY-0401";
const TEST_MODEL: &str = "siheom-model-e";

// ---------- 도우미 ----------

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

fn source() -> Arc<dyn CredentialSource> {
    Arc::new(FixedSource(Some(api_key())))
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
        jitter_seed: Some(11),
    }
}

fn provider(config: AnthropicConfig, limits: AnthropicLimits) -> AnthropicProvider {
    AnthropicProvider::with_limits(config, limits, source()).unwrap()
}

fn request() -> CompletionRequest {
    CompletionRequest {
        model: TEST_MODEL.to_string(),
        system: None,
        messages: vec![Message {
            role: Role::User,
            content: "질문".to_string(),
        }],
        max_tokens: 64,
    }
}

/// 공식 문서(Create a Message)의 성공 응답 모양.
fn ok_body() -> Value {
    json!({
        "id": "msg_siheom_0401",
        "type": "message",
        "role": "assistant",
        "model": TEST_MODEL,
        "content": [{"type": "text", "text": "안녕하세요."}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {"input_tokens": 3, "output_tokens": 4}
    })
}

/// 공식 문서(Errors)의 오류 응답 모양. `retry_after` 칸이 없다.
fn error_body(error_type: &str) -> Value {
    json!({
        "type": "error",
        "error": {"type": error_type, "message": "잠깐"},
        "request_id": "req_siheom_0401"
    })
}

async fn count(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

fn assert_clean(err: &AiError) {
    assert!(!err.to_string().contains(TEST_API_KEY), "오류 문장: {err}");
    assert!(!format!("{err:?}").contains(TEST_API_KEY), "오류 Debug");
}

/// 머리와 `content-length`만큼의 본문을 읽는다.
async fn read_request(stream: &mut tokio::net::TcpStream) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
        let length: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .map_or(0, |v| v.trim().parse().unwrap_or(0));
        if buf.len() >= end + 4 + length {
            return Some(buf);
        }
    }
}

/// `content-length` 없이 `transfer-encoding: chunked`로 답하는 작은 로컬 TCP 서버.
///
/// `body`가 `Some`이면 그 글을 조각 몇 개로 나눠 보내고 끝 조각(`0`)으로 마친다.
/// `None`이면 끝 조각 없이 16KB 조각을 끝없이 보낸다(상대가 끊을 때까지). 보낸 바이트 수를 센다.
async fn chunked_server(status: u16, body: Option<Vec<u8>>) -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let sent = Arc::new(AtomicUsize::new(0));
    let counter = sent.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let counter = counter.clone();
            let body = body.clone();
            tokio::spawn(async move {
                if read_request(&mut stream).await.is_none() {
                    return;
                }
                let head = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n"
                );
                if stream.write_all(head.as_bytes()).await.is_err() {
                    return;
                }
                match body {
                    Some(body) => {
                        for piece in body.chunks(1000) {
                            let frame = format!("{:x}\r\n", piece.len());
                            let _ = stream.write_all(frame.as_bytes()).await;
                            let _ = stream.write_all(piece).await;
                            let _ = stream.write_all(b"\r\n").await;
                        }
                        let _ = stream.write_all(b"0\r\n\r\n").await;
                        let _ = stream.shutdown().await;
                    }
                    None => {
                        let piece = vec![b' '; 16 * 1024];
                        let frame = format!("{:x}\r\n", piece.len());
                        loop {
                            if stream.write_all(frame.as_bytes()).await.is_err()
                                || stream.write_all(&piece).await.is_err()
                                || stream.write_all(b"\r\n").await.is_err()
                            {
                                return;
                            }
                            counter.fetch_add(piece.len(), Ordering::SeqCst);
                        }
                    }
                }
            });
        }
    });
    (format!("http://127.0.0.1:{}", addr.port()), sent)
}

// ---------- 3. 429에 Retry-After가 없으면 원본의 물러서기 ----------

// 검증: A15, A17
#[tokio::test]
async fn retry_after가_없는_429는_늘어나는_간격으로_물러섰다가_다시_시도한다() {
    // 헤더에도 본문에도 retry_after가 없다.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(429).set_body_json(error_body("rate_limit_error")))
        .mount(&server)
        .await;
    let mut config = config_for(&server.uri());
    config.max_retries = 2;
    config.retry_base_delay = Duration::from_millis(200);

    let started = Instant::now();
    let err = provider(config, fast_limits())
        .complete(request())
        .await
        .unwrap_err();
    let elapsed = started.elapsed();

    assert_eq!(err, AiError::RateLimited { retry_after: None });
    assert_clean(&err);
    assert_eq!(count(&server).await, 3, "첫 요청 + 다시 시도 2번");
    // 원본 식의 아래 끝: 200ms + 400ms. 곧바로 다시 보내면 안 된다.
    assert!(
        elapsed >= Duration::from_millis(600),
        "물러서지 않고 다시 보냈다: {elapsed:?}"
    );
}

// 검증: A15, A17
#[tokio::test]
async fn retry_after가_없는_429_뒤에_성공하면_그_응답을_돌려준다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(429).set_body_json(error_body("rate_limit_error")))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(ok_body()))
        .with_priority(2)
        .mount(&server)
        .await;
    let mut config = config_for(&server.uri());
    config.retry_base_delay = Duration::from_millis(150);

    let started = Instant::now();
    let response = provider(config, fast_limits())
        .complete(request())
        .await
        .unwrap();

    assert_eq!(response.text, "안녕하세요.");
    assert_eq!(count(&server).await, 2);
    assert!(
        started.elapsed() >= Duration::from_millis(150),
        "첫 간격을 기다리지 않았다: {:?}",
        started.elapsed()
    );
}

// ---------- 5. 테스트 빈칸: chunked 응답의 상한 ----------

// 검증: A11, E38
#[tokio::test]
async fn 길이_없이_끝없이_오는_chunked_응답은_상한에서_끊고_읽지_못한_응답이다() {
    let (base, sent) = chunked_server(200, None).await;
    let mut limits = fast_limits();
    limits.max_response_bytes = 100_000;

    let result = tokio::time::timeout(
        Duration::from_secs(10),
        provider(config_for(&base), limits).complete(request()),
    )
    .await
    .expect("상한을 넘은 뒤에도 끝없이 읽었다");

    let err = result.unwrap_err();
    assert_eq!(err, AiError::InvalidResponse);
    assert_clean(&err);
    // 상한(100KB)을 넘긴 뒤 곧 그만 읽었다. 소켓 버퍼를 넉넉히 쳐도 수십 MB에는 닿지 않는다.
    assert!(
        sent.load(Ordering::SeqCst) < 32 * 1024 * 1024,
        "보낸 바이트: {}",
        sent.load(Ordering::SeqCst)
    );
}

// 검증: A11
#[tokio::test]
async fn 상한_안의_chunked_응답은_읽는다() {
    let body = ok_body().to_string().into_bytes();
    let (base, _) = chunked_server(200, Some(body)).await;

    let response = provider(config_for(&base), fast_limits())
        .complete(request())
        .await
        .unwrap();

    assert_eq!(response.text, "안녕하세요.");
}

// 검증: A11
#[tokio::test]
async fn 상한을_넘는_chunked_응답은_길이가_정해져_있어도_읽지_못한_응답이다() {
    // 끝 조각으로 끝나는 응답이라도 합이 상한을 넘으면 실패다.
    let mut body = ok_body();
    body["content"][0]["text"] = Value::String("가".repeat(50_000));
    let body = body.to_string().into_bytes();
    let (base, _) = chunked_server(200, Some(body)).await;
    let mut limits = fast_limits();
    limits.max_response_bytes = 10_000;

    let err = provider(config_for(&base), limits)
        .complete(request())
        .await
        .unwrap_err();

    assert_eq!(err, AiError::InvalidResponse);
}

// 검증: V31, E38
#[tokio::test]
async fn 키_확인의_끝없는_chunked_응답도_상한에서_끊는다() {
    let (base, _) = chunked_server(200, None).await;
    let mut limits = fast_limits();
    limits.max_response_bytes = 100_000;
    let p = provider(config_for(&base), limits);

    let result = tokio::time::timeout(Duration::from_secs(10), p.verify(&api_key()))
        .await
        .expect("키 확인이 끝없이 읽었다");

    assert_eq!(result.unwrap_err(), AiError::InvalidResponse);
}

// ---------- 5. 테스트 빈칸: retry_after_unit이 아주 클 때 ----------

// 검증: A18
#[tokio::test]
async fn retry_after_unit이_아주_커도_공황하지_않고_기다린다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "600")
                .set_body_json(error_body("rate_limit_error")),
        )
        .mount(&server)
        .await;
    let mut limits = fast_limits();
    // 600초 × 이 단위는 Duration으로 나타낼 수 없다. 상한에서 멈춰야 한다.
    limits.retry_after_unit = Duration::from_secs(u64::MAX / 4);
    let p = Arc::new(provider(config_for(&server.uri()), limits));

    let running = {
        let p = p.clone();
        tokio::spawn(async move { p.complete(request()).await })
    };
    tokio::time::sleep(Duration::from_millis(1500)).await;

    if running.is_finished() {
        let joined = running.await;
        assert!(
            !joined.as_ref().is_err_and(tokio::task::JoinError::is_panic),
            "기다릴 시간을 셈하다 공황했다"
        );
    } else {
        // 아직 기다리는 중이다(공황하지 않았다). 첫 요청 하나만 보냈다.
        assert_eq!(count(&server).await, 1);
        running.abort();
    }
}

// 검증: A18
#[tokio::test]
async fn 본문의_retry_after와_큰_단위도_공황하지_않는다() {
    let server = MockServer::start().await;
    let mut body = error_body("overloaded_error");
    body["error"]["retry_after"] = json!(1.0e300);
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(529).set_body_json(body))
        .mount(&server)
        .await;
    let mut limits = fast_limits();
    limits.retry_after_unit = Duration::MAX;
    let p = Arc::new(provider(config_for(&server.uri()), limits));

    let running = {
        let p = p.clone();
        tokio::spawn(async move { p.complete(request()).await })
    };
    tokio::time::sleep(Duration::from_millis(1500)).await;

    if running.is_finished() {
        let joined = running.await;
        assert!(
            !joined.as_ref().is_err_and(tokio::task::JoinError::is_panic),
            "기다릴 시간을 셈하다 공황했다"
        );
    } else {
        assert_eq!(count(&server).await, 1);
        running.abort();
    }
}
