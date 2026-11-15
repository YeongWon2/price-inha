//! pi-gate: docs/dev/modules/gate.md 를 따른다.
//!
//! 구성 요소 A2 접속 관문과 실시간 연결. 화면의 모든 요청은 여기를 거친다(D11).
//!
//! # 경로
//!
//! | 경로 | 하는 일 |
//! |---|---|
//! | `POST` [`SESSION_PATH`] | 그 컴퓨터에서 바로 들어오기(C15). 성공하면 204와 세션 쿠키를 준다 |
//! | `GET` [`WS_PATH`]`?`[`LAST_SEQ_PARAM`]`=N` | 실시간 연결(C2). 세션 쿠키가 있어야 한다 |
//! | [`GateBuilder::public_routes`] | 세션 없이 열리는 경로(화면 한 장). Host 확인은 받는다 |
//! | [`GateBuilder::protected_routes`] | 세션이 있어야 열리는 경로. 없으면 401 |
//! | [`GateBuilder::local_routes`] | 세션이 있어야 하고 그 컴퓨터에서만 열리는 경로([`local_only`]) |
//!
//! # 요청마다 보는 것 (이 순서)
//!
//! 1. **Host**: `Host` 헤더가 `<허용 이름>:<연 포트>`가 아니면 거부한다(4xx, DNS rebinding 방지).
//!    허용 이름은 [`GateConfig::allowed_hosts`]다. 포트가 다르거나 없어도 거부한다.
//! 2. **상대 주소**: 그 컴퓨터의 요청(실제 상대 주소가 루프백이고 전달 헤더가 없다)이 아니면
//!    원격 세션이 있어야 하고, 없으면 401이다. 원격 세션은 접속 키로 만든다(9차, [`AccessKeys`]).
//!    `X-Forwarded-For` · `Forwarded` · `X-Real-IP` 같은 헤더의 값은 읽지 않는다. 있기만 해도 루프백으로 치지 않는다.
//!    공개 경로([`GateBuilder::public_routes`])도 같다(세션을 만드는 경로만 키를 받는다).
//! 3. **Origin**: 상태를 바꾸는 요청(`POST` · `PUT` · `PATCH` · `DELETE`)과 웹소켓 연결은
//!    `Origin` 헤더가 `http://<Host 헤더 값>`과 같아야 한다. 다르거나 **없으면** 거부한다(4xx).
//! 4. **세션**: 보호된 경로와 웹소켓은 세션 쿠키([`SESSION_COOKIE`])가 살아 있는 세션이어야 한다.
//!    없거나 모르는 값이면 401.
//!
//! # 세션 (C15 · M6)
//!
//! - 값은 운영체제 난수 256비트를 URL에 안전한 글자(`A-Z a-z 0-9 - _`)로 적은 것이다(43자 이상).
//! - 쿠키: `pi_session=<값>; HttpOnly; SameSite=Strict; Path=/`. (9차) 접속 키로 만든 원격 세션은
//!   여기에 `Max-Age=2592000`(30일)이 붙고, 키를 바꾸면 끊긴다([`AccessKeys`]).
//! - 서버에는 값의 SHA-256 해시만 메모리에 둔다. 값도 해시도 저장소(파일)에 쓰지 않는다.
//!   그래서 프로그램을 다시 켜면 세션은 사라지고 화면은 다시 들어온다.
//! - 세션을 만들 때마다 접속 기록 한 줄을 남긴다: 시각은 [`Clock::now`], 어디서는 상대 IP 글자
//!   (포트 없이, 예 `127.0.0.1`), 무엇을은 [`ACCESS_ENTER`]. 거부된 요청은 [`ACCESS_ENTER`]로 남기지 않는다.
//!
//! # 실시간 연결 (C2 · D11 · E3)
//!
//! 프레임은 모두 JSON 글자 프레임이다.
//!
//! - 저장되는 이벤트: `{"seq": 번호, "kind": 종류, "payload": 내용, "at": 유닉스 밀리초}`.
//!   `at`은 [`Publisher::publish`]가 불린 때의 [`Clock::now`]이고 이벤트와 함께 저장된다
//!   (다시 보내도, 프로그램을 다시 켠 뒤에 보내도 같은 값이다).
//! - 지나가는 이벤트: `{"kind": 종류, "payload": 내용, "at": 유닉스 밀리초}`. `seq` 칸이 없다.
//!
//! 4차(store 3차 검토 · 원본 대조 C9 · D17):
//!
//! - 저장되는 이벤트는 `pi_store::Store::append_at`으로 시각 칸에 시각을 쓰고, 내용은 받은 그대로
//!   저장한다(시각 봉투 `{"at", "payload"}`로 싸지 않는다). 프레임의 `at`은 저장소의 시각 칸에서
//!   온다(`pi_store::Store::read_events`). 시각 칸이 비어 있으면 `null`이다.
//! - 버전 2 이하에서 올라온 옛 봉투 줄(`{"at": 정수, "payload": …}`)은 예전처럼 풀어서 보낸다
//!   (다시 보내도 시각과 내용이 같다).
//!   (5차, 2026-11-14 사용자 결정 "경계 번호로") 푸는 줄은 저장소의 옛 형식 경계 번호
//!   이하의 줄 가운데 옛 봉투 줄뿐이다. (7차) 옛 봉투 줄인지는 저장소가 원래 글자로 정한다
//!   (`pi_store::Store::legacy_envelope_seqs`, 칸 이름 겹침 · `at` 범위까지). 경계 뒤의 줄은 내용 그대로 보내고,
//!   경계가 없는 저장소에서는 아무것도 풀지 않는다. 바로 보내기와 다시 보내기가 같다.
//! - 프레임의 `kind`는 저장소가 저장한 이름이다(저장소가 앞뒤 공백을 떼어 저장한다).
//! - 종류 이름은 [`valid_event_kind`] 규칙을 지킨다. 지나가는 이벤트도 검사하고, 어긋나면 보내지 않는다.
//! - 연결마다 [`GateConfig::keepalive_interval`]마다 ping을 보내고, [`GateConfig::pong_timeout`] 안에
//!   pong이 없으면 그 연결을 끊는다(시세가 흐르지 않을 때도).
//! - 모든 응답에 [`SECURITY_HEADERS`]를 붙인다(처리기가 붙인 것은 덮어쓰지 않는다).
//!
//! 연결하면 이 순서로 보낸다:
//!
//! 1. 종류마다 가장 최근의 지나가는 이벤트 하나씩(없으면 없다).
//! 2. 번호가 `last_seq`보다 큰 저장된 이벤트를 번호 순서로 전부.
//! 3. 그 뒤로 새 이벤트(저장되는 것과 지나가는 것)를 생기는 대로.
//!
//! 저장되는 이벤트의 번호는 한 연결 안에서 `last_seq + 1`부터 하나씩 커진다(빠지거나 겹치지 않는다).
//! `last_seq`가 숫자가 아니면 연결을 받지 않는다(4xx).
//!
//! 느린 연결: 연결마다 보낼 프레임을 [`GateConfig::send_buffer`]개까지만 쌓는다. 넘치면 그 연결만 끊는다.
//! 발행하는 쪽과 다른 연결은 기다리지 않는다. 끊긴 화면은 마지막 번호로 다시 이어 받는다.
//!
//! # 가리기
//!
//! 화면으로 나가는 모든 프레임 글자는 [`Redactor::redact`]를 거친다. `pi-gate`는 `pi-secrets`를
//! 직접 쓰지 않고, 조립하는 쪽이 `SecretStore::redact`를 끼운다.
//!
//! - **가린 뒤에도 프레임은 JSON이다.** 가릴 값이 숫자뿐이라 내용 속 JSON 숫자와 겹쳐도
//!   가려진 글자는 언제나 문자열 자리에 들어간다. `seq` · `at` 같은 프레임의 뼈대는 깨지지 않는다.
//! - **저장하기 전에도 가린다(N8 · E38).** [`Publisher::publish`]는 내용을 가린 뒤에 저장소에 쓴다.
//!   저장된 것을 다시 보낼 때도 그때의 가리기를 한 번 더 거친다(나중에 등록된 값도 가려진다).
//!
//! # 크기 상한
//!
//! 내용을 JSON으로 적은 글이 [`GateConfig::max_frame_bytes`]바이트를 넘으면
//! [`Publisher::publish`]는 [`GateError::FrameTooLarge`]로 거부한다. 저장하지 않고 번호도 쓰지 않는다.
//! [`Publisher::publish_transient`]는 돌려줄 오류가 없으므로 보내지 않고 버린다(기억하지도 않는다).

