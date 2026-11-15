//! 이벤트를 저장하고 연결된 화면들에 나눠 보내는 곳.
//!
//! 번호가 빠지거나 겹치지 않게 하는 방법: 저장하고 나눠 보내는 일과 "연결을 받는 쪽에 올리는 일"을
//! 같은 순서 잠금(`order`) 안에서 한다. 그래서 어떤 연결이든 올라간 순간을 기준으로
//! 그 전의 이벤트는 모두 저장소에 있고(다시 보내기로 받는다), 그 뒤의 이벤트는 모두 그 연결의
//! 줄로 간다.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use pi_store::{EventRange, Store, TimedEvent};
use serde_json::Value;
use tokio::sync::{Notify, mpsc};

use crate::frame::{self, Frame};
use crate::{Clock, GateError, Redactor};

/// 다시 보낼 때 저장소에서 한 번에 읽는 이벤트 수.
const REPLAY_PAGE: usize = 64;

/// 연결 하나로 가는 줄.
struct Conn {
    id: u64,
    tx: mpsc::Sender<Frame>,
    /// 밀려서 끊을 때 알린다(쓰다가 멈춰 있는 연결도 깨어나 닫는다).
    kill: Arc<Notify>,
}

#[derive(Default)]
struct Inner {
    closed: bool,
    next_id: u64,
    conns: Vec<Conn>,
    /// 종류마다 가장 최근의 지나가는 이벤트.
    latest: BTreeMap<String, Frame>,
}

/// 이벤트를 저장하고 나눠 보내는 곳. 관문과 손잡이들이 함께 쓴다.
pub(crate) struct Hub {
    store: Store,
    clock: Arc<dyn Clock>,
    redactor: Arc<dyn Redactor>,
    send_buffer: usize,
    /// 발행하는 내용 하나(JSON으로 적은 글)의 크기 상한(바이트).
    max_frame_bytes: usize,
    /// 저장 · 나눠 보내기 · 연결 올리기의 순서를 하나로 세운다.
    order: tokio::sync::Mutex<()>,
    inner: Mutex<Inner>,
}

/// 읽은 쪽(`after` 뒤의 `events`, 번호 순서) 가운데 저장소가 **옛 봉투 줄**로 판정한 번호(7차 6번).
///
/// 판정은 저장소가 원래 글자로 한다(`Store::legacy_envelope_seqs`): 관문은 읽힌 JSON 값만 받아
/// 맨 위 칸 이름이 겹친 줄을 옛 봉투와 가를 수 없다. 범위는 읽은 쪽과 같게 맞춘다: 옛 봉투 줄은
/// 저장된 줄이므로 `(after, 마지막 번호]` 안에서 읽은 줄 수보다 많을 수 없다.
/// 막히는 일이라 `spawn_blocking` 안에서 부른다.
fn legacy_seqs(
    store: &Store,
    after: u64,
    events: &[TimedEvent],
) -> Result<BTreeSet<u64>, pi_store::StoreError> {
    let Some(last) = events.last().map(|event| event.seq) else {
        return Ok(BTreeSet::new());
    };
    Ok(store
        .legacy_envelope_seqs(after, events.len())?
        .into_iter()
        .filter(|seq| *seq <= last)
        .collect())
}

/// 받는 쪽에 올리려 했을 때의 결과.
pub(crate) enum Attach {
    /// 아직 다시 보낼 것이 남았다. 보내고 다시 부른다.
    More(Vec<(u64, Frame)>),
    /// 올라갔다. 이제부터 생기는 이벤트는 줄로 온다.
    Live(Subscription),
}

/// 올라간 연결이 쥐는 것. 놓으면 받는 쪽 목록에서 빠진다.
pub(crate) struct Subscription {
    /// 새 프레임이 오는 줄. 최대 `send_buffer`개가 쌓인다.
    pub(crate) rx: mpsc::Receiver<Frame>,
    /// 밀려서 끊겼다는 알림.
    pub(crate) kill: Arc<Notify>,
    /// 연결을 시작한 뒤에 바뀐 지나가는 이벤트(올라가기 전에 놓친 것).
    pub(crate) fresh: Vec<Frame>,
    _detach: Detach,
}

struct Detach {
    hub: Arc<Hub>,
    id: u64,
}

