//! 요청마다 보는 것: Host → 상대 주소 → Origin → 세션. 그리고 세션을 만드는 경로.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::header::{COOKIE, HOST, ORIGIN, SET_COOKIE};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use pi_store::NewAccess;
use tokio::sync::{mpsc, watch};

use crate::hub::Hub;
use crate::session::Sessions;
use crate::{ACCESS_ENTER, PeerOverride, SESSION_COOKIE};

/// 요청 처리에 함께 쓰는 것들.
pub(crate) struct Shared {
    pub(crate) hub: Arc<Hub>,
    pub(crate) sessions: Sessions,
    /// 허용하는 Host 이름(포트 없이).
    pub(crate) allowed_hosts: Vec<String>,
    /// 실제로 연 포트.
    pub(crate) port: u16,
    pub(crate) peer_override: Option<PeerOverride>,
    /// 끄기 알림. 참이 되거나 보내는 쪽이 사라지면 멈춘다.
    pub(crate) stop: watch::Receiver<bool>,
    /// 실시간 연결이 모두 끝났는지 아는 데 쓴다(쥐고만 있는다).
    pub(crate) _done: mpsc::Sender<()>,
    /// 열려 있는 실시간 연결 수.
    pub(crate) live: Arc<AtomicUsize>,
}

/// 확인을 지난 상대 주소(포트 없이). 접속 관문이 요청의 확장(extension)에 넣는다.
///
/// 뒤의 핸들러와 [`local_only`]가 읽는다: 접속 기록의 "어디서", 그리고 "그 컴퓨터에서 온
/// 요청인가"의 판단. IPv4로 적을 수 있는 IPv6 주소는 IPv4로 바꾼 값이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peer(pub IpAddr);

/// [`local_only`]가 그 컴퓨터 밖의 요청을 거부할 때의 HTTP 상태.
pub const LOCAL_ONLY_STATUS: u16 = 403;

/// 그 컴퓨터에서 온 요청만 받는 경로로 만든다(3차 규칙 4: 키 등록은 그 컴퓨터에서만).
///
/// 접속 관문의 "보호된 경로" 확인과 **별개로**, 이 경로 자체가 한 번 더 본다:
/// 요청의 확장에 든 [`Peer`]가 루프백이면 핸들러로 넘기고, 루프백이 아니거나 [`Peer`]가
/// **없으면** [`LOCAL_ONLY_STATUS`]로 거부한다(핸들러는 불리지 않는다).
///
/// 지금은 루프백이 아닌 요청이 관문에서 모두 401이라 이 확인까지 오지 않는다. 접속 키(T17)가
/// 생겨 밖에서 온 요청이 관문을 지나게 되어도 이 경로들은 닫혀 있게 하려는 것이다.
/// 전달 헤더(`X-Forwarded-For` 등)는 읽지 않는다.
pub fn local_only(routes: axum::Router) -> axum::Router {
    routes.layer(axum::middleware::from_fn(loopback_peer))
}

/// [`local_only`]의 확인: 확장의 [`Peer`]가 루프백일 때만 넘긴다. 없으면 닫는다.
async fn loopback_peer(request: Request, next: Next) -> Response {
    let local = request
        .extensions()
        .get::<Peer>()
        .is_some_and(|Peer(ip)| ip.to_canonical().is_loopback());
    if local {
        next.run(request).await
    } else {
        let status = StatusCode::from_u16(LOCAL_ONLY_STATUS).unwrap_or(StatusCode::FORBIDDEN);
        reject(status, "이 컴퓨터에서만 할 수 있습니다")
    }
}

fn reject(status: StatusCode, why: &'static str) -> Response {
    (status, why).into_response()
}

/// 헤더가 꼭 하나 있고 글자로 읽힐 때만 그 값을 준다.
fn single<'a>(headers: &'a HeaderMap, name: &axum::http::HeaderName) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?;
    if values.next().is_some() {
        return None;
    }
    first.to_str().ok()
}

