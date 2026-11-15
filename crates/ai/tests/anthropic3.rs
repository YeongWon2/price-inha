//! pi-ai 3차: Anthropic 공급자를 원본(Hermes) 동작에 맞춘다. docs/dev/modules/ai.md "3차 작업" 1~8번.
//!
//! 로컬 가짜 서버(wiremock, 또는 연결을 끊는 작은 TCP 서버)만 쓴다. 실제 AI 서버를 부르지 않는다.
//! 다시 시도 간격 · `Retry-After`는 원본 `agent/retry_utils.py`(`jittered_backoff` ·
//! `parse_retry_after_seconds` · `provider_retry_after_seconds` · `RETRY_AFTER_CAP_S`)와
//! `agent/turn_recovery.py`의 `compute_error_backoff`를 따른다.
//! 시간에 기대는 테스트는 간격 크기를 설정으로 줄이고, 무작위는 씨앗으로 고정한다.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use pi_ai::{
    ANTHROPIC_DEFAULT_CONNECT_TIMEOUT, ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES, AiError, AiProvider,
    AnthropicConfig, AnthropicLimits, AnthropicProvider, CompletionRequest, Credential,
    CredentialSource, Message, RETRY_AFTER_CAP, RETRY_MAX_DELAY, RetryBackoff, Role, Secret,
    parse_retry_after,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 진짜 키처럼 보이지 않는 시험용 글자.
const TEST_API_KEY: &str = "SIHEOM-AI-KEY-0301";
const TEST_MODEL: &str = "siheom-model-c";

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

fn source(credential: Option<Credential>) -> Arc<dyn CredentialSource> {
    Arc::new(FixedSource(credential))
}

/// 가짜 서버를 보는 설정. 첫 간격 1ms, 다시 시도 2번.
fn config_for(base_url: &str) -> AnthropicConfig {
    AnthropicConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(5),
        max_retries: 2,
        retry_base_delay: Duration::from_millis(1),
    }
}

/// 시험용 3차 설정: `Retry-After` 1초를 1ms로 줄이고, 씨앗을 고정한다.
fn fast_limits() -> AnthropicLimits {
    AnthropicLimits {
        connect_timeout: Duration::from_secs(2),
        max_response_bytes: ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES,
        max_retry_delay: Duration::from_millis(50),
        retry_after_unit: Duration::from_millis(1),
        jitter_seed: Some(7),
    }
}

fn provider(config: AnthropicConfig, limits: AnthropicLimits) -> AnthropicProvider {
    AnthropicProvider::with_limits(config, limits, source(Some(api_key()))).unwrap()
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
        "id": "msg_siheom_0301",
        "type": "message",
        "role": "assistant",
        "model": TEST_MODEL,
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
        "request_id": "req_siheom_0301"
    })
}

fn ok_response() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(ok_body())
}

async fn mount_messages(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(response)
        .mount(server)
        .await;
}

/// 첫 `n`번은 `first`로, 그 뒤는 성공으로 답한다.
async fn mount_first_then_ok(server: &MockServer, first: ResponseTemplate, n: u64) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(first)
        .up_to_n_times(n)
        .with_priority(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ok_response())
        .with_priority(2)
        .mount(server)
        .await;
}

async fn count(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

fn assert_clean(err: &AiError) {
    assert!(!err.to_string().contains(TEST_API_KEY), "오류 문장: {err}");
    assert!(!format!("{err:?}").contains(TEST_API_KEY), "오류 Debug");
}

/// `SystemTime`을 HTTP 날짜(IMF-fixdate, RFC 9110)로 쓴다.
fn http_date(at: SystemTime) -> String {
    let secs = at.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let days = secs / 86_400;
    let rest = secs % 86_400;
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(days % 7) as usize];
    // 1970-01-01부터 센 날을 그레고리력 날짜로(Howard Hinnant의 civil_from_days).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!(
        "{weekday}, {day:02} {} {year} {:02}:{:02}:{:02} GMT",
        months[(month - 1) as usize],
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// 2015-10-21 07:28:00 UTC (RFC 9110의 보기 날짜).
const EXAMPLE_DATE: &str = "Wed, 21 Oct 2015 07:28:00 GMT";
const EXAMPLE_EPOCH: u64 = 1_445_412_480;

fn example_instant() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(EXAMPLE_EPOCH)
}