impl Drop for Detach {
    fn drop(&mut self) {
        self.hub.lock().conns.retain(|conn| conn.id != self.id);
    }
}

impl Hub {
    pub(crate) fn new(
        store: Store,
        clock: Arc<dyn Clock>,
        redactor: Arc<dyn Redactor>,
        send_buffer: usize,
        max_frame_bytes: usize,
    ) -> Self {
        Self {
            store,
            clock,
            redactor,
            // tokio의 줄은 크기 0을 받지 않는다.
            send_buffer: send_buffer.max(1),
            max_frame_bytes,
            order: tokio::sync::Mutex::new(()),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// 한 번에 하나씩만 값을 바꾸므로, 잠금을 쥔 채 죽은 스레드가 있어도 값은 온전하다.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn store(&self) -> &Store {
        &self.store
    }

    pub(crate) fn clock(&self) -> &dyn Clock {
        &*self.clock
    }

    /// 내용이 크기 상한 안인가.
    fn fits(&self, payload: &Value) -> bool {
        frame::json_len(payload) <= self.max_frame_bytes
    }

    /// 가린 내용을 돌려준다. 가리기 전이나 뒤에 크기 상한을 넘으면 `None`.
    ///
    /// 가리기 전에 먼저 재서, 너무 큰 내용에는 가리기 일을 하지 않는다. 가린 뒤에 다시 재는 것은
    /// 저장되고 나가는 것이 가린 쪽이기 때문이다(가린 글자가 원래 값보다 길 수 있다).
    fn redact_within_limit(&self, payload: Value) -> Option<Value> {
        if !self.fits(&payload) {
            return None;
        }
        let payload = frame::redact_value(&*self.redactor, payload);
        self.fits(&payload).then_some(payload)
    }

    /// 가리고 저장하고 나눠 보낸다. 저장소가 붙인 번호를 돌려준다.
    pub(crate) async fn publish(&self, kind: String, payload: Value) -> Result<u64, GateError> {
        // 저장소에 쓰기 전에 가린다(N8). 종류 이름도 같다.
        let payload = self
            .redact_within_limit(payload)
            .ok_or(GateError::FrameTooLarge {
                limit: self.max_frame_bytes,
            })?;
        let kind = self.redactor.redact(&kind);
        let _order = self.order.lock().await;
        if self.lock().closed {
            return Err(GateError::Closed);
        }
        // 시각은 시각 칸에 둔다(4차). 저장소가 다듬어 저장한 줄을 돌려받아 그 줄로 보낸다:
        // 바로 보내는 프레임과 다시 보내는 프레임이 같다(종류는 저장된 이름).
        let at = self.clock.now();
        let store = self.store.clone();
        let event = tokio::task::spawn_blocking(move || {
            let seq = store.append_at(&kind, &payload, at)?;
            let range = EventRange {
                after_seq: seq.saturating_sub(1),
                ..EventRange::default()
            };
            let saved = store.read_events(range, 1)?.into_iter().next();
            let event = saved.unwrap_or(TimedEvent {
                seq,
                kind,
                at: Some(at),
                payload,
            });
            Ok::<_, pi_store::StoreError>(event)
        })
        .await
        .map_err(|_| GateError::Task)??;
        let seq = event.seq;
        // 프레임을 만들 때 한 번 더 가린다(다시 보낼 때와 같은 길). 방금 쌓은 줄은 옛 봉투가 아니다:
        // 저장소의 옛 형식 경계는 버전을 올릴 때 그때의 마지막 번호 이하로 한 번 정해지고 바뀌지 않으므로
        // 새로 받은 번호는 늘 경계 뒤다(`legacy_envelope_seqs`도 경계 뒤 번호는 주지 않는다).
        let frame = frame::stored(&*self.redactor, event, false);
        self.broadcast(&mut self.lock(), &frame);
        Ok(seq)
    }

    /// 지나가는 이벤트를 보낸다. 종류마다 가장 최근 것 하나를 기억한다.
    /// 종류 이름이 규칙에 어긋나거나 크기 상한을 넘으면 보내지도 기억하지도 않는다.
    pub(crate) fn publish_transient(&self, kind: &str, payload: Value) {
        // 저장소처럼 앞뒤 공백을 떼고 본다. 뗀 이름으로 보내고 기억한다(5차 2번).
        let Some(kind) = frame::trimmed_kind(kind) else {
            tracing::warn!("이름 규칙에 어긋나는 종류의 지나가는 이벤트를 버립니다");
            return;
        };
        let Some(payload) = self.redact_within_limit(payload) else {
            tracing::warn!(
                limit = self.max_frame_bytes,
                "크기 상한을 넘는 지나가는 이벤트를 버립니다"
            );
            return;
        };
        let frame = frame::transient(&*self.redactor, kind, payload, self.clock.now());
        let mut inner = self.lock();
        if inner.closed {
            return;
        }
        inner.latest.insert(kind.to_owned(), frame.clone());
        self.broadcast(&mut inner, &frame);
    }

    /// 모든 연결의 줄에 넣는다. 기다리지 않는다: 줄이 찬 연결은 끊는다.
    fn broadcast(&self, inner: &mut Inner, frame: &Frame) {
        inner
            .conns
            .retain(|conn| match conn.tx.try_send(frame.clone()) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    tracing::warn!(conn = conn.id, "밀린 실시간 연결을 끊습니다");
                    conn.kill.notify_one();
                    false
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            });
    }

