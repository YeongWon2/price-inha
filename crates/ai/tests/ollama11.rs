//! 11차(결정 34): 같은 집 다른 기기의 Ollama는 켜야만 쓴다.
//!
//! - 지금 길(`new` · `with_limits`)은 루프백만 받는다(그대로).
//! - 새 길(`on_private_network`)은 루프백 · 사설망 주소(10/8 · 172.16/12 · 192.168/16 · 169.254/16 · fc00::/7 ·
//!   fe80::/10 · `*.local`)만 받고 공인 주소 · 도메인은 `InsecureBaseUrl`.
//! - 보내는 글은 지금처럼 가린다. 리다이렉트를 따라가지 않는다.
//!
//! 실제 Ollama · 다른 기기를 부르지 않는다: `wiremock`을 루프백에 띄운다(사설망 주소는 만들기만 본다).
//! (11차) 2026-11-27 백로그 작업을 합쳐 잠금.

use std::sync::Arc;
use std::time::Duration;

use pi_ai::{
    ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES, AiError, AnthropicLimits, EmbedPurpose,
    EmbeddingProvider, EmbeddingRequest, OllamaConfig, OllamaEmbeddingProvider, OllamaReach,
    Redactor, ollama_reach,
};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PRIVATE: [&str; 8] = [
    "http://10.0.0.5:11434",
    "http://172.16.3.4:11434",
    "http://172.31.0.2:11434",
    "http://192.168.0.2:11434",
    "http://169.254.7.8:11434",
    "http://[fd00::7]:11434",
    "http://[fe80::2]:11434",
    "http://ollama-box.local:11434",
];

const PUBLIC: [&str; 5] = [
    "http://8.8.8.8:11434",
    "http://172.32.0.1:11434",
    "https://ollama.example:11434",
    "http://[2001:db8::5]:11434",
    "http://192.168.0.2.nip.io:11434",
];

fn config_for(base_url: &str) -> OllamaConfig {
    OllamaConfig {
        base_url: base_url.to_string(),
        timeout: Duration::from_secs(5),
        max_retries: 0,
        retry_base_delay: Duration::from_millis(1),
        max_inputs_per_request: 64,
        max_input_chars: 8_192,
    }
}

fn limits() -> AnthropicLimits {
    AnthropicLimits {
        connect_timeout: Duration::from_secs(2),
        max_response_bytes: ANTHROPIC_DEFAULT_MAX_RESPONSE_BYTES,
        max_retry_delay: Duration::from_millis(20),
        retry_after_unit: Duration::from_millis(1),
        jitter_seed: Some(3),
    }
}

struct HideSecret;

impl Redactor for HideSecret {
    fn redact(&self, text: &str) -> String {
        text.replace("qmzv-lan-secret", "[가림]")
    }
}

// 검증: V23
#[test]
fn 지금_길은_사설망_주소도_받지_않는다() {
    for base in PRIVATE {
        let made = OllamaEmbeddingProvider::with_limits(config_for(base), limits());
        assert!(
            matches!(made, Err(AiError::InsecureBaseUrl)),
            "켜지 않으면 루프백만: {base}"
        );
    }
}

// 검증: V23
#[test]
fn 켜면_사설망_주소만_받고_공인_주소는_거부한다() {
    for base in PRIVATE {
        let made = OllamaEmbeddingProvider::on_private_network(config_for(base), limits());
        assert!(made.is_ok(), "사설망은 받는다: {base}");
        assert!(
            matches!(ollama_reach(base), OllamaReach::PrivateNetwork { .. }),
            "{base}"
        );
    }
    for base in PUBLIC {
        let made = OllamaEmbeddingProvider::on_private_network(config_for(base), limits());
        assert!(
            matches!(made, Err(AiError::InsecureBaseUrl)),
            "공인 주소 · 도메인은 거부: {base}"
        );
        assert_eq!(ollama_reach(base), OllamaReach::Outside, "{base}");
    }
    assert_eq!(
        ollama_reach("http://192.168.0.2:11434"),
        OllamaReach::PrivateNetwork {
            host: "192.168.0.2".to_string()
        }
    );
    assert_eq!(
        ollama_reach("http://localhost:11434"),
        OllamaReach::Loopback
    );
}

// 검증: V23 E38
#[tokio::test]
async fn 켠_길도_보내는_글을_가리고_리다이렉트를_따라가지_않는다() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/embed"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "bge-m3", "embeddings": [[0.5, 0.25]], "prompt_eval_count": 3
        })))
        .mount(&server)
        .await;
    let provider = OllamaEmbeddingProvider::on_private_network(config_for(&server.uri()), limits())
        .expect("루프백은 켠 길에서도 받는다")
        .with_redactor(Arc::new(HideSecret));
    let reply = provider
        .embed(EmbeddingRequest {
            model: "bge-m3".to_string(),
            inputs: vec!["손절 기록 qmzv-lan-secret".to_string()],
            purpose: EmbedPurpose::Document,
            dimensions: None,
        })
        .await
        .expect("임베딩");
    assert_eq!(reply.vectors, vec![vec![0.5, 0.25]]);
    let sent = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&sent[0].body).unwrap();
    assert_eq!(
        body["input"],
        json!(["손절 기록 [가림]"]),
        "가린 글만 나간다"
    );

    let moved = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/embed"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "http://8.8.8.8:11434/api/embed"),
        )
        .mount(&moved)
        .await;
    let provider =
        OllamaEmbeddingProvider::on_private_network(config_for(&moved.uri()), limits()).unwrap();
    let error = provider
        .embed(EmbeddingRequest {
            model: "bge-m3".to_string(),
            inputs: vec!["글".to_string()],
            purpose: EmbedPurpose::Query,
            dimensions: None,
        })
        .await
        .expect_err("리다이렉트는 따라가지 않는다");
    assert_eq!(error, AiError::InvalidResponse);
    assert_eq!(moved.received_requests().await.unwrap().len(), 1);
}
