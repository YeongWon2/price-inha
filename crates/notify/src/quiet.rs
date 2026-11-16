//! 조용한 시간. 기본값은 없다(설정하지 않으면 없다).

use pi_store::UnixMillis;

use crate::Error;

/// 하루 중 조용한 시간대. 자정을 넘길 수 있다(예: 22:00~07:00).
/// 시작은 포함, 끝은 제외한다. 고정 시차(분)로 현지 시각을 센다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuietHours {
    start_minute: u16,
    end_minute: u16,
    utc_offset_minutes: i32,
}

impl QuietHours {
    /// 만든다. 시작 · 끝은 0~1439(자정부터 분). 같으면(빈 시간대인지 하루 종일인지 모호) 또는 범위 밖이면
    /// [`Error::InvalidQuietHours`].
    pub fn new(start_minute: u16, end_minute: u16, utc_offset_minutes: i32) -> Result<Self, Error> {
        let _ = (start_minute, end_minute, utc_offset_minutes);
        todo!()
    }

    /// 이 시각이 조용한 시간에 드는가.
    pub fn contains(&self, at: UnixMillis) -> bool {
        let _ = at;
        todo!()
    }
}
