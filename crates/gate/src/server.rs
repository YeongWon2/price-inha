//! 접속 관문을 열고 끄기, 이벤트를 내보내는 손잡이.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{get, post};
use pi_store::Store;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::access::Access;
use crate::guard::{self, Shared};
use crate::hub::Hub;
use crate::{
    AccessKeys, Clock, GateBuilder, GateConfig, GateError, LockAlert, NoLockAlert, NoRedaction,
    PeerOverride, SESSION_PATH, SystemClock, WS_PATH, enter, ws,
};

/// 접속 관문. 아직 주소를 열지 않은 상태다.
pub struct Gate {
    pub(crate) hub: Arc<Hub>,
    pub(crate) config: GateConfig,
    pub(crate) public: Router,
    pub(crate) protected: Router,
    pub(crate) peer_override: Option<PeerOverride>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) lock_alert: Arc<dyn LockAlert>,
}

/// 끌 때 요청 처리와 실시간 연결이 끝나기를 기다리는 가장 긴 시간.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

impl Gate {
    /// 저장소와 설정으로 만들기 시작한다.
    pub fn builder(store: Store, config: GateConfig) -> GateBuilder {
        GateBuilder {
            store,
            config,
            clock: Arc::new(SystemClock),
            redactor: Arc::new(NoRedaction),
            public: Router::new(),
            protected: Router::new(),
            peer_override: None,
            lock_alert: Arc::new(NoLockAlert),
        }
    }

    /// 이벤트를 내보내는 손잡이. 주소를 열기 전에도 쓸 수 있다(저장은 되고, 연결된 화면이 없을 뿐이다).
    pub fn publisher(&self) -> Publisher {
        Publisher {
            hub: Arc::clone(&self.hub),
        }
    }

    /// [`GateConfig::bind`]를 열고 요청을 받기 시작한다. tokio 런타임 안에서 부른다.
    ///
    /// 열지 못하면 [`GateError::Bind`].
    /// (5차) [`GateConfig::keepalive_interval`] · [`GateConfig::pong_timeout`]이 0이면
    /// 주소를 열지 않고 [`GateError::InvalidConfig`]다.
    pub async fn serve(self) -> Result<GateServer, GateError> {
        if self.config.keepalive_interval.is_zero() {
            return Err(GateError::InvalidConfig {
                field: "keepalive_interval",
            });
        }
        if self.config.pong_timeout.is_zero() {
            return Err(GateError::InvalidConfig {
                field: "pong_timeout",
            });
        }
        let bind_error = |error: std::io::Error| GateError::Bind { kind: error.kind() };
        let listener = tokio::net::TcpListener::bind(self.config.bind)
            .await
            .map_err(bind_error)?;
        let addr = listener.local_addr().map_err(bind_error)?;
        let (stop, stop_rx) = watch::channel(false);
        let (done_tx, done) = mpsc::channel(1);
        let access = Arc::new(Access::new(
            self.hub.store().clone(),
            self.clock,
            self.lock_alert,
            (
                self.config.loopback_delay_start,
                self.config.loopback_delay_max,
            ),
        ));
        let shared = Arc::new(Shared {
            hub: Arc::clone(&self.hub),
            access: Arc::clone(&access),
            allowed_hosts: self.config.allowed_hosts,
            port: addr.port(),
            peer_override: self.peer_override,
            stop: stop_rx.clone(),
            _done: done_tx,
            live: Arc::new(AtomicUsize::new(0)),
            keepalive: ws::Keepalive {
                interval: self.config.keepalive_interval,
                pong_timeout: self.config.pong_timeout,
            },
        });
        let app = routes(shared, self.public, self.protected);
        let task = tokio::spawn(async move {
            let mut stop = stop_rx;
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move { ws::stopped(&mut stop).await })
            .await
        });
        Ok(GateServer {
            addr,
            hub: self.hub,
            access,
            stop,
            task,
            done,
        })
    }
}

/// 경로를 잇는다. 바깥 층부터: 보안 헤더 → Host · 상대 주소 → Origin → 세션 → 핸들러.
fn routes(shared: Arc<Shared>, public: Router, protected: Router) -> Router {
    let session = || from_fn_with_state(Arc::clone(&shared), guard::session);
    let enter = Router::new()
        .route(SESSION_PATH, post(enter::enter))
        .layer(from_fn(guard::origin_if_changing));
    let live = Router::new()
        .route(WS_PATH, get(ws::upgrade))
        .layer(session())
        .layer(from_fn(guard::origin_always));
    let protected = protected
        .layer(session())
        .layer(from_fn(guard::origin_if_changing));
    Router::new()
        .merge(enter)
        .merge(live)
        .with_state(Arc::clone(&shared))
        .merge(public)
        .merge(protected)
        .layer(from_fn_with_state(shared, guard::host_and_peer))
        .layer(from_fn(guard::security_headers))
}

