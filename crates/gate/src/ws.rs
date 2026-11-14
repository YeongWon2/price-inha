//! 실시간 연결(C2 · D11 · E3): 지나가는 이벤트 → 놓친 이벤트 → 새 이벤트 순서로 보낸다.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tokio::sync::{Notify, watch};
use tokio::time::Instant;

use crate::LAST_SEQ_PARAM;
use crate::frame::Frame;
use crate::guard::Shared;
use crate::hub::{Attach, Hub, Subscription};

/// 화면이 보내는 프레임의 크기 상한. 화면은 보낼 것이 없으므로 작게 둔다.
pub(crate) const MAX_INCOMING_BYTES: usize = 4 * 1024;
/// 한꺼번에 열려 있을 수 있는 실시간 연결 수.
pub(crate) const MAX_CONNECTIONS: usize = 64;
/// 프레임 하나를 이 시간 안에 쓰지 못하면 그 연결을 끊는다.
const SEND_TIMEOUT: Duration = Duration::from_secs(30);
/// 닫는다는 인사를 기다리는 시간.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);

/// 실시간 연결의 살아 있음 신호(D17): `interval`마다 ping을 보내고, 보낸 뒤 `pong_timeout` 안에
/// pong이 없으면 그 연결을 끊는다.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Keepalive {
    pub(crate) interval: Duration,
    pub(crate) pong_timeout: Duration,
}

/// `last_seq`를 읽는다. 없으면 `None`(지금부터 받는다). 숫자가 아니거나 두 번 주면 오류.
pub(crate) fn parse_last_seq(query: Option<&str>) -> Result<Option<u64>, ()> {
    let mut found = None;
    for pair in query.unwrap_or_default().split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name != LAST_SEQ_PARAM {
            continue;
        }
        let digits_only = !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
        if found.is_some() || !digits_only {
            return Err(());
        }
        found = Some(value.parse::<u64>().map_err(|_| ())?);
    }
    Ok(found)
}

/// 열려 있는 연결 수를 세는 자리. 놓으면 줄어든다.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(live: &Arc<AtomicUsize>) -> Option<Self> {
        if live.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            live.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self(Arc::clone(live)))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `GET /ws?last_seq=N`. Host · 상대 주소 · Origin · 세션 확인을 지난 뒤에 온다.
pub(crate) async fn upgrade(
    State(shared): State<Arc<Shared>>,
    RawQuery(query): RawQuery,
    upgrade: WebSocketUpgrade,
) -> Response {
    let Ok(last_seq) = parse_last_seq(query.as_deref()) else {
        return (StatusCode::BAD_REQUEST, "last_seq는 숫자여야 합니다").into_response();
    };
    let Some(slot) = Slot::take(&shared.live) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "실시간 연결이 너무 많습니다",
        )
            .into_response();
    };
    upgrade
        .max_message_size(MAX_INCOMING_BYTES)
        .max_frame_size(MAX_INCOMING_BYTES)
        .on_upgrade(move |socket| run(socket, shared, last_seq, slot))
}

/// 연결이 끝난 까닭.
enum End {
    /// 서버를 끈다.
    Shutdown,
    /// 밀려서 끊는다(줄이 찼다).
    Lagged,
    /// 화면이 떠났거나 쓰지 못했다.
    Gone,
    /// 저장소를 읽지 못했거나 관문이 닫혔다.
    Failed,
    /// ping에 정한 시간 안에 pong이 없었다(죽은 연결).
    Silent,
}

async fn run(mut socket: WebSocket, shared: Arc<Shared>, last_seq: Option<u64>, _slot: Slot) {
    let mut stop = shared.stop.clone();
    let end = match stream(&mut socket, &shared, last_seq, &mut stop).await {
        Ok(never) => match never {},
        Err(end) => end,
    };
    // 밀린 연결은 쓰기가 막혀 있으니 인사 없이 닫는다.
    if matches!(end, End::Shutdown | End::Failed | End::Silent) {
        let goodbye = socket.send(Message::Close(None));
        let _ = tokio::time::timeout(CLOSE_TIMEOUT, goodbye).await;
    }
}

/// 끄기 알림을 기다린다. 알리는 쪽이 사라져도 끝난 것으로 본다.
pub(crate) async fn stopped(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|stopping| *stopping).await;
}

async fn lagged(kill: Option<&Notify>) {
    match kill {
        Some(kill) => kill.notified().await,
        None => std::future::pending().await,
    }
}

/// 프레임 하나를 쓴다. 끄기 · 밀림 알림이 오면 쓰다 말고 그만둔다.
async fn push(
    socket: &mut WebSocket,
    frame: Frame,
    stop: &mut watch::Receiver<bool>,
    kill: Option<&Notify>,
) -> Result<(), End> {
    tokio::select! {
        biased;
        () = stopped(stop) => Err(End::Shutdown),
        () = lagged(kill) => Err(End::Lagged),
        sent = tokio::time::timeout(SEND_TIMEOUT, socket.send(Message::Text(frame))) => match sent {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) | Err(_) => Err(End::Gone),
        },
    }
}

