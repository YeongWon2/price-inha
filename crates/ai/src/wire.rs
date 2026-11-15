//! Anthropic Messages API와 주고받는 JSON 모양, 그리고 응답을 이 모듈의 값으로 옮기는 일.

use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::credential::{Credential, REDACTED};
use crate::error::AiError;
use crate::retry;
use crate::types::{CompletionRequest, CompletionResponse, Role, StopReason, Usage};

/// 오류에 담는 공급자 설명의 최대 길이(글자 수).
const MAX_MESSAGE_CHARS: usize = 500;

/// `POST /v1/messages`의 본문. 자격은 여기에 들어가지 않는다.
#[derive(Serialize)]
pub(crate) struct MessagesBody<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    messages: Vec<WireMessage<'a>>,
}

#[derive(Serialize)]
struct WireMessage<'a> {
    role: &'static str,
    content: &'a str,
}

impl<'a> MessagesBody<'a> {
    /// 요청을 본문 모양으로 옮긴다.
    pub(crate) fn from_request(request: &'a CompletionRequest) -> Self {
        Self {
            model: &request.model,
            max_tokens: request.max_tokens,
            system: request.system.as_deref(),
            messages: request
                .messages
                .iter()
                .map(|m| WireMessage {
                    role: match m.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                    },
                    content: &m.content,
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
struct WireResponse {
    content: Vec<WireBlock>,
    stop_reason: String,
    usage: WireUsage,
}

/// 내용 블록. `text`가 아닌 종류(thinking 등)는 종류만 읽고 넘긴다.
#[derive(Deserialize)]
struct WireBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct WireUsage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Deserialize)]
struct WireError {
    error: WireErrorDetail,
}

#[derive(Deserialize)]
struct WireErrorDetail {
    message: String,
}

/// 성공 응답의 본문을 읽는다. 읽을 수 없는 모양이면 [`AiError::InvalidResponse`].
pub(crate) fn parse_completion(body: &[u8]) -> Result<CompletionResponse, AiError> {
    let wire: WireResponse = serde_json::from_slice(body).map_err(|_| AiError::InvalidResponse)?;
    let text = wire
        .content
        .into_iter()
        .filter(|block| block.kind == "text")
        .filter_map(|block| block.text)
        .collect();
    Ok(CompletionResponse {
        text,
        stop_reason: stop_reason(wire.stop_reason),
        usage: Usage {
            input_tokens: wire.usage.input_tokens,
            output_tokens: wire.usage.output_tokens,
        },
    })
}

pub(crate) fn stop_reason(wire: String) -> StopReason {
    match wire.as_str() {
        "end_turn" => StopReason::EndTurn,
        "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        "tool_use" => StopReason::ToolUse,
        "pause_turn" => StopReason::PauseTurn,
        "refusal" => StopReason::Refusal,
        "model_context_window_exceeded" => StopReason::ModelContextWindowExceeded,
        _ => StopReason::Other(wire),
    }
}

/// 오류 본문의 `retry_after`(A18, 원본 `provider_retry_after_seconds`).
///
/// 본문에 `error` 객체가 있으면 그 안의 값을, 없으면 맨 위의 값을 본다. 숫자(초)나
/// 글자(`Retry-After`와 같은 모양)를 읽는다. 읽을 수 없거나 기다릴 시간이 없으면 `None`.
pub(crate) fn body_retry_after(body: &[u8], now: SystemTime) -> Option<Duration> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let holder = match value.get("error") {
        Some(error) if error.is_object() => error,
        _ => &value,
    };
    match holder.get("retry_after")? {
        Value::Number(n) => retry::seconds_to_wait(n.as_f64()?),
        Value::String(s) => retry::parse_retry_after(s, now),
        _ => None,
    }
}

/// 키 확인 응답이 공식 문서의 모델 목록 모양(`data` 배열)인지 본다.
pub(crate) fn check_model_list(body: &[u8]) -> Result<(), AiError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| AiError::InvalidResponse)?;
    if value.get("data").is_some_and(Value::is_array) {
        Ok(())
    } else {
        Err(AiError::InvalidResponse)
    }
}