mod access;
mod enter;
mod error;
mod frame;
mod guard;
mod hub;
mod port;
mod server;
mod session;
mod strikes;
mod ws;

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use pi_store::Store;

use crate::hub::Hub;

pub use access::{
    ACCESS_KEY_ALPHABET, ACCESS_KEY_CHANGED, ACCESS_KEY_FIELD, ACCESS_KEY_LEN,
    ACCESS_KEY_SALT_BYTES, ACCESS_KEY_WRONG, ACCESS_LOCKED, AccessKeys, DEFAULT_LOOPBACK_DELAY_MAX,
    DEFAULT_LOOPBACK_DELAY_START, FORWARDING_HEADERS, IssuedKey, LOCKED_STATUS, LOCKOUT, LockAlert,
    LockNotice, LockedDevice, MAX_KEY_FAILURES, NoLockAlert, REMOTE_SESSION_MAX_AGE,
};
pub use error::GateError;
pub use guard::{LOCAL_ONLY_STATUS, Peer, local_only};
pub use port::{Clock, NoRedaction, Redactor, SystemClock};
pub use server::{Gate, GateServer, Publisher};

/// 기본 포트.
pub const DEFAULT_PORT: u16 = 7878;
/// 기본 허용 Host 이름들(포트 없이).
pub const DEFAULT_ALLOWED_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];
/// 연결 하나에 쌓아 두는 프레임 수의 기본값.
pub const DEFAULT_SEND_BUFFER: usize = 256;
/// 발행하는 내용 하나의 크기 상한 기본값(바이트).
pub const DEFAULT_MAX_FRAME_BYTES: usize = 1024 * 1024;
/// 세션 쿠키 이름.
pub const SESSION_COOKIE: &str = "pi_session";
/// 세션을 만드는 경로(`POST`).
pub const SESSION_PATH: &str = "/api/session";
/// 웹소켓 경로(`GET`).
pub const WS_PATH: &str = "/ws";
/// "마지막으로 받은 이벤트 번호"를 주는 쿼리 이름.
pub const LAST_SEQ_PARAM: &str = "last_seq";
/// 접속 기록의 "무엇을": 들어옴.
pub const ACCESS_ENTER: &str = "들어옴";
/// 실시간 연결의 살아 있음 신호(ping) 간격 기본값. (4차, 원본 대조 D17)
///
/// 원본(Hermes api_server)은 조용할 때 10초마다 keepalive를 보낸다
/// (`CHAT_COMPLETIONS_SSE_KEEPALIVE_SECONDS = 10.0`). 우리는 SSE가 아니라 WebSocket이라(D11)
/// 같은 뜻을 WebSocket ping으로 낸다.
pub const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
/// ping을 보낸 뒤 pong을 기다리는 가장 긴 시간의 기본값. 넘으면 그 연결을 끊는다. (4차)
///
/// 원본에는 없는 값이다(SSE는 답을 받지 않는다). 간격과 같게 둔다.
pub const DEFAULT_PONG_TIMEOUT: Duration = Duration::from_secs(10);
/// 모든 응답에 붙이는 보안 응답 헤더(이름, 값). (4차, 원본 대조 D21)
///
/// 원본(Hermes api_server의 `_SECURITY_HEADERS`)과 같은 이름 · 값이다. 오류 응답 · 401 · 403에도
/// 붙는다. 처리기가 이미 같은 이름의 헤더를 붙였으면 덮어쓰지 않는다(원본의 `setdefault`).
/// 화면의 `Content-Security-Policy`는 화면을 내주는 쪽이 정한다.
pub const SECURITY_HEADERS: [(&str, &str); 3] = [
    ("X-Content-Type-Options", "nosniff"),
    ("X-Frame-Options", "DENY"),
    ("Referrer-Policy", "no-referrer"),
];