async fn stream(
    socket: &mut WebSocket,
    shared: &Shared,
    last_seq: Option<u64>,
    stop: &mut watch::Receiver<bool>,
) -> Result<Infallible, End> {
    let hub = &shared.hub;
    // 1. 종류마다 가장 최근의 지나가는 이벤트.
    let seen = hub.latest();
    for frame in seen.values() {
        push(socket, frame.clone(), stop, None).await?;
    }
    // 2. 놓친 이벤트. 3. 받는 쪽에 올라간다.
    let mut subscription = catch_up(socket, hub, last_seq, &seen, stop).await?;
    let kill = Arc::clone(&subscription.kill);
    for frame in std::mem::take(&mut subscription.fresh) {
        push(socket, frame, stop, Some(&kill)).await?;
    }
    let keepalive = shared.keepalive;
    let mut next_ping = Instant::now() + keepalive.interval;
    // 답을 기다리는 ping이 있으면 그 기한.
    let mut pong_due: Option<Instant> = None;
    loop {
        tokio::select! {
            biased;
            () = stopped(stop) => return Err(End::Shutdown),
            () = kill.notified() => return Err(End::Lagged),
            () = until(pong_due) => return Err(End::Silent),
            frame = subscription.rx.recv() => match frame {
                Some(frame) => push(socket, frame, stop, Some(&kill)).await?,
                None => return Err(End::Lagged),
            },
            () = tokio::time::sleep_until(next_ping) => {
                ping(socket, stop, &kill).await?;
                let now = Instant::now();
                next_ping = now + keepalive.interval;
                pong_due.get_or_insert(now + keepalive.pong_timeout);
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_) | Ok(Message::Close(_))) => return Err(End::Gone),
                Some(Ok(Message::Pong(_))) => pong_due = None,
                // 화면이 보내는 다른 것은 쓰지 않는다(핑에는 라이브러리가 답한다).
                Some(Ok(_)) => {}
            },
        }
    }
}

/// 기한이 있으면 그때까지 기다린다. 없으면 끝나지 않는다.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// 살아 있음 신호(ping)를 보낸다. 쓰지 못하면 그 연결을 끝낸다.
async fn ping(
    socket: &mut WebSocket,
    stop: &mut watch::Receiver<bool>,
    kill: &Notify,
) -> Result<(), End> {
    tokio::select! {
        biased;
        () = stopped(stop) => Err(End::Shutdown),
        () = kill.notified() => Err(End::Lagged),
        sent = tokio::time::timeout(SEND_TIMEOUT, socket.send(Message::Ping(Bytes::new()))) => match sent {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) | Err(_) => Err(End::Gone),
        },
    }
}

/// 번호가 `last_seq`보다 큰 저장된 이벤트를 순서대로 보내고, 더 없을 때 받는 쪽에 올라간다.
async fn catch_up(
    socket: &mut WebSocket,
    hub: &Arc<Hub>,
    last_seq: Option<u64>,
    seen: &BTreeMap<String, Frame>,
    stop: &mut watch::Receiver<bool>,
) -> Result<Subscription, End> {
    let mut cursor = last_seq;
    // 먼저 순서 잠금 없이 따라잡는다(오래 걸려도 발행을 막지 않는다).
    if let Some(after) = cursor.as_mut() {
        loop {
            let page = hub.read_page(*after).await.map_err(|_| End::Failed)?;
            if page.is_empty() {
                break;
            }
            for (seq, frame) in page {
                push(socket, frame, stop, None).await?;
                *after = seq;
            }
        }
    }
    // 그 사이에 생긴 것까지 보내고, 더 없는 순간에 올라간다.
    loop {
        match hub.attach(cursor, seen).await.map_err(|_| End::Failed)? {
            Attach::Live(subscription) => return Ok(subscription),
            Attach::More(page) => {
                for (seq, frame) in page {
                    push(socket, frame, stop, None).await?;
                    cursor = Some(seq);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_seq는_숫자만_받는다() {
        assert_eq!(parse_last_seq(None), Ok(None), "안 주면 지금부터 받는다");
        assert_eq!(parse_last_seq(Some("")), Ok(None));
        assert_eq!(parse_last_seq(Some("other=1")), Ok(None));
        assert_eq!(parse_last_seq(Some("last_seq=0")), Ok(Some(0)));
        assert_eq!(parse_last_seq(Some("a=b&last_seq=42")), Ok(Some(42)));
        assert_eq!(
            parse_last_seq(Some("last_seq=18446744073709551615")),
            Ok(Some(u64::MAX))
        );
        for bad in [
            "last_seq",
            "last_seq=",
            "last_seq=abc",
            "last_seq=-1",
            "last_seq=+1",
            "last_seq=1.5",
            "last_seq=%31",
            "last_seq=18446744073709551616",
            "last_seq=1&last_seq=2",
        ] {
            assert_eq!(parse_last_seq(Some(bad)), Err(()), "{bad}");
        }
    }

    #[test]
    fn 연결_수는_상한을_넘지_않는다() {
        let live = Arc::new(AtomicUsize::new(0));
        let slots: Vec<Slot> = (0..MAX_CONNECTIONS)
            .filter_map(|_| Slot::take(&live))
            .collect();
        assert_eq!(slots.len(), MAX_CONNECTIONS);
        assert!(Slot::take(&live).is_none());
        drop(slots);
        assert_eq!(live.load(Ordering::SeqCst), 0);
        assert!(Slot::take(&live).is_some());
    }
}
