//! Ollama API와 주고받는 JSON 모양(공식 문서 `docs/api.md`의 "Generate Embeddings" ·
//! "List Local Models", `docs/api/errors.mdx`).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AiError;
use crate::redact::Redactor;

/// 오류에 담는 공급자 설명의 최대 글자 수.
const MAX_MESSAGE_CHARS: usize = 500;

/// 오류 문장에 담는 모델 이름의 최대 글자 수.
const MAX_MODEL_CHARS: usize = 200;

/// `POST /api/embed`의 본문. `truncate`는 보내지 않는다(길이는 보내기 전에 막는다).
#[derive(Serialize)]
pub(crate) struct EmbedBody<'a> {
    pub(crate) model: &'a str,
    pub(crate) input: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dimensions: Option<u32>,
}

/// `POST /api/embed`의 성공 응답에서 읽는 자리.
#[derive(Deserialize)]
struct EmbedReply {
    #[serde(default)]
    model: Option<String>,
    embeddings: Vec<Vec<f32>>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
}

/// 한 요청(조각)의 읽은 응답.
pub(crate) struct EmbedChunk {
    pub(crate) vectors: Vec<Vec<f32>>,
    pub(crate) model: Option<String>,
    pub(crate) total_tokens: u64,
}

/// 임베딩 응답을 읽는다.
///
/// 벡터 개수가 `expected`와 같아야 하고, 모든 벡터가 비어 있지 않은 같은 길이의 유한한 수여야 하며,
/// `dimensions`가 `Some(n)`이면 길이가 `n`이어야 한다. 하나라도 어긋나면 [`AiError::InvalidResponse`].
pub(crate) fn parse_embed(
    body: &[u8],
    expected: usize,
    dimensions: Option<u32>,
) -> Result<EmbedChunk, AiError> {
    let reply: EmbedReply = serde_json::from_slice(body).map_err(|_| AiError::InvalidResponse)?;
    if reply.embeddings.len() != expected {
        return Err(AiError::InvalidResponse);
    }
    let length = reply.embeddings.first().map_or(0, Vec::len);
    let wanted = match dimensions {
        Some(n) => usize::try_from(n).map_err(|_| AiError::InvalidResponse)?,
        None => length,
    };
    let sound = reply.embeddings.iter().all(|vector| {
        !vector.is_empty() && vector.len() == wanted && vector.iter().all(|x| x.is_finite())
    });
    if !sound {
        return Err(AiError::InvalidResponse);
    }
    Ok(EmbedChunk {
        vectors: reply.embeddings,
        model: reply.model.filter(|name| !name.is_empty()),
        total_tokens: reply.prompt_eval_count.unwrap_or(0),
    })
}

/// `GET /api/tags` 응답에 `model` 또는 `model:latest`라는 이름이 있는지 본다.
///
/// 본문이 모델 목록 모양(`models` 배열)이 아니면 [`AiError::InvalidResponse`].
pub(crate) fn has_model(body: &[u8], model: &str) -> Result<bool, AiError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| AiError::InvalidResponse)?;
    let models = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or(AiError::InvalidResponse)?;
    let latest = format!("{model}:latest");
    Ok(models
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .any(|name| name == model || name == latest))
}

/// 모델을 받아 두지 않았을 때의 오류. 문장에 `ollama pull <모델>`을 안내한다.
pub(crate) fn model_missing(model: &str, redactor: &dyn Redactor) -> AiError {
    let model: String = redactor
        .redact(model)
        .chars()
        .take(MAX_MODEL_CHARS)
        .collect();
    AiError::InvalidRequest {
        message: format!(
            "Ollama에 모델 `{model}`이 없습니다. 터미널에서 `ollama pull {model}`로 받아 두세요"
        ),
    }
}

/// 성공이 아닌 응답을 구분한다(`docs/api/errors.mdx`: 본문은 `{"error": "<설명>"}`).
///
/// 404이거나 설명이 "모델 없음"이면 `ollama pull` 안내가 든 [`AiError::InvalidRequest`],
/// 그 밖의 4xx는 설명을 가려 담은 [`AiError::InvalidRequest`], 429는 [`AiError::RateLimited`],
/// 5xx는 [`AiError::ProviderUnavailable`], 따라가지 않은 3xx 등은 [`AiError::InvalidResponse`].
pub(crate) fn error_from_status(
    status: u16,
    retry_after: Option<Duration>,
    body: &[u8],
    model: &str,
    redactor: &dyn Redactor,
) -> AiError {
    let described = error_text(body);
    if status == 404 || described.as_deref().is_some_and(says_model_missing) {
        return model_missing(model, redactor);
    }
    match status {
        401 | 403 => AiError::Auth { status },
        429 => AiError::RateLimited { retry_after },
        500..=599 => AiError::ProviderUnavailable { status },
        400..=499 => AiError::InvalidRequest {
            message: match described {
                Some(text) => redactor
                    .redact(&text)
                    .chars()
                    .take(MAX_MESSAGE_CHARS)
                    .collect(),
                None => format!("상태 {status}"),
            },
        },
        _ => AiError::InvalidResponse,
    }
}