/// 첫 `drop_first`번의 연결은 요청을 읽은 뒤 답 없이 끊고, 그 뒤 연결에는 성공 응답을 주는
/// 작은 로컬 TCP 서버. 받은 연결 수를 센다.
async fn dropping_server(drop_first: usize) -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let counter = accepted.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let n = counter.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let request = read_request(&mut stream).await;
                if n < drop_first || request.is_none() {
                    drop(stream);
                    return;
                }
                let body = ok_body().to_string();
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(body.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (format!("http://127.0.0.1:{}", addr.port()), accepted)
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

// ---------- 1. 다시 시도 간격 (A17) ----------

// 검증: A17
#[test]
fn 간격은_2의_거듭제곱으로_늘고_jitter는_간격의_절반_안이다() {
    let base = Duration::from_secs(2);
    for seed in 0..50 {
        let mut backoff = RetryBackoff::new(base, RETRY_MAX_DELAY, seed);
        for (retry, secs) in [(1, 2), (2, 4), (3, 8), (4, 16), (5, 32)] {
            let d = Duration::from_secs(secs);
            let wait = backoff.delay(retry);
            assert!(
                wait >= d && wait <= d + d / 2,
                "씨앗 {seed}, {retry}번째: {wait:?}는 [{d:?}, {:?}] 밖이다",
                d + d / 2
            );
        }
    }
}

// 검증: A17
#[test]
fn 첫_다시_시도_간격은_2초_이상_3초_이하다() {
    // 원본: jittered_backoff(1, base_delay=2.0, max_delay=60.0) → 2초 + [0, 1초].
    for seed in 0..100 {
        let wait = RetryBackoff::new(Duration::from_secs(2), RETRY_MAX_DELAY, seed).delay(1);
        assert!(
            wait >= Duration::from_secs(2) && wait <= Duration::from_secs(3),
            "씨앗 {seed}: {wait:?}"
        );
    }
}

// 검증: A17
#[test]
fn 간격은_위_끝_60초에서_멈추고_jitter는_그_위에_더한다() {
    assert_eq!(RETRY_MAX_DELAY, Duration::from_secs(60));
    for seed in 0..20 {
        let mut backoff = RetryBackoff::new(Duration::from_secs(2), RETRY_MAX_DELAY, seed);
        for retry in [6, 7, 10, 40, 64, 1000, u32::MAX] {
            let wait = backoff.delay(retry);
            assert!(
                wait >= Duration::from_secs(60) && wait <= Duration::from_secs(90),
                "씨앗 {seed}, {retry}번째: {wait:?}"
            );
        }
    }
}

// 검증: A17
#[test]
fn 같은_씨앗이면_같은_간격이_나온다() {
    let draw = |seed: u64| {
        let mut backoff = RetryBackoff::new(Duration::from_millis(1000), RETRY_MAX_DELAY, seed);
        (1..=8).map(|n| backoff.delay(n)).collect::<Vec<_>>()
    };
    assert_eq!(draw(42), draw(42));
    assert_eq!(draw(9), draw(9));
}

// 검증: A17
#[test]
fn jitter는_고르게_퍼진다() {
    let d = Duration::from_millis(1000);
    // 여러 씨앗: 가장 작은 값은 아래 끝 가까이, 가장 큰 값은 위 끝 가까이에 있다.
    let waits: Vec<_> = (0..300)
        .map(|seed| RetryBackoff::new(d, RETRY_MAX_DELAY, seed).delay(1))
        .collect();
    let min = *waits.iter().min().unwrap();
    let max = *waits.iter().max().unwrap();
    assert!(min < d + d / 10, "가장 작은 간격 {min:?}");
    assert!(max > d + d * 4 / 10, "가장 큰 간격 {max:?}");
    // 씨앗 하나로도 부를 때마다 무작위가 나아간다.
    let mut one = RetryBackoff::new(d, RETRY_MAX_DELAY, 3);
    let same: Vec<_> = (0..50).map(|_| one.delay(1)).collect();
    assert!(
        same.iter().any(|w| *w != same[0]),
        "같은 차례의 간격이 늘 같다: {:?}",
        same[0]
    );
}

// 검증: A17
#[test]
fn 간격_크기는_첫_간격과_위_끝으로_줄일_수_있다() {
    let mut backoff = RetryBackoff::new(Duration::from_millis(10), Duration::from_millis(25), 1);
    let expected = [10, 20, 25, 25].map(Duration::from_millis);
    for (retry, d) in (1..=4).zip(expected) {
        let wait = backoff.delay(retry);
        assert!(wait >= d && wait <= d + d / 2, "{retry}번째: {wait:?}");
    }
}

// 검증: A17
#[test]
fn 기본_첫_간격은_2초이고_위_끝은_60초다() {
    assert_eq!(
        AnthropicConfig::default().retry_base_delay,
        Duration::from_secs(2)
    );
    assert_eq!(AnthropicLimits::default().max_retry_delay, RETRY_MAX_DELAY);
    assert_eq!(
        AnthropicLimits::default().retry_after_unit,
        Duration::from_secs(1)
    );
    assert_eq!(AnthropicLimits::default().jitter_seed, None);
}

// 검증: A17
#[tokio::test]
async fn 공급자는_늘어나는_간격과_위_끝을_지켜_기다린다() {
    let server = MockServer::start().await;
    mount_messages(
        &server,
        ResponseTemplate::new(503).set_body_json(error_body("api_error")),
    )
    .await;
    let mut config = config_for(&server.uri());
    config.max_retries = 3;
    config.retry_base_delay = Duration::from_millis(100);
    let mut limits = fast_limits();
    limits.max_retry_delay = Duration::from_millis(100);

    let started = Instant::now();
    let err = provider(config, limits)
        .complete(request())
        .await
        .unwrap_err();
    let elapsed = started.elapsed();

    assert_eq!(err, AiError::ProviderUnavailable { status: 503 });
    assert_eq!(count(&server).await, 4);
    // 간격 100 · 100 · 100ms(위 끝) + 각각 jitter [0, 50ms] → 300~450ms.
    // 위 끝이 없으면 100 + 200 + 400 = 700ms 이상이다.
    assert!(elapsed >= Duration::from_millis(300), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(650), "{elapsed:?}");
}

// ---------- 2. Retry-After (A18) ----------

// 검증: A18
#[test]
fn retry_after는_초_숫자를_읽는다() {
    let now = SystemTime::now();
    assert_eq!(parse_retry_after("7", now), Some(Duration::from_secs(7)));
    assert_eq!(
        parse_retry_after(" 120 ", now),
        Some(Duration::from_secs(120))
    );
    assert_eq!(
        parse_retry_after("1.5", now),
        Some(Duration::from_millis(1500))
    );
}

// 검증: A18
#[test]
fn retry_after는_http_날짜를_읽는다() {
    let now = example_instant() - Duration::from_secs(90);
    assert_eq!(
        parse_retry_after(EXAMPLE_DATE, now),
        Some(Duration::from_secs(90))
    );
    // 도우미가 만든 날짜도 같은 모양이다.
    assert_eq!(http_date(example_instant()), EXAMPLE_DATE);
}

// 검증: A18
#[test]
fn retry_after의_상한은_600초다() {
    let now = SystemTime::now();
    assert_eq!(RETRY_AFTER_CAP, Duration::from_secs(600));
    assert_eq!(
        parse_retry_after("600", now),
        Some(Duration::from_secs(600))
    );
    assert_eq!(parse_retry_after("601", now), Some(RETRY_AFTER_CAP));
    assert_eq!(parse_retry_after("100000", now), Some(RETRY_AFTER_CAP));
    let far = example_instant() - Duration::from_secs(3600);
    assert_eq!(parse_retry_after(EXAMPLE_DATE, far), Some(RETRY_AFTER_CAP));
}

// 검증: A18
#[test]
fn 읽을_수_없거나_기다릴_시간이_없는_retry_after는_없는_것이다() {
    let now = example_instant() + Duration::from_secs(10);
    for bad in ["", "   ", "soon", "7초", "Wed, 99 Foo 2015 07:28:00 GMT"] {
        assert_eq!(parse_retry_after(bad, now), None, "{bad:?}");
    }
    // 0과 이미 지난 시각은 기다릴 시간이 없다(원본: 뜨거운 되풀이를 막는다).
    assert_eq!(parse_retry_after("0", now), None);
    assert_eq!(parse_retry_after(EXAMPLE_DATE, now), None);
}

// 검증: A18
#[tokio::test]
async fn 장애_응답의_retry_after도_따라_기다린다() {
    let server = MockServer::start().await;
    mount_first_then_ok(
        &server,
        ResponseTemplate::new(503)
            .insert_header("retry-after", "300")
            .set_body_json(error_body("api_error")),
        1,
    )
    .await;

    let started = Instant::now();
    let response = provider(config_for(&server.uri()), fast_limits())
        .complete(request())
        .await
        .unwrap();

    assert_eq!(response.text, "안녕하세요.");
    assert_eq!(count(&server).await, 2);
    // Retry-After 300초 × 단위 1ms = 300ms.
    assert!(
        started.elapsed() >= Duration::from_millis(300),
        "{:?}",
        started.elapsed()
    );
}

// 검증: A18
#[tokio::test]
async fn 과부하_529의_retry_after도_따라_기다린다() {
    let server = MockServer::start().await;
    mount_first_then_ok(
        &server,
        ResponseTemplate::new(529)
            .insert_header("retry-after", "200")
            .set_body_json(error_body("overloaded_error")),
        1,
    )
    .await;

    let started = Instant::now();
    provider(config_for(&server.uri()), fast_limits())
        .complete(request())
        .await
        .unwrap();

    assert_eq!(count(&server).await, 2);
    assert!(started.elapsed() >= Duration::from_millis(200));
}

// 검증: A18
#[tokio::test]
async fn 한도_초과의_http_날짜_retry_after를_따라_기다린다() {
    let server = MockServer::start().await;
    let at = http_date(SystemTime::now() + Duration::from_secs(400));
    mount_first_then_ok(
        &server,
        ResponseTemplate::new(429)
            .insert_header("retry-after", at.as_str())
            .set_body_json(error_body("rate_limit_error")),
        1,
    )
    .await;

    let started = Instant::now();
    provider(config_for(&server.uri()), fast_limits())
        .complete(request())
        .await
        .unwrap();

    assert_eq!(count(&server).await, 2);
    // 남은 시간 약 399~400초 × 1ms.
    assert!(
        started.elapsed() >= Duration::from_millis(350),
        "{:?}",
        started.elapsed()
    );
}

// 검증: A18
#[tokio::test]
async fn 오래_기다리라는_retry_after도_다시_시도하고_600초에서_자른다() {
    // 2차의 "30초를 넘으면 기다리지 않고 오류" 규칙은 원본에 없어 지웠다.
    let server = MockServer::start().await;
    mount_first_then_ok(
        &server,
        ResponseTemplate::new(429)
            .insert_header("retry-after", "100000")
            .set_body_json(error_body("rate_limit_error")),
        1,
    )
    .await;

    let started = Instant::now();
    let response = provider(config_for(&server.uri()), fast_limits())
        .complete(request())
        .await
        .unwrap();
    let elapsed = started.elapsed();

    assert_eq!(response.text, "안녕하세요.");
    assert_eq!(count(&server).await, 2);
    // 600초(상한) × 1ms = 600ms. 상한이 없으면 100초를 기다린다.
    assert!(elapsed >= Duration::from_millis(600), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(3), "{elapsed:?}");
}

// 검증: A18
#[tokio::test]
async fn retry_after가_30초를_넘어도_다시_시도한다() {
    let server = MockServer::start().await;
    mount_first_then_ok(
        &server,
        ResponseTemplate::new(429)
            .insert_header("retry-after", "45")
            .set_body_json(error_body("rate_limit_error")),
        1,
    )
    .await;

    provider(config_for(&server.uri()), fast_limits())
        .complete(request())
        .await
        .unwrap();

    assert_eq!(count(&server).await, 2);
}

// 검증: A18
#[tokio::test]
async fn retry_after_헤더가_없으면_본문의_retry_after를_본다() {
    // 원본: 본문에 `error` 객체가 있으면 그 안의 retry_after, 없으면 맨 위의 retry_after.
    let nested = json!({
        "type": "error",
        "error": {"type": "rate_limit_error", "message": "잠깐", "retry_after": 30},
    });
    let top = json!({"type": "error", "retry_after": 12});
    for (body, secs) in [(nested, 30), (top, 12)] {
        let server = MockServer::start().await;
        mount_messages(&server, ResponseTemplate::new(429).set_body_json(body)).await;
        let mut config = config_for(&server.uri());
        config.max_retries = 0;

        let err = provider(config, fast_limits())
            .complete(request())
            .await
            .unwrap_err();

        assert_eq!(
            err,
            AiError::RateLimited {
                retry_after: Some(Duration::from_secs(secs))
            }
        );
    }
}

// 검증: A18
#[tokio::test]
async fn 장애_응답_본문의_retry_after도_따라_기다린다() {
    let server = MockServer::start().await;
    let body = json!({
        "type": "error",
        "error": {"type": "overloaded_error", "message": "잠깐", "retry_after": 250},
    });
    mount_first_then_ok(&server, ResponseTemplate::new(529).set_body_json(body), 1).await;

    let started = Instant::now();
    provider(config_for(&server.uri()), fast_limits())
        .complete(request())
        .await
        .unwrap();

    assert_eq!(count(&server).await, 2);
    assert!(started.elapsed() >= Duration::from_millis(250));
}

// 검증: A18
#[tokio::test]
async fn 읽을_수_없는_retry_after_헤더는_없는_것으로_보고_본문을_본다() {
    let cases = [
        (
            ResponseTemplate::new(429)
                .insert_header("retry-after", "soon")
                .set_body_json(error_body("rate_limit_error")),
            None,
        ),
        (
            ResponseTemplate::new(429)
                .insert_header("retry-after", "soon")
                .set_body_json(json!({
                    "type": "error",
                    "error": {"type": "rate_limit_error", "message": "잠깐", "retry_after": 9},
                })),
            Some(Duration::from_secs(9)),
        ),
        (
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0")
                .set_body_json(error_body("rate_limit_error")),
            None,
        ),
    ];
    for (response, expected) in cases {
        let server = MockServer::start().await;
        mount_messages(&server, response).await;
        let mut config = config_for(&server.uri());
        config.max_retries = 0;

        let err = provider(config, fast_limits())
            .complete(request())
            .await
            .unwrap_err();

        assert_eq!(
            err,
            AiError::RateLimited {
                retry_after: expected
            }
        );
    }
}

// ---------- 3. 시간 초과 · 연결 실패도 다시 시도한다 (A15) ----------

// 검증: A15
#[tokio::test]
async fn 시간_초과는_다시_시도한다() {
    let server = MockServer::start().await;
    mount_first_then_ok(&server, ok_response().set_delay(Duration::from_secs(3)), 1).await;
    let mut config = config_for(&server.uri());
    config.timeout = Duration::from_millis(200);

    let response = provider(config, fast_limits())
        .complete(request())
        .await
        .unwrap();

    assert_eq!(response.text, "안녕하세요.");
    assert_eq!(count(&server).await, 2);
}

// 검증: A15, E38
#[tokio::test]
async fn 시간_초과가_계속되면_정한_횟수만큼_다시_시도한_뒤_시간_초과다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response().set_delay(Duration::from_secs(3))).await;
    let mut config = config_for(&server.uri());
    config.timeout = Duration::from_millis(150);
    config.max_retries = 2;

    let err = provider(config, fast_limits())
        .complete(request())
        .await
        .unwrap_err();

    assert_eq!(err, AiError::Timeout);
    assert_clean(&err);
    assert_eq!(count(&server).await, 3, "첫 요청 1번 + 다시 시도 2번");
}