/// 성공이 아닌 응답을 상태 코드로 구분한다. 본문이 JSON이 아니어도 구분한다.
///
/// 공급자의 설명은 잘못된 요청에만 담고, 그 안의 자격 글자는 가린다(E38).
pub(crate) fn error_from_status(
    status: u16,
    retry_after: Option<Duration>,
    body: &[u8],
    credential: &Credential,
) -> AiError {
    match status {
        401 | 403 => AiError::Auth { status },
        429 => AiError::RateLimited { retry_after },
        500..=599 => AiError::ProviderUnavailable { status },
        // 400과 그 밖의 4xx(404 없는 모델, 413 너무 큰 요청 등)는 다시 보내도 같은 답이다.
        400..=499 => AiError::InvalidRequest {
            message: provider_message(status, body, credential),
        },
        // 따라가지 않은 3xx 등: 성공도 문서에 있는 오류도 아니다.
        _ => AiError::InvalidResponse,
    }
}

fn provider_message(status: u16, body: &[u8], credential: &Credential) -> String {
    let message = match serde_json::from_slice::<WireError>(body) {
        Ok(wire) => wire.error.message,
        Err(_) => return format!("상태 {status}"),
    };
    redact(&message, credential)
        .chars()
        .take(MAX_MESSAGE_CHARS)
        .collect()
}

/// 글에서 자격 글자가 있는 자리만 가린다.
fn redact(text: &str, credential: &Credential) -> String {
    let secret = credential.secret().expose();
    if secret.is_empty() {
        return text.to_string();
    }
    text.replace(secret, REDACTED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::Secret;

    const KEY: &str = "SIHEOM-DANWI-KEY-0002";

    fn key() -> Credential {
        Credential::ApiKey(Secret::new(KEY))
    }

    fn error_json(message: &str) -> Vec<u8> {
        serde_json::json!({"type": "error", "error": {"type": "x", "message": message}})
            .to_string()
            .into_bytes()
    }

    #[test]
    fn 설명에_키가_여러_번_나와도_모두_가린다() {
        let body = error_json(&format!("{KEY} and again {KEY}."));
        let err = error_from_status(400, None, &body, &key());
        assert_eq!(
            err,
            AiError::InvalidRequest {
                message: format!("{REDACTED} and again {REDACTED}.")
            }
        );
    }

    #[test]
    fn 빈_자격이면_설명을_그대로_둔다() {
        let empty = Credential::ApiKey(Secret::new(""));
        assert_eq!(redact("그대로", &empty), "그대로");
    }

    #[test]
    fn 긴_설명은_잘라서_담는다() {
        let body = error_json(&"가".repeat(MAX_MESSAGE_CHARS * 3));
        match error_from_status(400, None, &body, &key()) {
            AiError::InvalidRequest { message } => {
                assert_eq!(message.chars().count(), MAX_MESSAGE_CHARS);
            }
            other => panic!("잘못된 요청이 아니다: {other:?}"),
        }
    }

    #[test]
    fn 그_밖의_4xx는_잘못된_요청이고_본문이_json이_아니면_상태만_담는다() {
        let err = error_from_status(404, None, b"<html>", &key());
        assert_eq!(
            err,
            AiError::InvalidRequest {
                message: "상태 404".to_string()
            }
        );
    }

    #[test]
    fn 성공도_오류도_아닌_상태는_읽지_못한_응답이다() {
        assert_eq!(
            error_from_status(302, None, b"", &key()),
            AiError::InvalidResponse
        );
    }

    #[test]
    fn 본문의_retry_after는_숫자와_글자를_읽고_error_안을_먼저_본다() {
        let now = SystemTime::now();
        let read = |v: serde_json::Value| body_retry_after(v.to_string().as_bytes(), now);
        assert_eq!(
            read(serde_json::json!({"error": {"retry_after": 1.5}, "retry_after": 9})),
            Some(Duration::from_millis(1500))
        );
        assert_eq!(
            read(serde_json::json!({"retry_after": "12"})),
            Some(Duration::from_secs(12))
        );
        assert_eq!(
            read(serde_json::json!({"error": {}, "retry_after": 9})),
            None
        );
        assert_eq!(read(serde_json::json!({"retry_after": -1})), None);
        assert_eq!(read(serde_json::json!({"retry_after": [1]})), None);
        assert_eq!(body_retry_after(b"<html>", now), None);
    }

    #[test]
    fn 필수_자리가_없는_성공_응답은_읽지_못한다() {
        let body = br#"{"content": [], "usage": {"input_tokens": 1, "output_tokens": 2}}"#;
        assert_eq!(parse_completion(body), Err(AiError::InvalidResponse));
    }
}