/// 오류 본문의 `error` 글.
fn error_text(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    value.get("error")?.as_str().map(str::to_string)
}

/// Ollama가 모델이 없을 때 주는 설명인가(`model "<이름>" not found, try pulling it first`).
fn says_model_missing(text: &str) -> bool {
    let text = text.to_lowercase();
    text.contains("model") && text.contains("not found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redact::NoRedaction;

    struct MaskKey;

    impl Redactor for MaskKey {
        fn redact(&self, text: &str) -> String {
            text.replace("SIHEOM-KEY", "***")
        }
    }

    fn error_body(text: &str) -> Vec<u8> {
        serde_json::json!({ "error": text })
            .to_string()
            .into_bytes()
    }

    #[test]
    fn 본문이_모델_없음이면_상태가_달라도_pull을_안내한다() {
        let body = error_body("model \"bge-m3\" not found, try pulling it first");
        for status in [400, 404, 500] {
            let err = error_from_status(status, None, &body, "bge-m3", &NoRedaction);
            assert!(
                err.to_string().contains("ollama pull bge-m3"),
                "{status}: {err}"
            );
            assert!(matches!(err, AiError::InvalidRequest { .. }));
        }
    }

    #[test]
    fn 상태로_오류를_구분한다() {
        let body = error_body("busy");
        let classify = |status| error_from_status(status, None, &body, "m", &NoRedaction);
        assert_eq!(classify(429), AiError::RateLimited { retry_after: None });
        assert_eq!(classify(500), AiError::ProviderUnavailable { status: 500 });
        assert_eq!(classify(401), AiError::Auth { status: 401 });
        assert_eq!(classify(307), AiError::InvalidResponse);
        assert_eq!(
            classify(400),
            AiError::InvalidRequest {
                message: "busy".to_string()
            }
        );
        assert_eq!(
            error_from_status(413, None, b"<html>", "m", &NoRedaction),
            AiError::InvalidRequest {
                message: "상태 413".to_string()
            }
        );
    }

    #[test]
    fn 공급자_설명은_가리고_잘라서_담는다() {
        let body = error_body(&format!(
            "SIHEOM-KEY {}",
            "가".repeat(MAX_MESSAGE_CHARS * 2)
        ));
        match error_from_status(400, None, &body, "m", &MaskKey) {
            AiError::InvalidRequest { message } => {
                assert!(!message.contains("SIHEOM-KEY"), "{message}");
                assert_eq!(message.chars().count(), MAX_MESSAGE_CHARS);
            }
            other => panic!("잘못된 요청이 아니다: {other:?}"),
        }
    }

    #[test]
    fn 빈_벡터와_넘치는_수는_읽지_못한_응답이다() {
        for body in [
            r#"{"embeddings": [[]]}"#,
            r#"{"embeddings": [[1e39]]}"#,
            r#"{"embeddings": [[0.1]], "prompt_eval_count": "가"}"#,
            "<html>",
        ] {
            assert_eq!(
                parse_embed(body.as_bytes(), 1, None).err(),
                Some(AiError::InvalidResponse),
                "{body}"
            );
        }
    }

    #[test]
    fn 빈_model_칸은_없는_것으로_본다() {
        let chunk = parse_embed(br#"{"model": "", "embeddings": [[0.5]]}"#, 1, Some(1)).unwrap();
        assert_eq!(chunk.model, None);
        assert_eq!(chunk.total_tokens, 0);
    }

    #[test]
    fn 목록_모양이_아니면_읽지_못한_응답이다() {
        assert_eq!(has_model(b"{}", "m"), Err(AiError::InvalidResponse));
        assert_eq!(
            has_model(br#"{"models": {}}"#, "m"),
            Err(AiError::InvalidResponse)
        );
        assert_eq!(
            has_model(br#"{"models": [{"size": 1}, {"name": "m:latest"}]}"#, "m"),
            Ok(true)
        );
        assert_eq!(
            has_model(br#"{"models": [{"name": "m:q4"}]}"#, "m"),
            Ok(false)
        );
    }
}
