//! 요청마다 보는 것: Host → 상대 주소 → Origin → 세션. 그리고 세션을 만드는 경로.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::header::{COOKIE, HOST, ORIGIN};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tokio::sync::{mpsc, watch};

use crate::access::Access;
use crate::hub::Hub;
use crate::session::SessionKind;
use crate::{FORWARDING_HEADERS, PeerOverride, SESSION_COOKIE, SESSION_PATH};

/// 요청 처리에 함께 쓰는 것들.
pub(crate) struct Shared {
    pub(crate) hub: Arc<Hub>,
    /// 접속 키 · 세션 · 잠금(9차).
    pub(crate) access: Arc<Access>,
    /// 허용하는 Host 이름(포트 없이).
    pub(crate) allowed_hosts: Vec<String>,
    /// (10차) https 프록시 뒤에서 들어오는 원격 이름(포트 없이).
    pub(crate) remote_hosts: Vec<String>,
    /// 실제로 연 포트.
    pub(crate) port: u16,
    pub(crate) peer_override: Option<PeerOverride>,
    /// 끄기 알림. 참이 되거나 보내는 쪽이 사라지면 멈춘다.
    pub(crate) stop: watch::Receiver<bool>,
    /// 실시간 연결이 모두 끝났는지 아는 데 쓴다(쥐고만 있는다).
    pub(crate) _done: mpsc::Sender<()>,
    /// 열려 있는 실시간 연결 수.
    pub(crate) live: Arc<AtomicUsize>,
    /// 실시간 연결의 살아 있음 신호(ping) 간격과 pong 기다림.
    pub(crate) keepalive: crate::ws::Keepalive,
}

/// 확인을 지난 상대 주소(포트 없이). 접속 관문이 요청의 확장(extension)에 넣는다.
///
/// 뒤의 핸들러와 [`local_only`]가 읽는다: 접속 기록의 "어디서", 그리고 "그 컴퓨터에서 온
/// 요청인가"의 판단. IPv4로 적을 수 있는 IPv6 주소는 IPv4로 바꾼 값이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peer(pub IpAddr);

/// 그 컴퓨터의 요청이 아니다(상대가 루프백이 아니거나 전달 헤더가 있다). 관문이 확장에 넣는다(9차).
/// 이 표시가 있으면 [`Peer`]가 루프백이어도 [`local_only`]는 닫는다.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Outside;

/// [`local_only`]가 그 컴퓨터 밖의 요청을 거부할 때의 HTTP 상태.
pub const LOCAL_ONLY_STATUS: u16 = 403;

/// 그 컴퓨터에서 온 요청만 받는 경로로 만든다(3차 규칙 4: 키 등록은 그 컴퓨터에서만).
///
/// 접속 관문의 "보호된 경로" 확인과 **별개로**, 이 경로 자체가 한 번 더 본다:
/// 요청의 확장에 든 [`Peer`]가 루프백이면 핸들러로 넘기고, 루프백이 아니거나 [`Peer`]가
/// **없으면** [`LOCAL_ONLY_STATUS`]로 거부한다(핸들러는 불리지 않는다).
///
/// (9차) 접속 키로 원격 세션을 가진 다른 기기의 요청도 관문을 지나지만, 이 경로들은 닫혀 있다.
/// 관문이 그 컴퓨터의 요청이 아니라고 본 요청(전달 헤더가 있는 루프백 포함)도 닫는다.
/// 전달 헤더(`X-Forwarded-For` 등)의 값은 읽지 않는다.
pub fn local_only(routes: axum::Router) -> axum::Router {
    routes.layer(axum::middleware::from_fn(loopback_peer))
}

/// [`local_only`]의 확인: 확장의 [`Peer`]가 루프백이고 관문이 밖이라고 표시하지 않았을 때만
/// 넘긴다. [`Peer`]가 없으면 닫는다.
async fn loopback_peer(request: Request, next: Next) -> Response {
    let extensions = request.extensions();
    let local = extensions.get::<Outside>().is_none()
        && extensions
            .get::<Peer>()
            .is_some_and(|Peer(ip)| ip.to_canonical().is_loopback());
    if local {
        next.run(request).await
    } else {
        let status = StatusCode::from_u16(LOCAL_ONLY_STATUS).unwrap_or(StatusCode::FORBIDDEN);
        reject(status, "이 컴퓨터에서만 할 수 있습니다")
    }
}

/// 가장 바깥 층: 모든 응답(오류 · 거부 · 없는 경로 포함)에 [`crate::SECURITY_HEADERS`]를 붙인다.
/// 처리기가 이미 붙인 같은 이름의 헤더는 그대로 둔다(원본의 `setdefault`).
pub(crate) async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in crate::SECURITY_HEADERS {
        if let Ok(name) = HeaderName::try_from(name)
            && !headers.contains_key(&name)
        {
            headers.insert(name, HeaderValue::from_static(value));
        }
    }
    response
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

