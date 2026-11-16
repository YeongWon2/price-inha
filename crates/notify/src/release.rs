//! 조용한 시간에 미룬 푸시를 조용한 시간이 끝난 뒤 다시 보낸다.
//!
//! 미룰 때 표시 이벤트(`push.held`)를 남기고, 다시 보내면 `push.released`를 남긴다. 표시는 번호 있는
//! 이벤트라 다시 켜도 이어지고, 알림 목록(`notices_after`)에는 나오지 않는다.

use std::collections::BTreeSet;
use std::sync::PoisonError;

use pi_store::{EventRange, UnixMillis};

use crate::dispatch::send_all;
use crate::notifier::{HELD_EVENT, RELEASED_EVENT, notice_record, push_payload};
use crate::{Delivery, Error, Notifier};

/// 미룬 푸시를 다시 보내는 기한(밀리초). 미룬 지 이보다 오래된 것은 다시 보내지 않는다(낡은 알림).
pub const HELD_RELEASE_WINDOW_MS: i64 = 24 * 3_600 * 1_000;

/// 표시를 한 번에 읽는 수.
const MARK_BATCH: usize = 256;

impl Notifier {
    /// 조용한 시간이 아닐 때, 미뤄 둔 알림(기한 [`HELD_RELEASE_WINDOW_MS`] 안)의 푸시를 번호 순서로 한 번씩
    /// 보낸다. 보낸 알림은 표시를 남겨 다시 보내지 않는다. 아직 조용한 시간이면 아무것도 하지 않는다.
    /// 구독이 없으면 보내지 못했으므로 표시를 남기지 않는다(기한 안에 구독이 생기면 보낸다).
    /// 이벤트(알림 목록)는 바꾸지 않는다. 결과의 `held`는 `None`이고 `seq`는 원래 알림 번호다.
    pub async fn release_held(&self) -> Result<Vec<Delivery>, Error> {
        let _once = self.release_gate.lock().await;
        let now = self.shared.clock.now();
        let quiet = *self
            .quiet_hours
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if quiet.is_some_and(|q| q.contains(now)) {
            return Ok(Vec::new());
        }
        let pending = self.pending_held(UnixMillis(now.0 - HELD_RELEASE_WINDOW_MS))?;
        let mut deliveries = Vec::new();
        for seq in pending {
            let subscriptions = self.shared.store.push_subscriptions()?;
            if subscriptions.is_empty() {
                break;
            }
            let Some(record) = self.notice_by_seq(seq)? else {
                continue;
            };
            let payload = push_payload(
                record.kind,
                &record.title,
                &record.body,
                record.link.as_deref(),
                seq,
            );
            let attempts = send_all(
                &self.shared,
                subscriptions,
                record.kind.urgency(),
                payload,
                now,
            )
            .await;
            self.shared
                .store
                .append_at(RELEASED_EVENT, &serde_json::json!({"seq": seq}), now)?;
            deliveries.push(Delivery {
                seq,
                held: None,
                first_attempt_at: Some(now),
                attempts,
            });
        }
        Ok(deliveries)
    }

    /// `since`보다 늦게 미룬 알림 중 아직 다시 보내지 않은 번호(오름차순).
    fn pending_held(&self, since: UnixMillis) -> Result<Vec<u64>, Error> {
        let mut held = BTreeSet::new();
        let mut cursor = 0;
        loop {
            let range = EventRange {
                after_seq: cursor,
                after_at: Some(since),
                ..EventRange::default()
            };
            let events = self.shared.store.read_events(range, MARK_BATCH)?;
            let Some(last) = events.last() else {
                break;
            };
            cursor = last.seq;
            let read = events.len();
            for event in events {
                let Some(seq) = event.payload["seq"].as_u64() else {
                    continue;
                };
                match event.kind.as_str() {
                    HELD_EVENT => {
                        held.insert(seq);
                    }
                    RELEASED_EVENT => {
                        held.remove(&seq);
                    }
                    _ => {}
                }
            }
            if read < MARK_BATCH {
                break;
            }
        }
        Ok(held.into_iter().collect())
    }

    /// 번호로 알림 하나를 읽는다(알림이 아니거나 없으면 `None`).
    fn notice_by_seq(&self, seq: u64) -> Result<Option<crate::NoticeRecord>, Error> {
        let range = EventRange {
            after_seq: seq.saturating_sub(1),
            ..EventRange::default()
        };
        let events = self.shared.store.read_events(range, 1)?;
        Ok(events
            .into_iter()
            .next()
            .filter(|event| event.seq == seq)
            .and_then(notice_record))
    }
}
