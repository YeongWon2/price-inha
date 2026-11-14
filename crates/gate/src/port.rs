//! 바깥에서 끼우는 자리: 시계와 가리기.

use std::time::{SystemTime, UNIX_EPOCH};

use pi_store::UnixMillis;

/// 시계. 접속 기록과 이벤트의 시각에 쓴다. 테스트가 시각을 정한다.
pub trait Clock: Send + Sync {
    /// 지금 시각.
    fn now(&self) -> UnixMillis;
}

/// 실제 시계(시스템 시각).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UnixMillis {
        // 시계가 1970년보다 앞이면 0으로, i64를 넘으면 가장 큰 값으로 둔다.
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            });
        UnixMillis(millis)
    }
}

/// 가리기 자리. 화면으로 나가는 글이 모두 이것을 거친다.
pub trait Redactor: Send + Sync {
    /// 글에서 비밀 값을 가린 글을 돌려준다. 가릴 것이 없으면 그대로 돌려준다.
    fn redact(&self, text: &str) -> String;
}

/// 아무것도 가리지 않는다(기본값).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRedaction;

impl Redactor for NoRedaction {
    fn redact(&self, text: &str) -> String {
        text.to_owned()
    }
}