/// 이벤트 종류 이름이 규칙에 맞는가. (4차, 원본 대조 C9)
///
/// 규칙은 저장소와 같다: `^[a-z][a-z0-9_.-]*$`이고 `pi_store::EVENT_KIND_MAX_CHARS`자 이하.
/// 받은 글자 그대로 본다(앞뒤 공백을 떼지 않는다). 지나가는 이벤트도 이 검사를 거치고,
/// 맞지 않으면 보내지도 기억하지도 않는다. (5차) 지나가는 이벤트는 앞뒤 공백을 먼저 뗀 이름으로 거친다.
/// ```
/// # use pi_gate::valid_event_kind;
/// assert!(valid_event_kind("feed_status") && !valid_event_kind("Feed") && !valid_event_kind(""));
/// assert!(!valid_event_kind(" feed_status ")); // 이 검사 자체는 떼지 않는다
/// assert!(valid_event_kind(" feed_status ".trim()) && !valid_event_kind(" feed status ".trim()));
/// ```
pub fn valid_event_kind(kind: &str) -> bool {
    frame::kind_follows_rule(kind)
}

/// 연결의 실제 상대 주소를 받아 "상대 주소로 볼 값"을 돌려주는 함수.
/// 테스트가 루프백이 아닌 상대를 흉내 낼 때만 쓴다.
pub type PeerOverride = Arc<dyn Fn(SocketAddr) -> SocketAddr + Send + Sync>;

