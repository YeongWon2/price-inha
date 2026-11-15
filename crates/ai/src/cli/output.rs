//! `stream-json` 출력 읽기와 오류 옮기기(ai.md 2차 6 · 7, `cli-backend.md` 2.4 · 5.4).
//!
//! 오류 문장에는 출력 원문을 담지 않는다. 정해 둔 글과 상태 번호, 아는 오류 이름만 담는다(E38).

use std::process::ExitStatus;

use serde_json::Value;

use crate::error::AiError;
use crate::types::{CompletionResponse, StopReason, Usage};
use crate::wire::stop_reason;

/// 오류에 담는 표준 오류 첫 줄의 최대 글자 수.
const STDERR_LINE_CHARS: usize = 200;

/// 표준 출력에서 읽어 둔 것.
#[derive(Default)]
pub(super) struct StreamState {
    /// 마지막 `type == "result"` 줄.
    pub(super) result: Option<Value>,
    /// 마지막 `system/api_retry` 줄의 `error`.
    pub(super) last_retry: Option<String>,
}

impl StreamState {
    /// 한 줄을 읽는다. 실패 결과 줄이면 참(그 결과가 우선이다 — 원본 2.7).
    pub(super) fn feed(&mut self, line: &[u8]) -> bool {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            return false;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("result") => {
                let failed = is_error(&value);
                self.result = Some(value);
                failed
            }
            Some("system") if value.get("subtype").and_then(Value::as_str) == Some("api_retry") => {
                self.last_retry = value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                false
            }
            _ => false,
        }
    }
}

fn is_error(result: &Value) -> bool {
    result.get("is_error").and_then(Value::as_bool) == Some(true)
}

/// 실패 결과 줄이 나온 즉시 낼 오류.
pub(super) fn early_failure(state: &StreamState) -> AiError {
    match &state.result {
        Some(result) => classify(result, state.last_retry.as_deref()),
        None => AiError::InvalidResponse,
    }
}

/// 프로세스가 끝난 뒤 결과를 낸다.
pub(super) fn finish(
    state: &StreamState,
    status: ExitStatus,
    stderr: &[u8],
) -> Result<CompletionResponse, AiError> {
    if let Some(result) = &state.result
        && (is_error(result) || (status.code().is_some() && !status.success()))
    {
        return Err(classify(result, state.last_retry.as_deref()));
    }
    let Some(code) = status.code() else {
        return Err(AiError::Connection);
    };
    let Some(result) = &state.result else {
        if code == 0 {
            return Err(AiError::InvalidResponse);
        }
        return Err(AiError::InvalidRequest {
            message: stderr_first_line(stderr, code),
        });
    };
    success(result)
}

fn success(result: &Value) -> Result<CompletionResponse, AiError> {
    let text = result
        .get("result")
        .and_then(Value::as_str)
        .ok_or(AiError::InvalidResponse)?;
    let denied = match result.get("permission_denials") {
        None | Some(Value::Null) => false,
        Some(Value::Array(items)) => !items.is_empty(),
        Some(_) => true,
    };
    let turns = result.get("num_turns").and_then(Value::as_u64).unwrap_or(1);
    if denied || turns > 1 {
        return Err(AiError::InvalidResponse);
    }
    let stop_reason = match result.get("stop_reason").and_then(Value::as_str) {
        Some(raw) => stop_reason(raw.to_string()),
        None => StopReason::Other(String::new()),
    };
    let usage = result.get("usage").ok_or(AiError::InvalidResponse)?;
    let tokens = |name: &str| usage.get(name).and_then(Value::as_u64);
    let (Some(input_tokens), Some(output_tokens)) =
        (tokens("input_tokens"), tokens("output_tokens"))
    else {
        return Err(AiError::InvalidResponse);
    };
    Ok(CompletionResponse {
        text: text.to_string(),
        stop_reason,
        usage: Usage {
            input_tokens,
            output_tokens,
        },
    })
}

/// 실패 결과를 상태 → 결과 글 → 마지막 `api_retry` 순서로 옮긴다.
pub(super) fn classify(result: &Value, last_retry: Option<&str>) -> AiError {
    if let Some(err) = result
        .get("api_error_status")
        .and_then(Value::as_u64)
        .and_then(from_status)
    {
        return err;
    }
    if let Some(err) = result
        .get("result")
        .and_then(Value::as_str)
        .and_then(from_text)
    {
        return err;
    }
    last_retry
        .and_then(from_retry)
        .unwrap_or(AiError::InvalidResponse)
}

fn from_status(status: u64) -> Option<AiError> {
    let code = u16::try_from(status).ok()?;
    match code {
        401 | 403 => Some(AiError::Auth { status: code }),
        429 => Some(AiError::RateLimited { retry_after: None }),
        400 | 404 | 413 => Some(AiError::InvalidRequest {
            message: format!("CLI가 상태 {code}를 받았습니다"),
        }),
        c if c >= 500 => Some(AiError::ProviderUnavailable { status: code }),
        _ => None,
    }
}

fn from_text(text: &str) -> Option<AiError> {
    if text.contains("Not logged in") || text.contains("Login expired") {
        return Some(AiError::Auth { status: 401 });
    }
    if text.contains("hit your") && text.contains("limit") {
        return Some(AiError::RateLimited { retry_after: None });
    }
    if text.contains("another Claude Code process is refreshing") {
        return Some(AiError::ProviderUnavailable { status: 503 });
    }
    None
}

fn from_retry(error: &str) -> Option<AiError> {
    match error {
        "authentication_failed" => Some(AiError::Auth { status: 401 }),
        "oauth_org_not_allowed" | "account_on_hold" | "billing_error" => {
            Some(AiError::Auth { status: 403 })
        }
        "rate_limit" => Some(AiError::RateLimited { retry_after: None }),
        "overloaded" => Some(AiError::ProviderUnavailable { status: 529 }),
        "server_error" => Some(AiError::ProviderUnavailable { status: 500 }),
        "invalid_request" | "model_not_found" => Some(AiError::InvalidRequest {
            message: format!("CLI가 {error} 오류를 받았습니다"),
        }),
        _ => None,
    }
}

/// 표준 오류의 첫 줄을 200자로 자른 것. 비어 있으면 끝 코드만 말한다.
fn stderr_first_line(stderr: &[u8], code: i32) -> String {
    let text = String::from_utf8_lossy(stderr);
    let line = text.lines().next().unwrap_or("").trim_end();
    if line.is_empty() {
        return format!("CLI가 끝 코드 {code}로 끝났습니다");
    }
    line.chars().take(STDERR_LINE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn 모르는_상태는_결과_글로_넘어간다() {
        let result = json!({"is_error": true, "api_error_status": 402, "result": "Login expired"});
        assert_eq!(classify(&result, None), AiError::Auth { status: 401 });
    }

    #[test]
    fn 비어_있는_표준_오류는_끝_코드만_말한다() {
        assert_eq!(stderr_first_line(b"", 3), "CLI가 끝 코드 3로 끝났습니다");
        assert_eq!(stderr_first_line(b"\xff\nok", 3), "\u{fffd}");
    }
}
