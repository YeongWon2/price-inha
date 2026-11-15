//! 웹 푸시 구독(11차 4번). 폰 알림을 받을 구독을 둔다. 표: `push_subscriptions`.

use rusqlite::{Row, params};

use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 새 구독(브라우저의 `PushSubscription`이 주는 값).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPushSubscription {
    /// 푸시 서비스 주소. `https://`로 시작해야 한다. 구독의 정체다(같은 주소는 하나뿐).
    pub endpoint: String,
    /// 구독의 공개키(P-256, base64url 글자). 비면 안 된다.
    pub p256dh: String,
    /// 구독의 인증 값(base64url 글자). 비면 안 된다.
    pub auth: String,
    /// 기기 이름표(사람이 읽는 글. 예: "폰").
    pub device: String,
    /// 시각.
    pub at: UnixMillis,
}

/// 저장된 구독.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushSubscription {
    /// 푸시 서비스 주소.
    pub endpoint: String,
    /// 공개키.
    pub p256dh: String,
    /// 인증 값.
    pub auth: String,
    /// 기기 이름표.
    pub device: String,
    /// 처음 둔 시각.
    pub created_at: UnixMillis,
    /// 마지막으로 다시 둔 시각.
    pub updated_at: UnixMillis,
}

impl Store {
    /// 구독을 둔다. 같은 `endpoint`가 이미 있으면 키 · 이름표를 새 값으로 바꾸고 `updated_at`만 `at`이 된다
    /// (`created_at`은 그대로, 줄은 늘지 않는다).
    ///
    /// 주소가 `https://`로 시작하지 않거나 `p256dh` · `auth`가 비었으면
    /// [`StoreError::InvalidPushSubscription`]이고 아무것도 바꾸지 않는다.
    pub fn save_push_subscription(
        &self,
        new: NewPushSubscription,
    ) -> Result<PushSubscription, StoreError> {
        if !new.endpoint.starts_with("https://") || new.p256dh.is_empty() || new.auth.is_empty() {
            return Err(StoreError::InvalidPushSubscription);
        }
        self.write(|conn| {
            conn.execute(
                "INSERT INTO push_subscriptions (endpoint, p256dh, auth, device, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT (endpoint) DO UPDATE
                 SET p256dh = excluded.p256dh, auth = excluded.auth,
                     device = excluded.device, updated_at = excluded.updated_at",
                params![new.endpoint, new.p256dh, new.auth, new.device, new.at.0],
            )?;
            Ok(conn.query_row(
                &format!("SELECT {COLUMNS} FROM push_subscriptions WHERE endpoint = ?1"),
                [&new.endpoint],
                subscription_from,
            )?)
        })
    }

    /// 모든 구독. 처음 둔 순서(같으면 주소 순서)다.
    pub fn push_subscriptions(&self) -> Result<Vec<PushSubscription>, StoreError> {
        self.read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {COLUMNS} FROM push_subscriptions ORDER BY created_at, endpoint"
            ))?;
            let rows = stmt.query_map([], subscription_from)?;
            Ok(rows.collect::<Result<Vec<_>, _>>()?)
        })
    }

    /// 구독을 지운다. 있어서 지웠으면 `true`, 없었으면 `false`(오류가 아니다).
    pub fn remove_push_subscription(&self, endpoint: &str) -> Result<bool, StoreError> {
        self.write(|conn| {
            let removed = conn.execute(
                "DELETE FROM push_subscriptions WHERE endpoint = ?1",
                [endpoint],
            )?;
            Ok(removed > 0)
        })
    }
}

const COLUMNS: &str = "endpoint, p256dh, auth, device, created_at, updated_at";

fn subscription_from(row: &Row<'_>) -> rusqlite::Result<PushSubscription> {
    Ok(PushSubscription {
        endpoint: row.get(0)?,
        p256dh: row.get(1)?,
        auth: row.get(2)?,
        device: row.get(3)?,
        created_at: UnixMillis(row.get(4)?),
        updated_at: UnixMillis(row.get(5)?),
    })
}
