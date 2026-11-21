//! 임베딩 공급자 자리(ai.md 10차, 결정 2026-11-21 2 · 15번).
//!
//! 공급자는 [`EmbeddingProvider`] trait 하나로 붙고, [`EmbeddingRegistry`]가 ID로 찾는다.
//! "구조는 다, 구현은 하나씩": 지금 구현은 로컬 Ollama([`crate::OllamaEmbeddingProvider`]) 하나다.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::error::{AiError, RegistryError};

/// 임베딩을 만드는 글의 쓰임.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EmbedPurpose {
    /// 색인할 글.
    Document,
    /// 검색 질의.
    Query,
}

/// 임베딩 요청. 모델 이름은 늘 요청(설정값)에서 받는다. 라이브러리 안에 기본 모델이 없다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddingRequest {
    /// 모델 이름(예: 설정의 `bge-m3`).
    pub model: String,
    /// 벡터로 만들 글. 응답의 `vectors[i]`가 `inputs[i]`의 벡터다.
    pub inputs: Vec<String>,
    /// 글의 쓰임. 공급자마다 다르게 다룰 수 있다(Ollama는 둘을 같게 보낸다).
    pub purpose: EmbedPurpose,
    /// 벡터 길이. `Some(n)`이면 공급자에 그 길이를 달라고 하고, 응답 길이가 `n`이어야 한다.
    pub dimensions: Option<u32>,
}

/// 임베딩에 쓴 양.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct EmbeddingUsage {
    /// 쓴 토큰 수.
    pub total_tokens: u64,
}

/// 임베딩 응답.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingResponse {
    /// `inputs`와 같은 순서 · 같은 개수의 벡터.
    pub vectors: Vec<Vec<f32>>,
    /// 공급자가 답한 모델 이름(없으면 요청의 모델).
    pub model: String,
    /// 쓴 양.
    pub usage: EmbeddingUsage,
}

/// 공급자 한 요청의 상한.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbeddingLimits {
    /// 한 요청에 담는 입력의 최대 개수. 넘으면 나눠 보낸다.
    pub max_inputs_per_request: usize,
    /// 입력 하나의 최대 글자 수(가린 뒤 글로 센다). 넘으면 요청하지 않고 거부한다.
    pub max_input_chars: usize,
}

/// 임베딩 공급자. 새 공급자는 어댑터 하나와 [`EmbeddingRegistry::register`] 한 줄로 붙는다.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// 공급자 ID(등록부의 열쇠, 소문자 영문).
    fn id(&self) -> &'static str;
    /// 이 공급자가 쓰는 `pi-secrets` 키 칸 이름. 키가 없으면 빈 목록.
    fn key_slots(&self) -> &'static [&'static str];
    /// 한 요청의 상한.
    fn limits(&self) -> EmbeddingLimits;
    /// 글들의 벡터를 만든다.
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse, AiError>;
}

/// 임베딩 공급자 등록부. ID로 찾는다.
#[derive(Default)]
pub struct EmbeddingRegistry {
    providers: BTreeMap<&'static str, Arc<dyn EmbeddingProvider>>,
}

impl EmbeddingRegistry {
    /// 빈 등록부.
    pub fn new() -> Self {
        Self::default()
    }

    /// 공급자를 올린다. 같은 ID가 이미 있으면 [`RegistryError::Duplicate`]이고 앞의 것이 남는다.
    pub fn register(&mut self, provider: Arc<dyn EmbeddingProvider>) -> Result<(), RegistryError> {
        let id = provider.id();
        if self.providers.contains_key(id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.providers.insert(id, provider);
        Ok(())
    }

    /// ID로 공급자를 찾는다. 모르는 ID면 `None`.
    pub fn get(&self, id: &str) -> Option<Arc<dyn EmbeddingProvider>> {
        self.providers.get(id).cloned()
    }

    /// 올린 공급자 ID들(이름순).
    pub fn ids(&self) -> Vec<&'static str> {
        self.providers.keys().copied().collect()
    }
}