/// `host`가 `<허용 이름>:<연 포트>`인가. 연 포트가 80이면 포트를 뺀 모양도 같은 주소다.
fn host_allowed(host: &str, allowed: &[String], port: u16) -> bool {
    let with_port = format!(":{port}");
    allowed.iter().any(|name| {
        let Some((head, rest)) = host.split_at_checked(name.len()) else {
            return false;
        };
        !name.is_empty()
            && head.eq_ignore_ascii_case(name)
            && (rest == with_port || (rest.is_empty() && port == 80))
    })
}

/// 1 · 2단계: Host와 상대 주소. 모든 경로(없는 경로 포함)가 거친다.
pub(crate) async fn host_and_peer(
    State(shared): State<Arc<Shared>>,
    mut request: Request,
    next: Next,
) -> Response {
    let host_ok = single(request.headers(), &HOST).is_some_and(|host| {
        // 주소 줄에 다른 authority를 적어 Host 확인을 비켜 가지 못하게 한다.
        let same_target = request
            .uri()
            .authority()
            .is_none_or(|authority| authority.as_str() == host);
        same_target && host_allowed(host, &shared.allowed_hosts, shared.port)
    });
    if !host_ok {
        return reject(StatusCode::FORBIDDEN, "허용되지 않은 Host입니다");
    }
    // 전달 헤더는 읽지 않는다. 연결의 실제 상대 주소만 본다.
    let actual = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let peer = actual.map(|addr| match &shared.peer_override {
        Some(replace) => replace(addr),
        None => addr,
    });
    let ip = peer.map(|addr| addr.ip().to_canonical());
    match ip {
        Some(ip) if ip.is_loopback() => {
            request.extensions_mut().insert(Peer(ip));
            next.run(request).await
        }
        _ => reject(
            StatusCode::UNAUTHORIZED,
            "이 컴퓨터에서만 들어올 수 있습니다",
        ),
    }
}

/// `Origin`이 `http://<Host 헤더 값>`과 같은가. 없거나 여러 개면 아니다.
fn same_origin(headers: &HeaderMap) -> bool {
    match (single(headers, &HOST), single(headers, &ORIGIN)) {
        (Some(host), Some(origin)) => origin
            .strip_prefix("http://")
            .is_some_and(|rest| rest == host),
        _ => false,
    }
}

fn cross_origin() -> Response {
    reject(StatusCode::FORBIDDEN, "같은 출처의 요청만 받습니다")
}

/// 3단계: 읽기만 하는 요청(`GET` · `HEAD` · `OPTIONS`)이 아니면 같은 출처여야 한다.
pub(crate) async fn origin_if_changing(request: Request, next: Next) -> Response {
    let reads_only = [Method::GET, Method::HEAD, Method::OPTIONS].contains(request.method());
    if reads_only || same_origin(request.headers()) {
        next.run(request).await
    } else {
        cross_origin()
    }
}

/// 3단계(웹소켓): 언제나 같은 출처여야 한다.
pub(crate) async fn origin_always(request: Request, next: Next) -> Response {
    if same_origin(request.headers()) {
        next.run(request).await
    } else {
        cross_origin()
    }
}

/// `Cookie` 헤더들에서 세션 쿠키 값들을 찾는다.
fn session_values(headers: &HeaderMap) -> impl Iterator<Item = &str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .filter(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value)
}

/// 4단계: 살아 있는 세션이 있어야 한다.
pub(crate) async fn session(
    State(shared): State<Arc<Shared>>,
    request: Request,
    next: Next,
) -> Response {
    let alive = session_values(request.headers()).any(|value| shared.sessions.is_alive(value));
    if alive {
        next.run(request).await
    } else {
        reject(StatusCode::UNAUTHORIZED, "세션이 없습니다")
    }
}