// 검증: A15
#[tokio::test]
async fn 연결이_끊기면_다시_시도한다() {
    let (base_url, accepted) = dropping_server(1).await;

    let response = provider(config_for(&base_url), fast_limits())
        .complete(request())
        .await
        .unwrap();

    assert_eq!(response.text, "안녕하세요.");
    assert_eq!(accepted.load(Ordering::SeqCst), 2);
}

// 검증: A15, E38
#[tokio::test]
async fn 연결_실패가_계속되면_정한_횟수만큼_다시_시도한_뒤_연결_실패다() {
    let (base_url, accepted) = dropping_server(usize::MAX).await;
    let mut config = config_for(&base_url);
    config.max_retries = 2;

    let err = provider(config, fast_limits())
        .complete(request())
        .await
        .unwrap_err();

    assert_eq!(err, AiError::Connection);
    assert_clean(&err);
    assert_eq!(accepted.load(Ordering::SeqCst), 3);
}

// 검증: A15
#[tokio::test]
async fn 시간_초과와_연결_실패의_간격도_늘어나는_간격이다() {
    let (base_url, _) = dropping_server(usize::MAX).await;
    let mut config = config_for(&base_url);
    config.max_retries = 2;
    config.retry_base_delay = Duration::from_millis(100);
    let mut limits = fast_limits();
    limits.max_retry_delay = Duration::from_secs(60);

    let started = Instant::now();
    provider(config, limits)
        .complete(request())
        .await
        .unwrap_err();

    // 100 + 200ms 이상.
    assert!(
        started.elapsed() >= Duration::from_millis(300),
        "{:?}",
        started.elapsed()
    );
}

