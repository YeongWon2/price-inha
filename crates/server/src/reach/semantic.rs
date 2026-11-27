//! (10차 10c 13번, 결정 15) 뜻 검색: 설정 "뜻 검색 켜기"(기본 꺼짐)가 켜지면 로컬 Ollama 공급자
//! (`pi_ai::OllamaEmbeddingProvider`)를 임베딩 등록부에 올리고, 감싸는 타입으로 `pi_knowledge::Embedder`를 만들어
//! `Knowledge::with_embedder`에 건다. 기존 기록의 임베딩은 `Knowledge::reembed_step`으로 걸음 간격
//! (`TeamWiring::reembed_interval`, 시계 기준)마다 한 걸음(`reembed_batch`)씩 다시 만든다.
//!
//! - 꺼져 있으면 Ollama에 아무것도 묻지 않는다(V23). 끄면 걸음을 멈추고, 다 만들지 않은 색인을 다 만든 것으로
//!   남기지 않는다(색인 설정은 `reembed_step`이 다 만든 뒤에만 남긴다).
//! - 논의 중에는 걸음을 쉰다(한가할 때만).
//! - 주소는 루프백만 받는다(공급자가 만들 때 거부한다). 설정은 보관 폴더의 [`SEMANTIC_SETTINGS_FILE`]에 남는다.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use pi_ai::{
    AiError, AnthropicLimits, EmbedPurpose, EmbeddingProvider, EmbeddingRegistry, EmbeddingRequest,
    OLLAMA_DEFAULT_BASE_URL, OLLAMA_PROVIDER_ID, OllamaConfig, OllamaEmbeddingProvider, Redactor,
};
use pi_knowledge::{
    CHUNK_OVERLAP, CHUNK_TOKENS, EmbedError, Embedder, IndexSettings, Knowledge, ReembedProgress,
    SearchConfig,
};
use pi_market::Clock;
use pi_store::Store;
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::{DEFAULT_EMBED_MODEL, SEMANTIC_SETTINGS_FILE};

/// 걸음 때를 보는 가장 긴 간격(설정이 바뀌면 바로 깬다).
const POLL: Duration = Duration::from_secs(1);
/// 알려진 모델의 벡터 차원(결정 15번: `bge-m3`은 1024). 모르는 모델은 한 번 물어 안다.
const KNOWN_DIMS: [(&str, u32); 1] = [("bge-m3", 1024)];
/// 색인 설정에 남기는 글 색인 방식 · 범위 이름.
const TOKENIZER: &str = "trigram";
const SCOPE: &str = "trade,review,mistake,market_note";

/// 뜻 검색 설정(비밀이 아니다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemanticSettings {
    pub(crate) enabled: bool,
    pub(crate) base_url: String,
    pub(crate) model: String,
    /// (65차, 결정 34) 같은 집 다른 기기(사설망)의 Ollama를 켰나. 켜야만 사설망 주소를 받는다(기본 꺼짐).
    pub(crate) private_network: bool,
}

impl Default for SemanticSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: OLLAMA_DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_EMBED_MODEL.to_string(),
            private_network: false,
        }
    }
}

impl SemanticSettings {
    /// 몸통 `{"enabled", "base_url", "model"}`을 읽는다. 모양이 틀리면 없음.
    pub(crate) fn from_json(body: &Value) -> Option<SemanticSettings> {
        let base_url = body["base_url"].as_str()?.trim().trim_end_matches('/');
        let model = body["model"].as_str()?.trim();
        if base_url.is_empty() || model.is_empty() {
            return None;
        }
        Some(SemanticSettings {
            enabled: body["enabled"].as_bool()?,
            base_url: base_url.to_string(),
            model: model.to_string(),
            private_network: body["private_network"].as_bool().unwrap_or(false),
        })
    }