    /// 종류마다 가장 최근의 지나가는 이벤트.
    pub(crate) fn latest(&self) -> BTreeMap<String, Frame> {
        self.lock().latest.clone()
    }

    /// 번호가 `after`보다 큰 저장된 이벤트를 번호 순서로 한 쪽 읽는다.
    pub(crate) async fn read_page(&self, after: u64) -> Result<Vec<(u64, Frame)>, GateError> {
        let store = self.store.clone();
        let range = EventRange {
            after_seq: after,
            ..EventRange::default()
        };
        let (events, legacy) = tokio::task::spawn_blocking(move || {
            let events = store.read_events(range, REPLAY_PAGE)?;
            let legacy = legacy_seqs(&store, after, &events)?;
            Ok::<_, pi_store::StoreError>((events, legacy))
        })
        .await
        .map_err(|_| GateError::Task)??;
        Ok(events
            .into_iter()
            .map(|event| {
                let old = legacy.contains(&event.seq);
                (event.seq, frame::stored(&*self.redactor, event, old))
            })
            .collect())
    }

    /// 받는 쪽에 올린다.
    ///
    /// `cursor`가 있으면 그 번호 뒤에 저장된 것이 더 없을 때만 올린다(있으면 그것을 돌려준다).
    /// `cursor`가 없으면 다시 보내지 않고 지금부터 받는다.
    /// `seen`은 이 연결에 이미 보낸 지나가는 이벤트다. 그 뒤에 바뀐 것만 [`Subscription::fresh`]로 준다.
    pub(crate) async fn attach(
        self: &Arc<Self>,
        cursor: Option<u64>,
        seen: &BTreeMap<String, Frame>,
    ) -> Result<Attach, GateError> {
        let _order = self.order.lock().await;
        if let Some(after) = cursor {
            let page = self.read_page(after).await?;
            if !page.is_empty() {
                return Ok(Attach::More(page));
            }
        }
        let mut inner = self.lock();
        if inner.closed {
            return Err(GateError::Closed);
        }
        let (tx, rx) = mpsc::channel(self.send_buffer);
        let kill = Arc::new(Notify::new());
        let id = inner.next_id;
        inner.next_id += 1;
        inner.conns.push(Conn {
            id,
            tx,
            kill: Arc::clone(&kill),
        });
        let fresh = inner
            .latest
            .iter()
            .filter(|(kind, frame)| seen.get(*kind) != Some(*frame))
            .map(|(_, frame)| frame.clone())
            .collect();
        Ok(Attach::Live(Subscription {
            rx,
            kill,
            fresh,
            _detach: Detach {
                hub: Arc::clone(self),
                id,
            },
        }))
    }

    /// 닫는다: 더는 저장하지도 보내지도 않고, 올라가 있는 연결을 모두 내린다.
    pub(crate) async fn close(&self) {
        let _order = self.order.lock().await;
        let mut inner = self.lock();
        inner.closed = true;
        for conn in inner.conns.drain(..) {
            conn.kill.notify_one();
        }
    }
}
