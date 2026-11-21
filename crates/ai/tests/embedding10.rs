//! 10차: 임베딩 공급자 trait과 등록부(ai.md 10차 1~3, 결정 2026-11-21 2번).

use std::sync::Arc;

use async_trait::async_trait;
use pi_ai::{
    AiError, EmbedPurpose, EmbeddingLimits, EmbeddingProvider, EmbeddingRegistry, EmbeddingRequest,
    EmbeddingResponse, EmbeddingUsage, OLLAMA_PROVIDER_ID, OllamaConfig, OllamaEmbeddingProvider,
    RegistryError,
};

/// 시험용 공급자. `limits`의 개수 상한을 표식으로 쓴다.
struct FakeProvider {
    id: &'static str,
    mark: usize,
}

#[async_trait]
impl EmbeddingProvider for FakeProvider {
    fn id(&self) -> &'static str {
        self.id
    }

    fn key_slots(&self) -> &'static [&'static str] {
        &["siheom-kan"]
    }

    fn limits(&self) -> EmbeddingLimits {
        EmbeddingLimits {
            max_inputs_per_request: self.mark,
            max_input_chars: 10,
        }
    }

    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, AiError> {
        Ok(EmbeddingResponse {
            vectors: request.inputs.iter().map(|_| vec![0.0]).collect(),
            model: request.model,
            usage: EmbeddingUsage::default(),
        })
    }
}

fn fake(id: &'static str, mark: usize) -> Arc<dyn EmbeddingProvider> {
    Arc::new(FakeProvider { id, mark })
}

// 검증: D10
#[test]
fn 같은_id를_두_번_올리면_중복이고_앞의_것이_남는다() {
    let mut registry = EmbeddingRegistry::new();
    registry.register(fake("siheom", 1)).unwrap();

    let err = registry.register(fake("siheom", 2)).unwrap_err();

    assert_eq!(err, RegistryError::Duplicate("siheom"));
    let kept = registry.get("siheom").unwrap();
    assert_eq!(kept.limits().max_inputs_per_request, 1);
    assert_eq!(registry.ids(), vec!["siheom"]);
}

// 검증: D10
#[test]
fn 모르는_id는_없다() {
    let mut registry = EmbeddingRegistry::new();
    registry.register(fake("siheom", 1)).unwrap();

    assert!(registry.get("moreum").is_none());
    assert!(EmbeddingRegistry::new().get("siheom").is_none());
}

// 검증: D10
#[test]
fn ids는_이름순이다() {
    let mut registry = EmbeddingRegistry::new();
    assert!(registry.ids().is_empty());
    registry.register(fake("zeta", 1)).unwrap();
    registry.register(fake("alpha", 2)).unwrap();
    registry.register(fake("mid", 3)).unwrap();

    assert_eq!(registry.ids(), vec!["alpha", "mid", "zeta"]);
    assert_eq!(
        registry.get("mid").unwrap().limits().max_inputs_per_request,
        3
    );
}

// 검증: D10, V23
#[test]
fn ollama_공급자는_ollama_id로_키_칸_없이_설정의_상한으로_올라간다() {
    let config = OllamaConfig {
        max_inputs_per_request: 7,
        max_input_chars: 99,
        ..OllamaConfig::default()
    };
    let provider = OllamaEmbeddingProvider::new(config).unwrap();

    assert_eq!(OLLAMA_PROVIDER_ID, "ollama");
    assert_eq!(provider.id(), OLLAMA_PROVIDER_ID);
    assert!(provider.key_slots().is_empty());
    assert_eq!(
        provider.limits(),
        EmbeddingLimits {
            max_inputs_per_request: 7,
            max_input_chars: 99,
        }
    );

    let mut registry = EmbeddingRegistry::new();
    registry.register(Arc::new(provider)).unwrap();
    assert_eq!(registry.ids(), vec!["ollama"]);
    let found = registry.get("ollama").unwrap();
    assert_eq!(found.id(), "ollama");
}

// 검증: D10
#[tokio::test]
async fn 등록부로_찾은_공급자로_임베딩을_부른다() {
    let mut registry = EmbeddingRegistry::new();
    registry.register(fake("siheom", 1)).unwrap();
    let provider = registry.get("siheom").unwrap();

    let response = provider
        .embed(EmbeddingRequest {
            model: "siheom-model".to_string(),
            inputs: vec!["가".to_string(), "나".to_string()],
            purpose: EmbedPurpose::Query,
            dimensions: None,
        })
        .await
        .unwrap();

    assert_eq!(response.vectors.len(), 2);
    assert_eq!(provider.key_slots(), &["siheom-kan"]);
}
