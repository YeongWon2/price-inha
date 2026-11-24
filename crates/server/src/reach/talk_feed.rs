//! (22차 1번) 논의 방의 담당 글을 사무실 이벤트 [`KIND_OFFICE_TALK`]로 내는 곳과 최근 줄 경로 [`OFFICE_TALK_PATH`].
//!
//! 방 로그는 `pi-agent`가 직접 쓴다. 그래서 논의가 도는 동안 그 방을 짧은 간격([`FOLLOW_TICK`])으로 따라가며
//! 새 `message.member` 줄을 하나씩 내고, 논의가 끝나면 한 번 더 훑는다([`TalkFeed::follow`]). 서버가 직접 쓰는
//! 하루 판단 방의 글은 쓴 자리에서 바로 낸다([`TalkFeed::publish_event`]). 훑기는 하나씩 한다(같은 줄을 두 번 내지 않는다).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use pi_office::EventSink;
use pi_secrets::SecretStore;
use pi_store::{RoomEvent, Store, StoreError};
use serde_json::{Value, json};

use super::linked::LateSink;
use super::office_http::reply;
use super::{KIND_OFFICE_TALK, OFFICE_TALK_PATH, OFFICE_TALK_RECENT, OFFICE_TEXT_MAX};

/// 논의가 도는 동안 방 로그를 다시 보는 간격(1초 안에 내려고 짧게 둔다).
const FOLLOW_TICK: Duration = Duration::from_millis(100);
/// 저장된 이벤트를 한 번에 읽는 수.
const READ_BATCH: usize = 1000;
/// 방 로그를 한 번에 읽는 수.
const ROOM_BATCH: usize = 200;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 팀 대화 줄을 내고 최근 줄을 쥔다.
pub(crate) struct TalkFeed {
    store: Store,
    sink: Arc<LateSink>,
    secrets: Arc<SecretStore>,
    /// 훑기 · 내기 · 최근 줄 읽기를 하나씩 한다.
    turn: tokio::sync::Mutex<()>,
    /// 최근 줄(이벤트 번호, 내용). 처음 읽을 때 저장소에서 채운다.
    recent: Mutex<Option<VecDeque<(u64, Value)>>>,
}

/// 따라가는 방 하나.
struct Follow {
    room: String,
    topic: String,
    /// 이 번호까지 봤다.
    cursor: Mutex<u64>,
    stop: AtomicBool,
}

/// 논의가 끝나면 [`Following::finish`]로 마지막으로 훑는다. 버려지면(취소 · panic) 따라가기만 멈춘다.
pub(crate) struct Following {
    feed: Arc<TalkFeed>,
    follow: Arc<Follow>,
}

impl Following {
    /// 따라가기를 멈추고 남은 줄을 모두 낸다.
    pub(crate) async fn finish(self) {
        self.follow.stop.store(true, Ordering::SeqCst);
        self.feed.sweep(&self.follow).await;
    }
}

impl Drop for Following {
    fn drop(&mut self) {
        self.follow.stop.store(true, Ordering::SeqCst);
    }
}

impl TalkFeed {
    pub(crate) fn new(store: Store, sink: Arc<LateSink>, secrets: Arc<SecretStore>) -> Self {
        Self {
            store,
            sink,
            secrets,
            turn: tokio::sync::Mutex::new(()),
            recent: Mutex::new(None),
        }
    }

    /// 방 로그 글 하나의 `office.talk` 내용(가린 글, [`OFFICE_TEXT_MAX`]자에서 자름).
    fn talk_of(&self, event: &RoomEvent, topic: &str) -> Value {
        let text = event.payload["text"].as_str().unwrap_or_default();
        let text: String = self
            .secrets
            .redact(text)
            .chars()
            .take(OFFICE_TEXT_MAX)
            .collect();
        json!({
            "room": event.room,
            "agent": event.actor_id,
            "text": text,
            "at": event.at.0,
            "topic": topic,
        })
    }