// 검증: A15
#[tokio::test]
async fn 키_확인은_시간_초과와_연결_실패도_다시_시도하지_않는다() {
    let (base_url, accepted) = dropping_server(usize::MAX).await;
    let provider = provider(config_for(&base_url), fast_limits());

    let err = provider.verify(&api_key()).await.unwrap_err();

    assert_eq!(err, AiError::Connection);
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
}

// ---------- 4. 연결 시간 제한 (A19) ----------

// 검증: A19
#[test]
fn 연결_시간_제한은_따로_10초이고_전체_제한은_그대로다() {
    assert_eq!(ANTHROPIC_DEFAULT_CONNECT_TIMEOUT, Duration::from_secs(10));
    assert_eq!(
        AnthropicLimits::default().connect_timeout,
        ANTHROPIC_DEFAULT_CONNECT_TIMEOUT
    );
    assert_eq!(AnthropicConfig::default().timeout, Duration::from_secs(600));
}

// 검증: A19
#[tokio::test]
async fn 연결_시간_제한을_따로_줘도_요청은_전체_제한까지_기다린다() {
    // 연결은 곧바로 되고 응답은 연결 제한보다 늦게 온다: 연결 제한은 응답 기다림에 걸지 않는다.
    let server = MockServer::start().await;
    mount_messages(&server, ok_response().set_delay(Duration::from_millis(600))).await;
    let mut config = config_for(&server.uri());
    config.timeout = Duration::from_secs(3);
    let mut limits = fast_limits();
    limits.connect_timeout = Duration::from_millis(200);

    let response = provider(config, limits).complete(request()).await.unwrap();

    assert_eq!(response.text, "안녕하세요.");
    assert_eq!(count(&server).await, 1);
}