    /// (65차) `outside`: 임베딩을 이 컴퓨터 밖(사설망)으로 보내면 그 한 줄([`super::semantic_lan::outside_line`]),
    /// 루프백이면 `null`.
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "enabled": self.enabled,
            "base_url": self.base_url,
            "model": self.model,
            "private_network": self.private_network,
            "outside": super::semantic_lan::outside_line(self),
        })
    }

    /// 보관 폴더의 설정 파일을 읽기만 한다. 없거나 틀리면(루프백이 아닌 주소 포함) 기본(꺼짐)이다.
    pub(crate) fn load(home: &Path) -> SemanticSettings {
        std::fs::read_to_string(home.join(SEMANTIC_SETTINGS_FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|value| SemanticSettings::from_json(&value))
            .filter(|settings| settings.provider().is_ok())
            .unwrap_or_default()
    }

    fn save(&self, home: &Path) -> std::io::Result<()> {
        std::fs::write(
            home.join(SEMANTIC_SETTINGS_FILE),
            self.to_json().to_string(),
        )
    }

    /// 이 주소의 Ollama 공급자(만들 때 아무것도 보내지 않는다). 루프백이 아니면 [`AiError::InsecureBaseUrl`].
    /// (65차, 결정 34) 같은 집 다른 기기를 켰으면 사설망 주소도 받는 공급자다(공인 주소는 그래도 거부).
    pub(crate) fn provider(&self) -> Result<OllamaEmbeddingProvider, AiError> {
        let config = OllamaConfig {
            base_url: self.base_url.clone(),
            ..OllamaConfig::default()
        };
        if self.private_network {
            OllamaEmbeddingProvider::on_private_network(config, AnthropicLimits::default())
        } else {
            OllamaEmbeddingProvider::new(config)
        }
    }
}

/// 설정을 바꾸지 못한 까닭.
#[derive(Debug)]
pub(crate) enum ChangeError {
    /// 주소가 루프백이 아니다.
    NotLoopback,
    /// (65차) 같은 집 다른 기기를 켰는데 주소가 루프백도 사설망도 아니다.
    NotPrivate,
    /// 파일에 남기지 못했다.
    Save(std::io::Error),
}

/// (65차) 공급자를 만들 수 없는 주소의 거부 까닭: 같은 집 다른 기기를 켰으면 [`ChangeError::NotPrivate`].
fn refused(settings: &SemanticSettings) -> ChangeError {
    if settings.private_network {
        ChangeError::NotPrivate
    } else {
        ChangeError::NotLoopback
    }
}

/// 공급자를 `pi_knowledge::Embedder`로 감싼다(모델 · 차원은 정해 둔다).
struct ProviderEmbedder {
    provider: Arc<dyn EmbeddingProvider>,
    model: String,
    dims: u32,
}

#[async_trait]
impl Embedder for ProviderEmbedder {
    fn model(&self) -> &str {
        &self.model
    }

    fn dims(&self) -> u32 {
        self.dims
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        embed_with(self.provider.as_ref(), &self.model, texts.to_vec()).await
    }

    /// (round64) 검색 질의는 질의로 보낸다(공급자가 질의와 색인 글을 달리 다룰 수 있게).
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        let request = EmbeddingRequest {
            model: self.model.clone(),
            inputs: vec![text.to_string()],
            purpose: EmbedPurpose::Query,
            dimensions: None,
        };
        let vectors = embed_request(self.provider.as_ref(), request).await?;
        vectors
            .into_iter()
            .next()
            .ok_or_else(|| EmbedError::Failed("질의에 벡터가 없다".to_string()))
    }
}

async fn embed_with(
    provider: &dyn EmbeddingProvider,
    model: &str,
    inputs: Vec<String>,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    let request = EmbeddingRequest {
        model: model.to_string(),
        inputs,
        purpose: EmbedPurpose::Document,
        dimensions: None,
    };
    embed_request(provider, request).await
}

async fn embed_request(
    provider: &dyn EmbeddingProvider,
    request: EmbeddingRequest,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    match provider.embed(request).await {
        Ok(response) => Ok(response.vectors),
        Err(error @ (AiError::Connection | AiError::Timeout)) => {
            Err(EmbedError::Unavailable(error.to_string()))
        }
        Err(error) => Err(EmbedError::Failed(error.to_string())),
    }
}

/// 임베딩을 건 지식 창구와 그 차원.
type Prepared = (Arc<Knowledge>, u32);

/// 켜져 있을 때의 공급자와(차원을 알면) 임베딩을 건 지식 창구.
#[derive(Clone)]
struct Engine {
    provider: Arc<dyn EmbeddingProvider>,
    model: String,
    prepared: Option<Prepared>,
}

struct Active {
    settings: SemanticSettings,
    engine: Option<Engine>,
}

