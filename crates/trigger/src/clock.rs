//! 시계 · 전체 정지 포트.
//!
//! 벽시계와 기다림은 [`pi_market::Clock`]을 그대로 쓴다. 단조 시계는 [`Monotonic`]으로 받는다.
//! 이 크레이트는 `SystemTime` · `Instant` · `Utc::now`를 직접 부르지 않는다(ARCH-CLOCK-2).

use std::time::Duration;

pub use pi_market::Clock;

/// 단조 시계. 뒤로 가지 않는다(시계 맞춤 · 서머타임과 무관).
///
/// 벽시계 경과와 비교해 시계가 뛰었는지(절전에서 깸 · 시계 맞춤) 안다(원본: OpenClaw tick 1단계).
pub trait Monotonic: Send + Sync {
    /// 정해지지 않은 기준점에서 지난 시간.
    fn elapsed(&self) -> Duration;
}

/// 전체 정지 표시. tick은 잠금 다음에 이것을 보고, 멈춰 있으면 아무것도 깨우지 않는다
/// (원본: Hermes tick 3단계 `estop`).
pub trait StopSignal: Send + Sync {
    /// 전체 정지 중이면 참.
    fn is_stopped(&self) -> bool;
}

/// 벽시계와 단조 시계 경과의 차이가 이것을 넘으면 시계가 뛴 것으로 본다(2초).
pub const CLOCK_JUMP_TOLERANCE: Duration = Duration::from_secs(2);

/// 타이머는 길어야 이만큼이다(60초). 다음 기한이 더 멀어도 이만큼마다 깨서 시계를 본다.
pub const MAX_TIMER: Duration = Duration::from_secs(60);

/// 다음 기한이 지금이거나 지났을 때 넣는 가장 짧은 간격(2초). 같은 초에 무한히 다시 도는 것을 막는다.
pub const MIN_REFIRE_GAP: Duration = Duration::from_secs(2);
