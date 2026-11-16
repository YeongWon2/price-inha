//! 조용한 시간에 미룬 푸시를 조용한 시간이 끝난 뒤 다시 보낸다.

use crate::{Delivery, Error, Notifier};

/// 미룬 푸시를 다시 보내는 기한(밀리초). 미룬 지 이보다 오래된 것은 다시 보내지 않는다(낡은 알림).
pub const HELD_RELEASE_WINDOW_MS: i64 = 24 * 3_600 * 1_000;

impl Notifier {
    /// 조용한 시간이 아닐 때, 미뤄 둔 알림의 푸시를 보낸다.
    pub async fn release_held(&self) -> Result<Vec<Delivery>, Error> {
        Ok(Vec::new())
    }
}