/// 뜻 검색 설정과 다시 만들기의 상태.
pub(crate) struct Semantic {
    home: PathBuf,
    store: Store,
    redactor: Arc<dyn Redactor>,
    active: Mutex<Active>,
    changed: watch::Sender<u64>,
    /// 지금 논의 중인 수(팀이 센다). 0보다 크면 걸음을 쉰다.
    discussing: Arc<AtomicUsize>,
}

impl Semantic {
    /// 남긴 설정을 읽어 연다(켜져 있으면 공급자를 올린다). 아무 요청도 하지 않는다.
    pub(crate) fn open(
        home: &Path,
        store: Store,
        redactor: Arc<dyn Redactor>,
        discussing: Arc<AtomicUsize>,
    ) -> Semantic {
        let settings = SemanticSettings::load(home);
        let semantic = Semantic {
            home: home.to_path_buf(),
            store,
            redactor,
            active: Mutex::new(Active {
                settings: SemanticSettings::default(),
                engine: None,
            }),
            changed: watch::Sender::new(0),
            discussing,
        };
        if let Err(error) = semantic.apply(settings) {
            tracing::warn!(?error, "남긴 뜻 검색 설정을 쓰지 못했습니다");
        }
        semantic
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Active> {
        self.active.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 지금 설정.
    pub(crate) fn settings(&self) -> SemanticSettings {
        self.lock().settings.clone()
    }

    /// 켜져 있고 차원을 안다면 임베딩을 건 지식 창구.
    pub(crate) fn knowledge(&self) -> Option<Arc<Knowledge>> {
        let active = self.lock();
        let (knowledge, _) = active.engine.as_ref()?.prepared.as_ref()?;
        Some(Arc::clone(knowledge))
    }

    /// 설정을 바꿔 파일에 남긴다. 루프백이 아니면 그대로 둔다.
    pub(crate) fn change(&self, settings: SemanticSettings) -> Result<(), ChangeError> {
        settings.provider().map_err(|_| refused(&settings))?;
        settings.save(&self.home).map_err(ChangeError::Save)?;
        self.apply(settings)
    }

    fn apply(&self, settings: SemanticSettings) -> Result<(), ChangeError> {
        let provider = settings
            .provider()
            .map_err(|_| refused(&settings))?
            .with_redactor(Arc::clone(&self.redactor));
        let engine = settings
            .enabled
            .then(|| self.engine(provider, &settings.model));
        *self.lock() = Active { settings, engine };
        self.changed.send_modify(|n| *n += 1);
        Ok(())
    }

    /// 공급자를 등록부에 올려 꺼낸다. 알려진 모델이면 바로 지식 창구에 건다.
    fn engine(&self, provider: OllamaEmbeddingProvider, model: &str) -> Engine {
        let mut registry = EmbeddingRegistry::new();
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(provider);
        let provider = match registry.register(Arc::clone(&provider)) {
            Ok(()) => registry.get(OLLAMA_PROVIDER_ID).unwrap_or(provider),
            Err(_) => provider,
        };
        let prepared = known_dims(model).map(|dims| (self.attach(&provider, model, dims), dims));
        Engine {
            provider,
            model: model.to_string(),
            prepared,
        }
    }

    fn attach(
        &self,
        provider: &Arc<dyn EmbeddingProvider>,
        model: &str,
        dims: u32,
    ) -> Arc<Knowledge> {
        let embedder = ProviderEmbedder {
            provider: Arc::clone(provider),
            model: model.to_string(),
            dims,
        };
        Arc::new(
            Knowledge::new(self.store.clone(), SearchConfig::default())
                .with_embedder(Arc::new(embedder)),
        )
    }

    /// 걸음을 걸을 공급자(켜져 있고 논의 중이 아닐 때).
    fn ready(&self) -> Option<Engine> {
        if self.discussing.load(Ordering::SeqCst) > 0 {
            return None;
        }
        self.lock().engine.clone()
    }

    /// 다시 만들기 한 걸음. 모르는 모델이면 먼저 한 번 물어 차원을 안다.
    async fn step(&self, engine: Engine, batch: usize) {
        let prepared = match engine.prepared {
            Some(prepared) => Some(prepared),
            None => self.learn_dims(&engine).await,
        };
        let Some((knowledge, dims)) = prepared else {
            return;
        };
        let current = index_settings(&engine.model, dims);
        match knowledge.reembed_step(&current, batch).await {
            Ok(ReembedProgress::InProgress { embedded }) => {
                tracing::debug!(embedded, "임베딩을 다시 만드는 중입니다");
            }
            Ok(ReembedProgress::Done { embedded, .. }) if embedded > 0 => {
                tracing::info!("뜻 검색 색인을 다 만들었습니다");
            }
            Ok(_) => {}
            Err(error) => {
                let text = self.redactor.redact(&error.to_string());
                tracing::warn!(error = %text, "임베딩을 다시 만들지 못했습니다");
            }
        }
    }

    /// 모르는 모델의 차원을 한 번 물어 알고, 설정이 그대로면 지식 창구에 건다.
    async fn learn_dims(&self, engine: &Engine) -> Option<Prepared> {
        let probe = vec!["dimension".to_string()];
        let vectors = match embed_with(engine.provider.as_ref(), &engine.model, probe).await {
            Ok(vectors) => vectors,
            Err(error) => {
                tracing::warn!(error = %self.redactor.redact(&error.to_string()), "임베딩 차원을 알지 못했습니다");
                return None;
            }
        };
        let dims = u32::try_from(vectors.first()?.len())
            .ok()
            .filter(|d| *d > 0)?;
        let knowledge = self.attach(&engine.provider, &engine.model, dims);
        let mut active = self.lock();
        let same = active
            .engine
            .as_ref()
            .is_some_and(|e| Arc::ptr_eq(&e.provider, &engine.provider));
        if !same {
            return None;
        }
        let prepared = (knowledge, dims);
        if let Some(current) = active.engine.as_mut() {
            current.prepared = Some(prepared.clone());
        }
        Some(prepared)
    }

    /// 걸음 간격(시계 기준)마다 한 걸음씩 다시 만든다. 꺼져 있으면 아무것도 묻지 않는다. `stop`이 참이 되면 끝난다.
    pub(crate) fn spawn(
        self: &Arc<Self>,
        clock: Arc<dyn Clock>,
        (interval, batch): (Duration, usize),
        mut stop: watch::Receiver<bool>,
    ) -> JoinHandle<()> {
        let me = Arc::clone(self);
        let mut changed = self.changed.subscribe();
        let interval = chrono::Duration::from_std(interval).unwrap_or(chrono::Duration::MAX);
        tokio::spawn(async move {
            let mut last: Option<chrono::DateTime<chrono::Utc>> = None;
            loop {
                if *stop.borrow() {
                    return;
                }
                let now = clock.now();
                match me.ready() {
                    Some(engine) if last.is_none_or(|at| now - at >= interval) => {
                        last = Some(now);
                        // (round64) 설정이 바뀌면(끄기 포함) 진행 중인 걸음도 끊는다 — 끈 뒤에 답이 와도 두지 않는다.
                        tokio::select! {
                            _ = stop.wait_for(|stop| *stop) => return,
                            _ = changed.changed() => {
                                tracing::debug!("뜻 검색 설정이 바뀌어 다시 만들기 걸음을 끊었다");
                                last = None;
                                continue;
                            }
                            () = me.step(engine, batch) => {}
                        }
                    }
                    Some(_) => {}
                    None if me.lock().engine.is_none() => last = None,
                    None => {}
                }
                tokio::select! {
                    _ = stop.wait_for(|stop| *stop) => return,
                    _ = changed.changed() => {}
                    () = clock.sleep(POLL) => {}
                }
            }
        })
    }
}

/// 알려진 모델의 차원(`bge-m3` 또는 `bge-m3:<태그>`).
fn known_dims(model: &str) -> Option<u32> {
    let name = model.split(':').next().unwrap_or(model);
    KNOWN_DIMS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, dims)| *dims)
}

/// 색인 설정(Ollama · 그 모델 · 그 차원 · 조각 설정).
fn index_settings(model: &str, dims: u32) -> IndexSettings {
    IndexSettings {
        provider: OLLAMA_PROVIDER_ID.to_string(),
        model: model.to_string(),
        dims: Some(dims),
        chunk_tokens: CHUNK_TOKENS,
        chunk_overlap: CHUNK_OVERLAP,
        tokenizer: TOKENIZER.to_string(),
        scope_hash: SCOPE.to_string(),
    }
}
