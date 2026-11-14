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
//!
//! # 요청마다 보는 것 (이 순서)
//!
//! 1. **Host**: `Host` 헤더가 `<허용 이름>:<연 포트>`가 아니면 거부한다(4xx, DNS rebinding 방지).
//!    허용 이름은 [`GateConfig::allowed_hosts`]다. 포트가 다르거나 없어도 거부한다.
//! 2. **상대 주소**: 연결의 실제 상대 주소가 루프백이 아니면 401이다(접속 키는 T17).
//!    `X-Forwarded-For` · `Forwarded` · `X-Real-IP` 같은 헤더는 읽지 않는다.
//!    공개 경로([`GateBuilder::public_routes`])도 같다.
//! 3. **Origin**: 상태를 바꾸는 요청(`POST` · `PUT` · `PATCH` · `DELETE`)과 웹소켓 연결은
//!    `Origin` 헤더가 `http://<Host 헤더 값>`과 같아야 한다. 다르거나 **없으면** 거부한다(4xx).
//! 4. **세션**: 보호된 경로와 웹소켓은 세션 쿠키([`SESSION_COOKIE`])가 살아 있는 세션이어야 한다.
//!    없거나 모르는 값이면 401.
//!
//! # 세션 (C15 · M6)
//!
//! - 값은 운영체제 난수 256비트를 URL에 안전한 글자(`A-Z a-z 0-9 - _`)로 적은 것이다(43자 이상).
//! - 쿠키: `pi_session=<값>; HttpOnly; SameSite=Strict; Path=/`.
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

mod error;

use std::net::SocketAddr;
use std::sync::Arc;

use pi_store::{Store, UnixMillis};

pub use error::GateError;

/// 기본 포트.
pub const DEFAULT_PORT: u16 = 7878;
/// 기본 허용 Host 이름들(포트 없이).
pub const DEFAULT_ALLOWED_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];
/// 연결 하나에 쌓아 두는 프레임 수의 기본값.
pub const DEFAULT_SEND_BUFFER: usize = 256;
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

/// 시계. 접속 기록과 이벤트의 시각에 쓴다. 테스트가 시각을 정한다.
pub trait Clock: Send + Sync {
    /// 지금 시각.
    fn now(&self) -> UnixMillis;
}

/// 실제 시계(시스템 시각).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UnixMillis {
        todo!()
    }
}

/// 가리기 자리. 화면으로 나가는 글이 모두 이것을 거친다.
pub trait Redactor: Send + Sync {
    /// 글에서 비밀 값을 가린 글을 돌려준다. 가릴 것이 없으면 그대로 돌려준다.
    fn redact(&self, text: &str) -> String;
}

/// 아무것도 가리지 않는다(기본값).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRedaction;

impl Redactor for NoRedaction {
    fn redact(&self, text: &str) -> String {
        todo!()
    }
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
}

impl Default for GateConfig {
    /// `127.0.0.1:`[`DEFAULT_PORT`], [`DEFAULT_ALLOWED_HOSTS`], [`DEFAULT_SEND_BUFFER`].
    fn default() -> Self {
        todo!()
    }
}

impl GateConfig {
    /// `127.0.0.1:port`에 여는 기본 설정.
    pub fn local(port: u16) -> Self {
        todo!()
    }
}

/// 접속 관문을 만드는 자리.
pub struct GateBuilder {
    _private: (),
}

impl GateBuilder {
    /// 시계를 바꾼다. 기본은 [`SystemClock`].
    pub fn clock(self, clock: Arc<dyn Clock>) -> Self {
        todo!()
    }

    /// 가리기 함수를 끼운다. 기본은 [`NoRedaction`].
    pub fn redactor(self, redactor: Arc<dyn Redactor>) -> Self {
        todo!()
    }

    /// 세션 없이 열리는 경로를 더한다(화면 한 장). Host 확인과 상대 주소 확인은 받는다.
    pub fn public_routes(self, routes: axum::Router) -> Self {
        todo!()
    }

    /// 세션이 있어야 열리는 경로를 더한다. 세션이 없으면 401이고 핸들러는 불리지 않는다.
    /// 상태를 바꾸는 요청은 Origin 확인도 받는다.
    pub fn protected_routes(self, routes: axum::Router) -> Self {
        todo!()
    }

    /// 상대 주소를 바꿔 본다. **테스트용이다.** 실행 파일은 부르지 않는다.
    pub fn peer_override(self, peer: PeerOverride) -> Self {
        todo!()
    }

    /// 접속 관문을 만든다. 아직 주소를 열지 않는다.
    pub fn build(self) -> Gate {
        todo!()
    }
}

/// 접속 관문. 아직 주소를 열지 않은 상태다.
pub struct Gate {
    _private: (),
}

impl Gate {
    /// 저장소와 설정으로 만들기 시작한다.
    pub fn builder(store: Store, config: GateConfig) -> GateBuilder {
        todo!()
    }

    /// 이벤트를 내보내는 손잡이. 주소를 열기 전에도 쓸 수 있다(저장은 되고, 연결된 화면이 없을 뿐이다).
    pub fn publisher(&self) -> Publisher {
        todo!()
    }

    /// [`GateConfig::bind`]를 열고 요청을 받기 시작한다. tokio 런타임 안에서 부른다.
    ///
    /// 열지 못하면 [`GateError::Bind`].
    pub async fn serve(self) -> Result<GateServer, GateError> {
        todo!()
    }
}

/// 열려 있는 접속 관문.
pub struct GateServer {
    _private: (),
}

impl GateServer {
    /// 실제로 연 주소(포트 0을 줬으면 고른 포트가 들어 있다).
    pub fn local_addr(&self) -> SocketAddr {
        todo!()
    }

    /// 이벤트를 내보내는 손잡이.
    pub fn publisher(&self) -> Publisher {
        todo!()
    }

    /// 끈다: 새 연결을 받지 않고, 열린 실시간 연결을 닫고, 돌던 작업이 끝나기를 기다린다.
    /// 끝난 뒤에는 그 주소로 연결되지 않는다. 저장소는 닫지 않는다(가진 쪽이 닫는다).
    pub async fn shutdown(self) -> Result<(), GateError> {
        todo!()
    }
}

/// 이벤트를 내보내는 손잡이. 복제해 여러 모듈이 함께 쓴다.
#[derive(Clone)]
pub struct Publisher {
    _private: (),
}

impl Publisher {
    /// 이벤트를 저장하고 연결된 화면에 보낸다. 저장소가 붙인 번호를 돌려준다.
    ///
    /// - 저장이 끝난 뒤에 보낸다. 저장에 실패하면 보내지 않는다.
    /// - 여러 곳에서 함께 불러도 화면에는 번호 순서로 간다.
    /// - 느린 연결을 기다리지 않는다.
    pub async fn publish(&self, kind: &str, payload: serde_json::Value) -> Result<u64, GateError> {
        todo!()
    }

    /// 지나가는 이벤트를 보낸다. 저장하지 않고 번호도 없다(시세처럼 자주 바뀌는 것).
    ///
    /// 종류마다 가장 최근 것 하나를 기억해 두었다가, 새로 연결한 화면에 먼저 보낸다.
    /// 기다리지 않는다(느린 연결은 끊긴다).
    pub fn publish_transient(&self, kind: &str, payload: serde_json::Value) {
        todo!()
    }
}