/// `POST /api/session`: 접속 기록을 남기고 세션을 준다(C15 · M6).
///
/// 기록을 남기지 못하면 세션도 주지 않는다.
pub(crate) async fn enter(
    State(shared): State<Arc<Shared>>,
    axum::Extension(Peer(ip)): axum::Extension<Peer>,
) -> Response {
    let access = NewAccess {
        at: shared.hub.clock().now(),
        origin: ip.to_string(),
        action: ACCESS_ENTER.to_owned(),
    };
    let store = shared.hub.store().clone();
    let recorded = tokio::task::spawn_blocking(move || store.record_access(access)).await;
    if !matches!(recorded, Ok(Ok(_))) {
        tracing::error!("접속 기록을 남기지 못해 세션을 주지 않습니다");
        return reject(
            StatusCode::INTERNAL_SERVER_ERROR,
            "접속 기록을 남기지 못했습니다",
        );
    }
    let Ok(value) = shared.sessions.create() else {
        tracing::error!("난수를 얻지 못해 세션을 주지 않습니다");
        return reject(
            StatusCode::INTERNAL_SERVER_ERROR,
            "세션을 만들지 못했습니다",
        );
    };
    let cookie = format!("{SESSION_COOKIE}={value}; HttpOnly; SameSite=Strict; Path=/");
    (StatusCode::NO_CONTENT, [(SET_COOKIE, cookie)]).into_response()
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn host는_허용_이름과_연_포트가_모두_맞아야_한다() {
        let allowed = names(&["localhost", "127.0.0.1", "[::1]"]);
        for good in [
            "localhost:7878",
            "LOCALHOST:7878",
            "127.0.0.1:7878",
            "[::1]:7878",
        ] {
            assert!(host_allowed(good, &allowed, 7878), "{good}");
        }
        for bad in [
            "localhost",
            "localhost:7879",
            "localhost:07878",
            "localhost:7878 ",
            "localhost.evil.example:7878",
            "evil-localhost:7878",
            "127.0.0.1.evil.example:7878",
            "[::1]",
            "한글:7878",
            ":7878",
            "",
        ] {
            assert!(!host_allowed(bad, &allowed, 7878), "{bad}");
        }
        assert!(
            !host_allowed(":7878", &names(&[""]), 7878),
            "빈 이름은 허용이 아니다"
        );
    }

    #[test]
    fn 포트_80으로_열면_포트를_뺀_host도_같은_주소다() {
        let allowed = names(&["localhost"]);
        assert!(host_allowed("localhost", &allowed, 80));
        assert!(host_allowed("localhost:80", &allowed, 80));
        assert!(!host_allowed("localhost", &allowed, 8080));
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_static(value));
        }
        map
    }

    #[test]
    fn origin은_host와_글자까지_같아야_한다() {
        let ok = headers(&[("host", "localhost:1"), ("origin", "http://localhost:1")]);
        assert!(same_origin(&ok));
        for origin in [
            "https://localhost:1",
            "http://localhost:1/",
            "http://LOCALHOST:1",
            "null",
        ] {
            let bad = headers(&[("host", "localhost:1"), ("origin", origin)]);
            assert!(!same_origin(&bad), "{origin}");
        }
        assert!(!same_origin(&headers(&[("host", "localhost:1")])));
        let twice = headers(&[
            ("host", "localhost:1"),
            ("origin", "http://localhost:1"),
            ("origin", "http://evil.example"),
        ]);
        assert!(!same_origin(&twice), "Origin이 여러 개면 믿지 않는다");
    }

    #[test]
    fn 여러_쿠키에서_세션_값만_찾는다() {
        let map = headers(&[
            ("cookie", "theme=dark; pi_session=abc; lang=ko"),
            ("cookie", "pi_session=def"),
            ("cookie", "xpi_session=no; pi_session"),
        ]);
        let found: Vec<&str> = session_values(&map).collect();
        assert_eq!(found, ["abc", "def"]);
    }
}
