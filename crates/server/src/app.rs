//! 서버 조립: 저장소 · 비밀 보관소 · 시세 · 접속 관문을 잇는다.

use std::net::SocketAddr;
use std::sync::Arc;

use pi_gate::Publisher;
use pi_market::Clock;

use crate::{AppError, Settings, Upstreams};

/// 저장소 파일 이름(보관 폴더 안).
pub const DB_FILE: &str = "price-inha.sqlite";

/// 지나가는 이벤트 종류: 시세 상태. 내용은 `{"state": ...}`.
pub const KIND_FEED_STATUS: &str = "feed_status";
/// 시세 상태: 증권사 키가 없어 시세를 받지 않는다(V31).
pub const FEED_NO_BROKER_KEY: &str = "no_broker_key";
/// 시세 상태: 시연용 가짜 시세다.
pub const FEED_DEMO: &str = "demo";
/// 시세 상태: 증권사에서 받는 실제 시세다.
pub const FEED_LIVE: &str = "live";
/// 지나가는 이벤트 종류: 시세 묶음.
/// 내용은 `{"demo": bool, "quotes": [{"symbol", "price"(십진수 글자), "currency", "received_at"(유닉스 밀리초)}]}`.
pub const KIND_QUOTES: &str = "quotes";
/// 지나가는 이벤트 종류: 시세 늦음(N6). 내용은 `{"symbols": [늦은 종목들]}`.
pub const KIND_QUOTES_STALE: &str = "quotes_stale";

/// 바깥과 닿는 자리. 테스트가 가짜 서버와 시계를 넣는다.
#[derive(Clone)]
pub struct Wiring {
    /// 바깥 서버 설정.
    pub upstreams: Upstreams,
    /// 시계. 시세를 받은 시각과 늦은 시세 판단(N6)에 쓴다.
    pub clock: Arc<dyn Clock>,
}

impl Default for Wiring {
    /// 실제 서버 주소와 실제 시계.
    fn default() -> Self {
        todo!()
    }
}

/// 떠 있는 서버.
///
/// 띄울 때 하는 일:
///
/// 1. 보관 폴더에서 비밀 보관소(`SecretStore::open(home)`)와 저장소(`home/`[`DB_FILE`])를 연다.
/// 2. 접속 관문을 `bind:port`에 연다. 공개 경로 `GET /`는 `web/index.html`을 내준다
///    (프로그램에 넣어 둔 것). 가리기 자리에는 `SecretStore::redact`를 끼운다.
/// 3. 시세:
///    - `--demo`면 가짜 어댑터로 시세를 흘린다. 바깥으로 요청을 보내지 않는다.
///      [`KIND_FEED_STATUS`]는 [`FEED_DEMO`]이고 시세 묶음의 `demo`는 참이다. 값은 받을 때마다 바뀐다.
///    - 아니고 증권사 키가 등록돼 있으면 토스 어댑터로 `quote_interval`마다 받는다([`FEED_LIVE`]).
///    - 아니면 받지 않는다. 바깥으로 요청을 보내지 않고 [`FEED_NO_BROKER_KEY`]를 알린다(V31).
/// 4. 받은 시세는 [`KIND_QUOTES`] 지나가는 이벤트로 보낸다. 가진 시세가 `pi_market::is_stale`
///    (기본 30초)이면 [`KIND_QUOTES_STALE`]을 보낸다(N6). 받기에 실패해도 흐름은 이어진다.
pub struct App {
    _private: (),
}

impl App {
    /// 서버를 띄운다.
    pub async fn start(settings: Settings, wiring: Wiring) -> Result<App, AppError> {
        todo!()
    }

    /// 실제로 연 주소.
    pub fn local_addr(&self) -> SocketAddr {
        todo!()
    }

    /// 이벤트를 내보내는 손잡이(다른 모듈이 쓴다).
    pub fn publisher(&self) -> Publisher {
        todo!()
    }

    /// 끈다(Ctrl+C): 시세 받기를 멈추고, 연결을 정리하고, 저장소를 닫는다.
    /// 같은 보관 폴더로 다시 띄우면 이벤트 번호가 이어진다(V6).
    pub async fn shutdown(self) -> Result<(), AppError> {
        todo!()
    }
}