// ---------- 5. 평문 주소를 막는다 ----------

// 검증: E38
#[test]
fn https가_아닌_주소는_만들_때_거부한다() {
    for url in [
        "http://example.test",
        "http://example.test:8080/",
        "http://0.0.0.0:1",
        "http://127.0.0.1.example.test",
        "http://localhost.example.test",
        "ftp://example.test",
        "example.test",
    ] {
        let result =
            AnthropicProvider::with_limits(config_for(url), fast_limits(), source(Some(api_key())));
        assert!(
            matches!(result, Err(AiError::InsecureBaseUrl)),
            "{url}를 받아들였다"
        );
    }
}

// 검증: E38
#[test]
fn https와_루프백_주소는_받아들인다() {
    for url in [
        "https://example.test",
        "https://example.test/",
        "http://127.0.0.1:1",
        "http://localhost:1",
        "http://[::1]:1",
    ] {
        let result =
            AnthropicProvider::with_limits(config_for(url), fast_limits(), source(Some(api_key())));
        assert!(result.is_ok(), "{url}를 거부했다");
    }
}

// 검증: E38
#[tokio::test]
async fn new로_만든_평문_주소_공급자는_아무것도_보내지_않는다() {
    // 0.0.0.0은 루프백 예외가 아니다. 잘못 보내면 같은 기계의 가짜 서버에 닿아 드러난다.
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let port = server.address().port();
    let provider = AnthropicProvider::new(
        config_for(&format!("http://0.0.0.0:{port}")),
        source(Some(api_key())),
    );

    let err = provider.complete(request()).await.unwrap_err();
    assert_eq!(err, AiError::InsecureBaseUrl);
    let err = provider.verify(&api_key()).await.unwrap_err();
    assert_eq!(err, AiError::InsecureBaseUrl);
    assert_eq!(count(&server).await, 0);
}