/// 접속 관문 설정.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateConfig {
    /// 여는 주소. 기본은 `127.0.0.1:`[`DEFAULT_PORT`]다(D2). 다른 주소는 이 칸에 분명히 줘야 한다.
    /// 포트 0이면 빈 포트를 고른다.
    pub bind: SocketAddr,
    /// 허용하는 Host 이름(포트 없이). 기본은 [`DEFAULT_ALLOWED_HOSTS`].
    pub allowed_hosts: Vec<String>,
    /// 연결 하나에 쌓아 두는 프레임 수. 넘치면 그 연결만 끊는다. 기본은 [`DEFAULT_SEND_BUFFER`].
    pub send_buffer: usize,
    /// 발행하는 내용 하나(JSON으로 적은 글)의 크기 상한(바이트). 넘으면 발행을 거부한다.
    /// 기본은 [`DEFAULT_MAX_FRAME_BYTES`].
    pub max_frame_bytes: usize,
    /// 실시간 연결마다 ping을 보내는 간격. 시세가 흐르지 않을 때도 보낸다. (4차, D17)
    /// 기본은 [`DEFAULT_KEEPALIVE_INTERVAL`].
    /// (5차) 0이면 [`Gate::serve`]가 [`GateError::InvalidConfig`]`{ field: "keepalive_interval" }`로 거부한다.
    pub keepalive_interval: Duration,
    /// ping을 보낸 뒤 이 시간 안에 pong이 없으면 그 연결을 끊는다. (4차, D17)
    /// 기본은 [`DEFAULT_PONG_TIMEOUT`].
    /// (5차) 0이면 [`Gate::serve`]가 [`GateError::InvalidConfig`]`{ field: "pong_timeout" }`로 거부한다.
    /// (5차) 들어온 프레임(pong)은 기한 판단보다 먼저 읽는다: 보낼 것이 밀려 있거나 프레임 쓰기가
    /// 느려 기한이 지난 뒤에 돌아왔어도, 그사이 pong이 들어와 있었으면 끊지 않는다.
    pub pong_timeout: Duration,
    /// (9차) 그 컴퓨터가 키를 틀렸을 때 응답을 늦추는 시간의 시작값. 틀릴 때마다 두 배가 되고
    /// [`GateConfig::loopback_delay_max`]에서 멈춘다(원본 OpenClaw). 그 컴퓨터는 잠그지 않는다.
    /// 기본은 [`DEFAULT_LOOPBACK_DELAY_START`].
    pub loopback_delay_start: Duration,
    /// (9차) 그 컴퓨터를 늦추는 시간의 상한. 기본은 [`DEFAULT_LOOPBACK_DELAY_MAX`].
    pub loopback_delay_max: Duration,
}