    /// 내고 최근 줄에 더한다. `turn`을 쥔 채 부른다.
    async fn emit(&self, event: &RoomEvent, topic: &str) {
        let talk = self.talk_of(event, topic);
        match EventSink::publish(self.sink.as_ref(), KIND_OFFICE_TALK, talk.clone()).await {
            Ok(seq) => {
                if let Some(recent) = lock(&self.recent).as_mut() {
                    recent.push_back((seq, talk));
                    while recent.len() > OFFICE_TALK_RECENT {
                        recent.pop_front();
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "팀 대화 줄을 내지 못했다"),
        }
    }

    /// 서버가 방 로그에 직접 쓴 담당 글 하나를 곧바로 낸다(하루 판단 방).
    pub(crate) async fn publish_event(&self, event: &RoomEvent, topic: &str) {
        if event.kind != "message.member" {
            return;
        }
        let _turn = self.turn.lock().await;
        self.emit(event, topic).await;
    }

    /// 방을 따라가기 시작한다: 지금 방의 마지막 번호 뒤의 담당 글을 [`FOLLOW_TICK`]마다 낸다.
    pub(crate) fn follow(self: &Arc<Self>, room: &str, topic: &str) -> Following {
        let cursor = self.store.room_last_seq(room).unwrap_or_else(|error| {
            tracing::warn!(%error, "방 로그의 마지막 번호를 읽지 못했다");
            0
        });
        let follow = Arc::new(Follow {
            room: room.to_string(),
            topic: topic.to_string(),
            cursor: Mutex::new(cursor),
            stop: AtomicBool::new(false),
        });
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let (feed, ticking) = (Arc::clone(self), Arc::clone(&follow));
            runtime.spawn(async move {
                while !ticking.stop.load(Ordering::SeqCst) {
                    tokio::time::sleep(FOLLOW_TICK).await;
                    if ticking.stop.load(Ordering::SeqCst) {
                        break;
                    }
                    feed.sweep(&ticking).await;
                }
            });
        }
        Following {
            feed: Arc::clone(self),
            follow,
        }
    }

    /// 그 방의 새 담당 글을 모두 낸다.
    async fn sweep(&self, follow: &Follow) {
        let _turn = self.turn.lock().await;
        loop {
            let after = *lock(&follow.cursor);
            let batch = match self.store.room_events(&follow.room, after, ROOM_BATCH) {
                Ok(batch) => batch,
                Err(error) => {
                    tracing::warn!(%error, "방 로그를 읽지 못했다");
                    return;
                }
            };
            let Some(last) = batch.last().map(|event| event.seq) else {
                return;
            };
            for event in batch.iter().filter(|e| e.kind == "message.member") {
                self.emit(event, &follow.topic).await;
            }
            *lock(&follow.cursor) = last;
        }
    }

    /// 최근 줄(이벤트 번호, 내용)을 시각 순서로. 처음이면 저장소에서 읽어 채운다.
    pub(crate) async fn recent(&self) -> Result<Vec<(u64, Value)>, StoreError> {
        let _turn = self.turn.lock().await;
        if lock(&self.recent).is_none() {
            let loaded = self.load()?;
            *lock(&self.recent) = Some(loaded);
        }
        let mut lines: Vec<(u64, Value)> = lock(&self.recent)
            .as_ref()
            .map(|recent| recent.iter().cloned().collect())
            .unwrap_or_default();
        lines.sort_by_key(|(_, talk)| talk["at"].as_i64().unwrap_or_default());
        Ok(lines)
    }

    /// 저장된 [`KIND_OFFICE_TALK`] 가운데 마지막 [`OFFICE_TALK_RECENT`]줄.
    fn load(&self) -> Result<VecDeque<(u64, Value)>, StoreError> {
        let mut recent = VecDeque::with_capacity(OFFICE_TALK_RECENT + 1);
        let mut after = 0;
        loop {
            let batch = self.store.read_after(after, READ_BATCH)?;
            let Some(last) = batch.last().map(|event| event.seq) else {
                return Ok(recent);
            };
            for event in batch.into_iter().filter(|e| e.kind == KIND_OFFICE_TALK) {
                recent.push_back((event.seq, event.payload));
                if recent.len() > OFFICE_TALK_RECENT {
                    recent.pop_front();
                }
            }
            after = last;
        }
    }
}

async fn recent_lines(State(feed): State<Arc<TalkFeed>>) -> Response {
    match feed.recent().await {
        Ok(lines) => {
            let lines: Vec<Value> = lines.into_iter().map(|(_, talk)| talk).collect();
            reply(StatusCode::OK, Value::Array(lines))
        }
        Err(error) => {
            tracing::warn!(%error, "팀 대화 최근 줄을 읽지 못했다");
            reply(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "internal"}),
            )
        }
    }
}

/// `GateBuilder::protected_routes`에 끼운다(세션이 있어야 열린다).
pub(crate) fn routes(feed: Arc<TalkFeed>) -> Router {
    Router::new()
        .route(OFFICE_TALK_PATH, get(recent_lines))
        .with_state(feed)
}
