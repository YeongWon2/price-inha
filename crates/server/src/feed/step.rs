//! 시세 흐름이 쓰는 작은 것들: 어디서 받는가 · 박자 · 한 번 받은 뒤의 할 일 · 실패를 상태로 옮기기.

use std::sync::Arc;
use std::time::Duration;

use pi_market::{MarketAdapter, MarketError};
use tokio::time::Instant;

use crate::{FEED_BROKER_REJECTED, FEED_BROKER_UNREACHABLE, FEED_REQUEST_REJECTED};

/// 시세를 어디서 받는가.
pub(crate) enum Source {
    /// 시연용 가짜 어댑터. 필수 설정 없이 바로 시작한다.
    Demo(Arc<dyn MarketAdapter>),
    /// 증권사. 필수 설정이 모두 등록돼 있을 때만 받는다.
    Broker(Arc<dyn MarketAdapter>),
}

/// 시세를 받는 박자.
pub(crate) struct Pace {
    /// 시세를 받는 간격.
    pub(crate) interval: Duration,
    /// 실패한 뒤 다시 시도하기까지의 첫 간격.
    pub(crate) retry_initial: Duration,
    /// 다시 시도하는 간격의 상한.
    pub(crate) retry_max: Duration,
    /// 달력을 받지 못했을 때(인증 실패가 아닌 실패) 다시 받기까지의 간격.
    pub(crate) calendar_retry: Duration,
}

/// 한 번 받은 뒤에 할 일.
pub(super) enum Step {
    /// 필수 설정을 다시 본다(증권사 키가 없어졌다).
    Stop,
    /// 기다렸다가 다시 받는다.
    Wait {
        wait: Duration,
        /// 등록이 깨워도 이 시각보다 빨리 다시 시도하지 않는다(증권사가 준 기다림).
        not_before: Option<Instant>,
    },
}

impl Step {
    pub(super) fn wait(wait: Duration) -> Self {
        Step::Wait {
            wait,
            not_before: None,
        }
    }
}

/// 쉬고 난 결과.
#[derive(PartialEq, Eq)]
pub(super) enum Rested {
    /// 다 쉬었다.
    Done,
    /// 등록이 깨웠다.
    Woken,
    /// 필수 설정이 빠졌다.
    NotReady,
}

/// 기록에 적는 밀리초.
pub(super) fn millis(wait: Duration) -> u64 {
    u64::try_from(wait.as_millis()).unwrap_or(u64::MAX)
}

/// 실패를 화면에 알릴 시세 상태로 옮긴다.
///
/// 인증 실패 · 허용 IP 아님은 "증권사가 키를 받지 않음", 4xx 거절(종목 없음 등)은
/// "증권사가 요청을 거절함", 그 밖(연결 실패 · 점검 · 장애 · 한도 · 읽을 수 없는 응답)은
/// "증권사에 닿지 않음"이다.
pub(super) fn failure_state(error: &MarketError) -> &'static str {
    match error {
        MarketError::AuthFailed { .. } | MarketError::IpNotAllowed => FEED_BROKER_REJECTED,
        MarketError::Rejected { .. } => FEED_REQUEST_REJECTED,
        _ => FEED_BROKER_UNREACHABLE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 실패는_키를_받지_않음과_요청_거절과_닿지_않음으로_나눈다() {
        let rejected = [
            MarketError::AuthFailed {
                code: "invalid_client".to_owned(),
            },
            MarketError::IpNotAllowed,
        ];
        for error in rejected {
            assert_eq!(failure_state(&error), FEED_BROKER_REJECTED, "{error}");
        }
        let refused = MarketError::Rejected {
            status: 404,
            code: "stock-not-found".to_owned(),
        };
        assert_eq!(failure_state(&refused), FEED_REQUEST_REJECTED);
        let unreachable = [
            MarketError::Connection {
                detail: "refused".to_owned(),
            },
            MarketError::Unavailable {
                status: 503,
                code: None,
            },
            MarketError::RateLimited { retry_after: None },
        ];
        for error in unreachable {
            assert_eq!(failure_state(&error), FEED_BROKER_UNREACHABLE, "{error}");
        }
    }
}