/// (10차) `host`가 원격 이름인가: 포트 없는 `<이름>`이나 `<이름>:<연 포트>`.
fn remote_host(host: &str, remote: &[String], port: u16) -> bool {
    let with_port = format!(":{port}");
    remote.iter().any(|name| {
        let Some((head, rest)) = host.split_at_checked(name.len()) else {
            return false;
        };
        !name.is_empty()
            && head.eq_ignore_ascii_case(name)
            && (rest.is_empty() || rest == with_port)
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
        same_target
            && (host_allowed(host, &shared.allowed_hosts, shared.port)
                || remote_host(host, &shared.remote_hosts, shared.port))
    });
    if !host_ok {
        return reject(StatusCode::FORBIDDEN, "허용되지 않은 Host입니다");
    }
    // 전달 헤더의 값은 읽지 않는다. 연결의 실제 상대 주소만 본다.
    let actual = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let peer = actual.map(|addr| match &shared.peer_override {
        Some(replace) => replace(addr),
        None => addr,
    });
    let Some(ip) = peer.map(|addr| addr.ip().to_canonical()) else {
        return reject(StatusCode::UNAUTHORIZED, "상대 주소를 알 수 없습니다");
    };
    request.extensions_mut().insert(Peer(ip));
    if ip.is_loopback() && !forwarded(request.headers()) {
        return next.run(request).await;
    }
    // 그 컴퓨터 밖: 접속 키로 세션을 만드는 경로만 열고, 나머지는 원격 세션이 있어야 한다.
    request.extensions_mut().insert(Outside);
    let entering = request.method() == Method::POST && request.uri().path() == SESSION_PATH;
    if entering {
        return next.run(request).await;
    }
    // (11차, V41) 다른 프로세스가 저장소의 키 세대를 올렸을 수 있으니 세션을 보기 전에 저장소의
    // 지금 세대를 본다. 읽지 못하면 닫힌 쪽(세션 없음과 같은 401)이다.
    if !shared.access.catch_up().await {
        return reject(StatusCode::UNAUTHORIZED, "접속 키로 들어와야 합니다");
    }
    if find_session(&shared, request.headers(), true).is_some() {
        next.run(request).await
    } else {
        reject(StatusCode::UNAUTHORIZED, "접속 키로 들어와야 합니다")
    }
}

/// 전달 헤더([`FORWARDING_HEADERS`])가 하나라도 있는가(값은 보지 않는다).
fn forwarded(headers: &HeaderMap) -> bool {
    FORWARDING_HEADERS
        .iter()
        .any(|name| headers.contains_key(*name))
}

/// 쿠키의 세션 값들 가운데 살아 있는 세션의 종류. `outside`면 원격 세션만 친다.
fn find_session(shared: &Shared, headers: &HeaderMap, outside: bool) -> Option<SessionKind> {
    let now = shared.access.session_now();
    session_values(headers)
        .filter_map(|value| shared.access.sessions.find(value, now))
        .find(|kind| !outside || matches!(kind, SessionKind::Remote { .. }))
}

/// `Origin`이 `http://<Host 헤더 값>`과 같은가. 없거나 여러 개면 아니다.
/// (10차) Host가 원격 이름이면 `https://<Host 헤더 값>`도 같은 출처다.
fn same_origin(headers: &HeaderMap, remote: &[String], port: u16) -> bool {
    match (single(headers, &HOST), single(headers, &ORIGIN)) {
        (Some(host), Some(origin)) => {
            origin
                .strip_prefix("http://")
                .is_some_and(|rest| rest == host)
                || (remote_host(host, remote, port)
                    && origin
                        .strip_prefix("https://")
                        .is_some_and(|rest| rest == host))
        }
        _ => false,
    }
}

fn cross_origin() -> Response {
    reject(StatusCode::FORBIDDEN, "같은 출처의 요청만 받습니다")
}

/// 3단계: 읽기만 하는 요청(`GET` · `HEAD` · `OPTIONS`)이 아니면 같은 출처여야 한다.
pub(crate) async fn origin_if_changing(
    State(shared): State<Arc<Shared>>,
    request: Request,
    next: Next,
) -> Response {
    let reads_only = [Method::GET, Method::HEAD, Method::OPTIONS].contains(request.method());
    if reads_only || same_origin(request.headers(), &shared.remote_hosts, shared.port) {
        next.run(request).await
    } else {
        cross_origin()
    }
}

/// 3단계(웹소켓): 언제나 같은 출처여야 한다.
pub(crate) async fn origin_always(
    State(shared): State<Arc<Shared>>,
    request: Request,
    next: Next,
) -> Response {
    if same_origin(request.headers(), &shared.remote_hosts, shared.port) {
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

/// 4단계: 살아 있는 세션이 있어야 한다. 그 컴퓨터 밖의 요청은 원격 세션이어야 한다.
/// 지난 요청의 확장에 세션 종류를 넣는다(실시간 연결이 키를 바꿀 때 닫히는 데 쓴다).
pub(crate) async fn session(
    State(shared): State<Arc<Shared>>,
    mut request: Request,
    next: Next,
) -> Response {
    let outside = request.extensions().get::<Outside>().is_some();
    match find_session(&shared, request.headers(), outside) {
        Some(kind) => {
            request.extensions_mut().insert(kind);
            next.run(request).await
        }
        None => reject(StatusCode::UNAUTHORIZED, "세션이 없습니다"),
    }
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
        assert!(same_origin(&ok, &[], 1));
        for origin in [
            "https://localhost:1",
            "http://localhost:1/",
            "http://LOCALHOST:1",
            "null",
        ] {
            let bad = headers(&[("host", "localhost:1"), ("origin", origin)]);
            assert!(!same_origin(&bad, &[], 1), "{origin}");
        }
        assert!(!same_origin(&headers(&[("host", "localhost:1")]), &[], 1));
        let twice = headers(&[
            ("host", "localhost:1"),
            ("origin", "http://localhost:1"),
            ("origin", "http://evil.example"),
        ]);
        assert!(
            !same_origin(&twice, &[], 1),
            "Origin이 여러 개면 믿지 않는다"
        );
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
