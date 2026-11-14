//! 이벤트를 저장하고 연결된 화면들에 나눠 보내는 곳.
//!
//! 번호가 빠지거나 겹치지 않게 하는 방법: 저장하고 나눠 보내는 일과 "연결을 받는 쪽에 올리는 일"을
//! 같은 순서 잠금(`order`) 안에서 한다. 그래서 어떤 연결이든 올라간 순간을 기준으로
//! 그 전의 이벤트는 모두 저장소에 있고(다시 보내기로 받는다), 그 뒤의 이벤트는 모두 그 연결의
//! 줄로 간다.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use pi_store::Store;
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
    /// 저장 · 나눠 보내기 · 연결 올리기의 순서를 하나로 세운다.
    order: tokio::sync::Mutex<()>,
    inner: Mutex<Inner>,
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
    ) -> Self {
        Self {
            store,
            clock,
            redactor,
            // tokio의 줄은 크기 0을 받지 않는다.
            send_buffer: send_buffer.max(1),
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

    /// 저장하고 나눠 보낸다. 저장소가 붙인 번호를 돌려준다.
    pub(crate) async fn publish(&self, kind: String, payload: Value) -> Result<u64, GateError> {
        let _order = self.order.lock().await;
        if self.lock().closed {
            return Err(GateError::Closed);
        }
        let stored = frame::envelope(self.clock.now(), payload);
        let store = self.store.clone();
        let event = tokio::task::spawn_blocking(move || {
            store.append(&kind, &stored).map(|seq| pi_store::Event {
                seq,
                kind,
                payload: stored,
            })
        })
        .await
        .map_err(|_| GateError::Task)??;
        let seq = event.seq;
        let frame = frame::stored(&*self.redactor, event);
        self.broadcast(&mut self.lock(), &frame);
        Ok(seq)
    }

    /// 지나가는 이벤트를 보낸다. 종류마다 가장 최근 것 하나를 기억한다.
    pub(crate) fn publish_transient(&self, kind: &str, payload: Value) {
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
        let events = tokio::task::spawn_blocking(move || store.read_after(after, REPLAY_PAGE))
            .await
            .map_err(|_| GateError::Task)??;
        Ok(events
            .into_iter()
            .map(|event| (event.seq, frame::stored(&*self.redactor, event)))
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