/// 열려 있는 접속 관문.
pub struct GateServer {
    addr: SocketAddr,
    hub: Arc<Hub>,
    access: Arc<Access>,
    stop: watch::Sender<bool>,
    task: JoinHandle<std::io::Result<()>>,
    done: mpsc::Receiver<()>,
}

impl GateServer {
    /// 실제로 연 주소(포트 0을 줬으면 고른 포트가 들어 있다).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// 이벤트를 내보내는 손잡이.
    pub fn publisher(&self) -> Publisher {
        Publisher {
            hub: Arc::clone(&self.hub),
        }
    }

    /// 접속 키와 잠금을 다루는 손잡이(9차). 그 컴퓨터에서만 쓰게 잇는 것은 조립하는 쪽이다.
    pub fn access_keys(&self) -> AccessKeys {
        AccessKeys::new(Arc::clone(&self.access))
    }

    /// 끈다: 새 연결을 받지 않고, 열린 실시간 연결을 닫고, 돌던 작업이 끝나기를 기다린다.
    /// 끝난 뒤에는 그 주소로 연결되지 않는다. 저장소는 닫지 않는다(가진 쪽이 닫는다).
    pub async fn shutdown(mut self) -> Result<(), GateError> {
        self.hub.close().await;
        self.stop.send_replace(true);
        let served = match tokio::time::timeout(SHUTDOWN_GRACE, &mut self.task).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(_)) | Err(_)) => Err(GateError::Task),
            Err(_) => {
                // 끝나지 않는 요청이 있어도 주소는 닫는다.
                tracing::warn!("요청 처리가 시간 안에 끝나지 않아 그만둡니다");
                self.task.abort();
                let _ = (&mut self.task).await;
                Ok(())
            }
        };
        // 실시간 연결이 모두 닫힐 때까지 기다린다(쥐고 있던 쪽이 다 사라지면 `None`이 온다).
        if tokio::time::timeout(SHUTDOWN_GRACE, self.done.recv())
            .await
            .is_err()
        {
            tracing::warn!("실시간 연결이 시간 안에 닫히지 않았습니다");
        }
        served
    }
}

/// 이벤트를 내보내는 손잡이. 복제해 여러 모듈이 함께 쓴다.
#[derive(Clone)]
pub struct Publisher {
    hub: Arc<Hub>,
}

impl Publisher {
    /// 이벤트를 저장하고 연결된 화면에 보낸다. 저장소가 붙인 번호를 돌려준다.
    ///
    /// - 저장이 끝난 뒤에 보낸다. 저장에 실패하면 보내지 않는다.
    /// - 여러 곳에서 함께 불러도 화면에는 번호 순서로 간다.
    /// - 느린 연결을 기다리지 않는다.
    /// - 내용은 가린 뒤에 저장한다(N8).
    /// - 내용이 [`GateConfig::max_frame_bytes`]를 넘으면 [`GateError::FrameTooLarge`]이고
    ///   저장하지 않는다(번호를 쓰지 않는다).
    pub async fn publish(&self, kind: &str, payload: serde_json::Value) -> Result<u64, GateError> {
        // 따로 띄운 작업에서 한다: 부른 쪽이 기다리다 그만둬도 "저장은 됐는데 화면에는 안 간"
        // 이벤트(번호가 빠져 보인다)가 생기지 않는다.
        let hub = Arc::clone(&self.hub);
        let kind = kind.to_owned();
        tokio::spawn(async move { hub.publish(kind, payload).await })
            .await
            .map_err(|_| GateError::Task)?
    }

    /// 지나가는 이벤트를 보낸다. 저장하지 않고 번호도 없다(시세처럼 자주 바뀌는 것).
    ///
    /// 종류마다 가장 최근 것 하나를 기억해 두었다가, 새로 연결한 화면에 먼저 보낸다.
    /// 기다리지 않는다(느린 연결은 끊긴다).
    /// 내용이 [`GateConfig::max_frame_bytes`]를 넘으면 보내지 않고 버린다.
    ///
    /// (5차) 종류 이름은 저장소와 같게 앞뒤 공백(Python `str.isspace`와 같은 공백)을 떼고 검사하고,
    /// 뗀 이름으로 보내고 기억한다(원본 Hermes `gateway/hosted_rooms_common.py`처럼).
    /// 떼고도 규칙에 어긋나면 보내지도 기억하지도 않는다.
    pub fn publish_transient(&self, kind: &str, payload: serde_json::Value) {
        self.hub.publish_transient(kind, payload);
    }
}
