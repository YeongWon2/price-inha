//! 다시 시도할지와 얼마나 기다릴지를 정한다.

use std::time::Duration;

use crate::anthropic::AnthropicConfig;
use crate::error::AiError;

/// 늘어나는 간격의 위 끝. 설정한 첫 간격이 이보다 길면 첫 간격을 그대로 쓴다.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// 공급자가 `retry-after`로 이보다 오래 기다리라고 하면 기다리지 않고 오류를 돌려준다.
/// 호출한 쪽이 오류의 `retry_after`를 보고 정한다.
const MAX_RETRY_AFTER_WAIT: Duration = Duration::from_secs(30);

/// `attempt`번째(0부터) 실패 뒤에 다시 시도하려면 기다릴 시간. 다시 시도하지 않으면 `None`.
///
/// 한도 초과와 공급자 장애만 다시 시도한다. 인증 실패 · 잘못된 요청은 다시 보내도
/// 같은 답이고, 시간 초과 · 연결 실패는 요청이 이미 처리됐을 수 있어 다시 보내지 않는다.
pub(crate) fn wait_before_retry(
    config: &AnthropicConfig,
    attempt: u32,
    err: &AiError,
) -> Option<Duration> {
    if attempt >= config.max_retries {
        return None;
    }
    let backoff = backoff(config.retry_base_delay, attempt);
    match err {
        AiError::RateLimited {
            retry_after: Some(after),
        } => (*after <= MAX_RETRY_AFTER_WAIT).then(|| backoff.max(*after)),
        AiError::RateLimited { retry_after: None } | AiError::ProviderUnavailable { .. } => {
            Some(backoff)
        }
        _ => None,
    }
}

/// 시도마다 두 배로 늘어나는 간격. 첫 간격보다 줄지 않는다.
fn backoff(base: Duration, attempt: u32) -> Duration {
    let factor = 1u32 << attempt.min(16);
    base.saturating_mul(factor).min(MAX_BACKOFF.max(base))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(max_retries: u32, base_ms: u64) -> AnthropicConfig {
        AnthropicConfig {
            base_url: "http://127.0.0.1:1".to_string(),
            timeout: Duration::from_secs(1),
            max_retries,
            retry_base_delay: Duration::from_millis(base_ms),
        }
    }

    const DOWN: AiError = AiError::ProviderUnavailable { status: 529 };

    #[test]
    fn 간격은_시도마다_두_배로_늘어난다() {
        let config = config(5, 100);
        let waits: Vec<_> = (0..4)
            .map(|n| wait_before_retry(&config, n, &DOWN).unwrap())
            .collect();
        assert_eq!(
            waits,
            [100, 200, 400, 800].map(Duration::from_millis).to_vec()
        );
    }

    #[test]
    fn 간격은_위_끝에서_멈추고_넘치지_않는다() {
        assert_eq!(backoff(Duration::from_secs(1), 40), MAX_BACKOFF);
        assert_eq!(backoff(Duration::MAX, 3), Duration::MAX);
        // 첫 간격이 위 끝보다 길면 줄이지 않는다.
        assert_eq!(backoff(Duration::from_secs(90), 0), Duration::from_secs(90));
    }

    #[test]
    fn 정한_횟수를_다_쓰면_다시_시도하지_않는다() {
        let config = config(2, 1);
        assert!(wait_before_retry(&config, 1, &DOWN).is_some());
        assert_eq!(wait_before_retry(&config, 2, &DOWN), None);
    }

    #[test]
    fn 한도_초과와_장애만_다시_시도한다() {
        let config = config(2, 1);
        let no_retry = [
            AiError::NotConfigured,
            AiError::Auth { status: 401 },
            AiError::InvalidRequest {
                message: "no".to_string(),
            },
            AiError::Timeout,
            AiError::Connection,
            AiError::InvalidResponse,
        ];
        for err in no_retry {
            assert_eq!(wait_before_retry(&config, 0, &err), None, "{err:?}");
        }
        let limited = AiError::RateLimited { retry_after: None };
        assert!(wait_before_retry(&config, 0, &limited).is_some());
    }

    #[test]
    fn retry_after가_있으면_적어도_그만큼_기다린다() {
        let config = config(2, 1);
        let err = AiError::RateLimited {
            retry_after: Some(Duration::from_secs(7)),
        };
        assert_eq!(
            wait_before_retry(&config, 0, &err),
            Some(Duration::from_secs(7))
        );
        // 설정한 간격이 더 길면 그 간격을 따른다.
        let slow = AnthropicConfig {
            retry_base_delay: Duration::from_secs(20),
            ..config
        };
        assert_eq!(
            wait_before_retry(&slow, 0, &err),
            Some(Duration::from_secs(20))
        );
    }

    #[test]
    fn retry_after가_너무_길면_기다리지_않고_오류를_돌려준다() {
        let config = config(2, 1);
        let err = AiError::RateLimited {
            retry_after: Some(MAX_RETRY_AFTER_WAIT + Duration::from_secs(1)),
        };
        assert_eq!(wait_before_retry(&config, 0, &err), None);
    }
}