// ---------- 6. 응답 크기에 끝이 있다 ----------

/// 성공 응답 JSON을 빈칸으로 늘려 꼭 `size`바이트로 만든다.
fn ok_body_of_size(size: usize) -> Vec<u8> {
    let mut body = ok_body().to_string().into_bytes();
    assert!(body.len() <= size);
    body.resize(size, b' ');
    body
}

// 검증: A11
#[tokio::test]
async fn 상한을_넘는_응답은_읽지_못한_응답이다() {
    let server = MockServer::start().await;
    mount_messages(
        &server,
        ResponseTemplate::new(200).set_body_raw(ok_body_of_size(4001), "application/json"),
    )
    .await;
    let mut limits = fast_limits();
    limits.max_response_bytes = 4000;

    let err = provider(config_for(&server.uri()), limits)
        .complete(request())
        .await
        .unwrap_err();

    assert_eq!(err, AiError::InvalidResponse);
    assert_eq!(
        count(&server).await,
        1,
        "읽지 못한 응답은 다시 보내지 않는다"
    );
}

// 검증: A11
#[tokio::test]
async fn 꼭_상한_크기인_응답은_읽는다() {
    let server = MockServer::start().await;
    mount_messages(
        &server,
        ResponseTemplate::new(200).set_body_raw(ok_body_of_size(4000), "application/json"),
    )
    .await;
    let mut limits = fast_limits();
    limits.max_response_bytes = 4000;

    let response = provider(config_for(&server.uri()), limits)
        .complete(request())
        .await
        .unwrap();

    assert_eq!(response.text, "안녕하세요.");
}