impl Default for GateConfig {
    /// `127.0.0.1:`[`DEFAULT_PORT`], [`DEFAULT_ALLOWED_HOSTS`], [`DEFAULT_SEND_BUFFER`],
    /// [`DEFAULT_MAX_FRAME_BYTES`], [`DEFAULT_KEEPALIVE_INTERVAL`], [`DEFAULT_PONG_TIMEOUT`].
    fn default() -> Self {
        Self::local(DEFAULT_PORT)
    }
}

impl GateConfig {
    /// `127.0.0.1:port`에 여는 기본 설정.
    pub fn local(port: u16) -> Self {
        Self {
            bind: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            allowed_hosts: DEFAULT_ALLOWED_HOSTS
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            send_buffer: DEFAULT_SEND_BUFFER,
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            keepalive_interval: DEFAULT_KEEPALIVE_INTERVAL,
            pong_timeout: DEFAULT_PONG_TIMEOUT,
            loopback_delay_start: DEFAULT_LOOPBACK_DELAY_START,
            loopback_delay_max: DEFAULT_LOOPBACK_DELAY_MAX,
        }
    }
}

/// 접속 관문을 만드는 자리.
pub struct GateBuilder {
    store: Store,
    config: GateConfig,
    clock: Arc<dyn Clock>,
    redactor: Arc<dyn Redactor>,
    public: Router,
    protected: Router,
    peer_override: Option<PeerOverride>,
    lock_alert: Arc<dyn LockAlert>,
}

impl GateBuilder {
    /// 시계를 바꾼다. 기본은 [`SystemClock`].
    pub fn clock(self, clock: Arc<dyn Clock>) -> Self {
        Self { clock, ..self }
    }

    /// 가리기 함수를 끼운다. 기본은 [`NoRedaction`].
    pub fn redactor(self, redactor: Arc<dyn Redactor>) -> Self {
        Self { redactor, ..self }
    }

    /// 세션 없이 열리는 경로를 더한다(화면 한 장). Host 확인과 상대 주소 확인은 받는다.
    pub fn public_routes(self, routes: axum::Router) -> Self {
        Self {
            public: self.public.merge(routes),
            ..self
        }
    }

    /// 세션이 있어야 열리는 경로를 더한다. 세션이 없으면 401이고 핸들러는 불리지 않는다.
    /// 상태를 바꾸는 요청은 Origin 확인도 받는다.
    pub fn protected_routes(self, routes: axum::Router) -> Self {
        Self {
            protected: self.protected.merge(routes),
            ..self
        }
    }

    /// 세션이 있어야 하고 **그 컴퓨터에서만** 열리는 경로를 더한다(키 등록처럼 밖에서는
    /// 접속 키가 있어도 할 수 없는 일).
    ///
    /// [`GateBuilder::protected_routes`]와 같은 확인(세션, 상태를 바꾸는 요청의 Origin)을 받고,
    /// 거기에 [`local_only`]를 씌운다: 세션이 없으면 401, 다른 출처면 4xx, 상대가 루프백이
    /// 아니면 거부이고 어느 경우에도 핸들러는 불리지 않는다.
    pub fn local_routes(self, routes: axum::Router) -> Self {
        self.protected_routes(local_only(routes))
    }

    /// 잠금 알림 자리를 끼운다(9차, E2). 기본은 [`NoLockAlert`].
    pub fn lock_alert(self, alert: Arc<dyn LockAlert>) -> Self {
        Self {
            lock_alert: alert,
            ..self
        }
    }

    /// 상대 주소를 바꿔 본다. **테스트용이다.** 실행 파일은 부르지 않는다.
    pub fn peer_override(self, peer: PeerOverride) -> Self {
        Self {
            peer_override: Some(peer),
            ..self
        }
    }

    /// 접속 관문을 만든다. 아직 주소를 열지 않는다.
    pub fn build(self) -> Gate {
        let hub = Arc::new(Hub::new(
            self.store,
            Arc::clone(&self.clock),
            self.redactor,
            self.config.send_buffer,
            self.config.max_frame_bytes,
        ));
        Gate {
            hub,
            config: self.config,
            public: self.public,
            protected: self.protected,
            peer_override: self.peer_override,
            clock: self.clock,
            lock_alert: self.lock_alert,
        }
    }
}