// 검증: A11
#[tokio::test]
async fn 키_확인의_응답도_상한을_넘으면_읽지_못한_응답이다() {
    let server = MockServer::start().await;
    let mut body = json!({"data": [], "has_more": false, "first_id": null, "last_id": null})
        .to_string()
        .into_bytes();
    body.resize(5000, b' ');
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&server)
        .await;
    let mut limits = fast_limits();
    limits.max_response_bytes = 4000;

    let err = provider(config_for(&server.uri()), limits)
        .verify(&api_key())
        .await
        .unwrap_err();

    assert_eq!(err, AiError::InvalidResponse);
}

// 검증: A11
#[test]
fn 응답_상한의_기본값은_10mb다() {
    assert_eq!(ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES, 10_000_000);
    assert_eq!(
        AnthropicLimits::default().max_response_bytes,
        ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES
    );
}

// ---------- 7. 키 확인은 응답 모양도 본다 ----------

// 검증: D4, V31
#[tokio::test]
async fn 키_확인은_2xx여도_모델_목록_모양이_아니면_실패다() {
    let bad_bodies = [
        ResponseTemplate::new(200).set_body_json(json!({})),
        ResponseTemplate::new(200).set_body_json(json!({"data": {}})),
        ResponseTemplate::new(200).set_body_json(json!({"data": null})),
        ResponseTemplate::new(200).set_body_json(json!([])),
        ResponseTemplate::new(200).set_body_string("<html>로그인 화면</html>"),
        ResponseTemplate::new(204),
    ];
    for response in bad_bodies {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(response)
            .mount(&server)
            .await;

        let err = provider(config_for(&server.uri()), fast_limits())
            .verify(&api_key())
            .await
            .unwrap_err();

        assert_eq!(err, AiError::InvalidResponse);
        assert_eq!(count(&server).await, 1);
    }
}

// 검증: D4, V31
#[tokio::test]
async fn 키_확인은_모델_목록_모양이면_성공이다() {
    let bodies = [
        json!({"data": [], "has_more": false, "first_id": null, "last_id": null}),
        json!({
            "data": [{
                "type": "model",
                "id": "siheom-model-c",
                "display_name": "시험 모델",
                "created_at": "2026-01-01T00:00:00Z"
            }],
            "has_more": true,
            "first_id": "siheom-model-c",
            "last_id": "siheom-model-c"
        }),
    ];
    for body in bodies {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        provider(config_for(&server.uri()), fast_limits())
            .verify(&api_key())
            .await
            .unwrap();
    }
}

// ---------- 8. 헤더로 보낼 수 없는 자격 ----------

/// 줄바꿈 · 제어 문자가 든 시험용 키.
fn malformed_keys() -> Vec<String> {
    ["\n", "\r", "\u{0}", "\u{1}", "\u{7f}"]
        .iter()
        .map(|c| format!("SIHEOM-AI{c}KEY-0302"))
        .collect()
}

// 검증: V31, E38
#[tokio::test]
async fn 줄바꿈이나_제어_문자가_든_키는_자격_모양이_잘못됨이고_보내지_않는다() {
    for key in malformed_keys() {
        let server = MockServer::start().await;
        mount_messages(&server, ok_response()).await;
        let provider = AnthropicProvider::with_limits(
            config_for(&server.uri()),
            fast_limits(),
            source(Some(Credential::ApiKey(Secret::new(key.clone())))),
        )
        .unwrap();

        let err = provider.complete(request()).await.unwrap_err();

        assert_eq!(err, AiError::InvalidCredential, "{key:?}");
        assert_eq!(count(&server).await, 0);
        for text in [err.to_string(), format!("{err:?}")] {
            assert!(!text.contains("SIHEOM-AI"), "오류에 키 값이 있다: {text}");
            assert!(!text.contains("KEY-0302"), "오류에 키 값이 있다: {text}");
        }
    }
}

// 검증: V31, E38
#[tokio::test]
async fn 키_확인도_모양이_잘못된_키를_보내지_않고_구분해_알린다() {
    for key in malformed_keys() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"data": [], "has_more": false})),
            )
            .mount(&server)
            .await;
        let provider = provider(config_for(&server.uri()), fast_limits());

        let err = provider
            .verify(&Credential::ApiKey(Secret::new(key.clone())))
            .await
            .unwrap_err();

        assert_eq!(err, AiError::InvalidCredential, "{key:?}");
        assert_eq!(count(&server).await, 0);
        assert!(!format!("{err:?}").contains("KEY-0302"));
    }
}

// 검증: V31
#[tokio::test]
async fn 빈_키는_여전히_설정_안_됨이다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let provider = AnthropicProvider::with_limits(
        config_for(&server.uri()),
        fast_limits(),
        source(Some(Credential::ApiKey(Secret::new("")))),
    )
    .unwrap();

    assert_eq!(
        provider.complete(request()).await.unwrap_err(),
        AiError::NotConfigured
    );
    assert_eq!(count(&server).await, 0);
}

// 검증: V31
#[tokio::test]
async fn 자격이_없으면_with_limits로_만든_공급자도_보내지_않는다() {
    let server = MockServer::start().await;
    mount_messages(&server, ok_response()).await;
    let provider =
        AnthropicProvider::with_limits(config_for(&server.uri()), fast_limits(), source(None))
            .unwrap();

    assert_eq!(
        provider.complete(request()).await.unwrap_err(),
        AiError::NotConfigured
    );
    assert_eq!(count(&server).await, 0);
}

// 검증: E38
#[test]
fn 새_오류의_문장에_자격_글자가_없다() {
    for err in [AiError::InvalidCredential, AiError::InsecureBaseUrl] {
        assert_clean(&err);
        assert!(!err.to_string().is_empty());
    }
}
