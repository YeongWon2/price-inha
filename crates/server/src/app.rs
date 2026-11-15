//! 서버 조립: 저장소 · 비밀 보관소 · 시세 · 접속 관문을 잇는다.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use pi_gate::{Gate, GateConfig, GateServer, Publisher};
use pi_market::{Clock, CredentialSource, MarketAdapter, Symbol, SystemClock, TossClient};
use pi_secrets::SecretStore;
use pi_store::{Store, UnixMillis};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::demo::DemoAdapter;
use crate::desk::{AiParts, Desk, DeskParts};
use crate::feed::{Feed, Pace, Source};
use crate::setup::{AttemptLimit, SymbolsSource, load_watchlist};
use crate::{
    AiChoice, AppError, BrokerKeys, CLI_COMMON_LOCATIONS, KeyVerifier, SecretsRedactor, Settings,
    Upstreams, load_ai_choice, page, setup_http,
};

/// 저장소 파일 이름(보관 폴더 안).
pub const DB_FILE: &str = "price-inha.sqlite";

/// 지나가는 이벤트 종류: 시세 상태. 내용은 `{"state": ...}`.
pub const KIND_FEED_STATUS: &str = "feed_status";
/// 시세 상태: 증권사 키가 없어 시세를 받지 않는다(V31).
pub const FEED_NO_BROKER_KEY: &str = "no_broker_key";
/// 시세 상태: 필수 설정이 빠져 시작하지 않았다(3차). 바깥으로 요청을 보내지 않는다(V31).
/// 무엇이 빠졌는지는 [`crate::KIND_SETUP`] 알림에 있다. 3차부터 시작 전의 시세 상태는 언제나 이것이고,
/// [`FEED_NO_BROKER_KEY`]는 더 알리지 않는다.
pub const FEED_SETUP_REQUIRED: &str = "setup_required";
/// 시세 상태: 증권사가 요청을 거절했다(4xx, 종목 없음 등 — 종목을 확인). "닿지 않음"과 다르다.
/// 받을 수 있는 종목이 하나도 없을 때의 상태다. 일부 종목만 거절됐으면 나머지의 시세는 나오고
/// 시세 상태는 [`FEED_LIVE`]다. 어느 종목이 거절됐는지는 [`KIND_SYMBOLS_REJECTED`]에 있다.
pub const FEED_REQUEST_REJECTED: &str = "request_rejected";
/// 지나가는 이벤트 종류: 증권사가 거절한 종목들. 내용은 `{"symbols": [거절된 종목들]}`.
/// 거절된 종목이 없어지면 빈 목록을 알린다.
pub const KIND_SYMBOLS_REJECTED: &str = "symbols_rejected";
/// 시세 상태: 시연용 가짜 시세다.
pub const FEED_DEMO: &str = "demo";
/// 시세 상태: 증권사에서 받는 실제 시세다. **시세를 실제로 한 번 받은 뒤에만** 알린다.
pub const FEED_LIVE: &str = "live";
/// 시세 상태: 증권사 키는 있고 아직 시세를 한 번도 받지 못했다(연결 중).
pub const FEED_CONNECTING: &str = "connecting";
/// 시세 상태: 증권사가 키를 받지 않는다(인증 실패 · 허용 IP 아님).
pub const FEED_BROKER_REJECTED: &str = "broker_rejected";
/// 시세 상태: 증권사에 닿지 않는다(연결 실패 · 점검 · 장애).
pub const FEED_BROKER_UNREACHABLE: &str = "broker_unreachable";
/// 지나가는 이벤트 종류: 시세 묶음.
/// 내용은 `{"demo": bool, "quotes": [{"symbol", "price"(십진수 글자), "currency", "received_at"(유닉스 밀리초)}]}`.
pub const KIND_QUOTES: &str = "quotes";
/// 지나가는 이벤트 종류: 시세 늦음(N6). 내용은 `{"symbols": [늦은 종목들], "hours_unknown": bool}`.
///
/// `hours_unknown`이 참이면 장 시간을 모르는 채로 판단한 것이다: 달력을 한 번도 받지 못했거나
/// 가진 달력이 지금 시각을 덮지 않는다. 이때는 "장 닫힘"으로 보지 않고, 30초 넘게 오지 않은
/// 시세를 늦은 것으로 본다. 달력을 아는 동안에는 거짓이다.
pub const KIND_QUOTES_STALE: &str = "quotes_stale";

/// 바깥과 닿는 자리. 테스트가 가짜 서버와 시계를 넣는다.
#[derive(Clone)]
pub struct Wiring {
    /// 바깥 서버 설정.
    pub upstreams: Upstreams,
    /// 시계. 시세를 받은 시각과 늦은 시세 판단(N6)에 쓴다.
    pub clock: Arc<dyn Clock>,
    /// 상대 주소를 바꿔 본다(`pi_gate::GateBuilder::peer_override`에 넘긴다). **테스트용이다.**
    /// 실행 파일은 `None`으로 둔다.
    pub peer_override: Option<pi_gate::PeerOverride>,
    /// (6차) `PATH`와 사용자 홈 다음에 CLI 실행 파일을 찾는 자리(이 순서). 기본은 `crate::CLI_COMMON_LOCATIONS`.
    /// 테스트는 임시 폴더를 넣거나 비운다.
    pub cli_locations: Vec<PathBuf>,
}

impl Default for Wiring {
    /// 실제 서버 주소와 실제 시계.
    fn default() -> Self {
        Self {
            upstreams: Upstreams::default(),
            clock: Arc::new(SystemClock),
            peer_override: None,
            cli_locations: CLI_COMMON_LOCATIONS.iter().map(PathBuf::from).collect(),
        }
    }
}

/// 접속 관문의 시계 자리에 시세와 같은 시계를 끼운다(이벤트 시각과 접속 기록).
struct GateClock(Arc<dyn Clock>);

impl pi_gate::Clock for GateClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.now().timestamp_millis())
    }
}

/// 띄울 때 사람에게 알려 주는 주소. `local`은 실제로 연 주소([`App::local_addr`])다.
///
/// 모든 주소로 열었으면(`0.0.0.0` · `::`) 그 주소로는 들어올 수 없으므로 같은 계열의
/// 루프백(`127.0.0.1` · `::1`)과 같은 포트를 돌려준다. 그 밖에는 그대로 돌려준다.
pub fn announce_addr(local: SocketAddr) -> SocketAddr {
    let loopback = match local.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        _ => return local,
    };
    SocketAddr::new(loopback, local.port())
}

/// 이 실행에서 지켜볼 종목과 그 출처.
///
/// 인자 · 환경 변수로 줬으면 그것이 먼저다(저장된 것을 읽지도 덮지도 않는다). 아니면 보관 폴더에
/// 저장된 것을 쓴다. 저장된 파일을 읽지 못하면 없는 것으로 보고 알린다(화면에서 다시 등록하면 고쳐진다).
fn watched_symbols(settings: &Settings) -> (Vec<Symbol>, SymbolsSource) {
    if !settings.symbols.is_empty() {
        return (settings.symbols.clone(), SymbolsSource::Argument);
    }
    match load_watchlist(&settings.home) {
        Ok(saved) if !saved.is_empty() => (saved, SymbolsSource::Saved),
        Ok(_) => (Vec::new(), SymbolsSource::None),
        Err(error) => {
            tracing::warn!(%error, "저장된 종목을 읽지 못해 없는 것으로 봅니다. 화면에서 다시 등록하세요");
            (Vec::new(), SymbolsSource::None)
        }
    }
}

/// 설정 책상을 연다: 지켜볼 종목과 고른 AI 연결을 정하고, 고른 것이 CLI면 한 번 확인한다
/// (모델을 부르지 않는다).
async fn open_desk(
    settings: &Settings,
    wiring: &Wiring,
    secrets: &Arc<SecretStore>,
) -> Result<Arc<Desk>, AppError> {
    let (symbols, source) = watched_symbols(settings);
    // 고른 것이 없으면 API 키다. 읽을 수 없으면 고르지 않은 쪽으로 넘어가지 않고 띄우지 않는다.
    let choice = load_ai_choice(&settings.home)?.unwrap_or_else(AiChoice::api_key);
    let desk = Arc::new(Desk::new(DeskParts {
        secrets: Arc::clone(secrets),
        verifier: KeyVerifier::new(wiring.upstreams.clone()),
        home: settings.home.clone(),
        demo: settings.demo,
        symbols,
        source,
        limit: AttemptLimit::new(settings.register_attempts, settings.register_window),
        symbols_limit: AttemptLimit::new(settings.symbols_attempts, settings.symbols_window),
        ai: AiParts {
            anthropic: wiring.upstreams.anthropic.clone(),
            cli_env: settings.cli_env.clone(),
            user_home: settings.user_home.clone(),
            cli_locations: wiring.cli_locations.clone(),
            choice,
        },
    })?);
    desk.check_cli().await;
    Ok(desk)
}

/// 떠 있는 서버.
///
/// 띄울 때 하는 일:
///
/// 1. 보관 폴더에서 비밀 보관소(`SecretStore::open(home)`)와 저장소(`home/`[`DB_FILE`])를 연다.
/// 2. 지켜볼 종목을 정한다: 인자 · 환경 변수로 줬으면 그것, 아니면 보관 폴더에 저장된 것.
/// 3. 접속 관문을 `bind:port`에 연다. 공개 경로 `GET /`는 `web/index.html`을 내준다
///    (프로그램에 넣어 둔 것). 가리기 자리에는 `SecretStore::redact`를 끼운다.
///    설정 상태는 세션이 있어야 열리는 경로에, 키 · 종목 등록은 그 컴퓨터에서만 열리는 경로
///    (`pi_gate::GateBuilder::local_routes`)에 끼운다([`crate::SETUP_PATH`] 등).
/// 4. 시세:
///    - `--demo`면 필수 설정 없이 가짜 어댑터로 시세를 흘린다. 바깥으로 요청을 보내지 않는다.
///      [`KIND_FEED_STATUS`]는 [`FEED_DEMO`]이고 시세 묶음의 `demo`는 참이다. 값은 받을 때마다 바뀐다.
///    - 아니고 **필수 설정이 모두 등록돼 있으면** 토스 어댑터로 `quote_interval`마다 받는다.
///      받기 전에는 [`FEED_CONNECTING`], 한 번 받은 뒤에 [`FEED_LIVE`]를 알린다.
///      실패하면 [`FEED_BROKER_REJECTED`](인증 실패 · 허용 IP 아님)나
///      [`FEED_BROKER_UNREACHABLE`](연결 실패 · 장애)를 알리고, 다시 시도하는 간격을
///      `retry_initial`부터 실패할 때마다 두 배로 늘려 `retry_max`에서 멈춘다(물러서기).
///      성공하면 `quote_interval`로 돌아온다. 같은 실패가 이어지는 동안 같은 상태를 다시 알리지 않고,
///      기록도 상태가 바뀔 때와 정한 간격마다만 남긴다.
///    - 필수 설정이 빠져 있으면 시작하지 않는다. 바깥으로 요청을 보내지 않고
///      [`FEED_SETUP_REQUIRED`]와 [`crate::KIND_SETUP`]을 알린다(V31). 화면에서 마지막 항목이
///      등록되면 다시 켜지 않아도 바로 시작한다.
/// 5. 받은 시세는 [`KIND_QUOTES`] 지나가는 이벤트로 보낸다. 가진 시세가 `pi_market::is_stale`
///    (기본 30초)이면 [`KIND_QUOTES_STALE`]을 보낸다(N6). 받기에 실패해도 흐름은 이어진다.
pub struct App {
    gate: GateServer,
    /// 설정 책상(6차: 고른 AI 연결의 공급자를 쥔다).
    desk: Arc<Desk>,
    /// 시세 흐름을 멈추는 알림.
    stop: watch::Sender<bool>,
    feed: JoinHandle<()>,
    /// 끌 때 닫는다(놓는다).
    store: Store,
}

impl App {
    /// 서버를 띄운다.
    pub async fn start(settings: Settings, wiring: Wiring) -> Result<App, AppError> {
        // 보관 폴더는 비밀 보관소가 만든다(없으면 0700으로).
        let secrets = Arc::new(SecretStore::open(settings.home.clone())?);
        let store = Store::open(settings.home.join(DB_FILE))?;
        if !settings.bind.is_loopback() {
            tracing::warn!(
                bind = %settings.bind,
                "그 컴퓨터 밖의 주소로 엽니다. 접속 키가 생기기 전까지 밖에서 온 요청은 모두 거부합니다"
            );
        }
        let config = GateConfig {
            bind: SocketAddr::new(settings.bind, settings.port),
            ..GateConfig::default()
        };
        let desk = open_desk(&settings, &wiring, &secrets).await?;
        let mut builder = Gate::builder(store.clone(), config)
            .clock(Arc::new(GateClock(Arc::clone(&wiring.clock))))
            .redactor(Arc::new(SecretsRedactor::new(Arc::clone(&secrets))))
            .public_routes(page::routes())
            .protected_routes(setup_http::status_routes(Arc::clone(&desk)))
            // 키 · 종목 등록은 접속 관문의 확인과 별개로 그 경로 자체가 그 컴퓨터인지 본다.
            .local_routes(setup_http::register_routes(Arc::clone(&desk)));
        if let Some(peer) = wiring.peer_override {
            builder = builder.peer_override(peer);
        }
        let gate = builder.build().serve().await?;

        let source = if settings.demo {
            let demo = DemoAdapter::new(settings.market, Arc::clone(&wiring.clock));
            Source::Demo(Arc::new(demo))
        } else {
            let keys: Arc<dyn CredentialSource> = Arc::new(BrokerKeys::new(secrets));
            let client = TossClient::new(wiring.upstreams.toss, keys, Arc::clone(&wiring.clock));
            let adapter: Arc<dyn MarketAdapter> = Arc::new(client.adapter(settings.market));
            Source::Broker(adapter)
        };
        let feed = Feed::new(
            gate.publisher(),
            wiring.clock,
            settings.market,
            Arc::clone(&desk),
            Pace {
                interval: settings.quote_interval,
                retry_initial: settings.retry_initial,
                retry_max: settings.retry_max,
                calendar_retry: settings.calendar_retry,
            },
        );
        let (stop, mut stopping) = watch::channel(false);
        let feed = tokio::spawn(async move {
            tokio::select! {
                // 알리는 쪽이 사라져도 멈춘다.
                _ = stopping.wait_for(|stop| *stop) => {}
                () = feed.run(source) => {}
            }
        });
        Ok(App {
            gate,
            desk,
            stop,
            feed,
            store,
        })
    }

    /// 실제로 연 주소.
    pub fn local_addr(&self) -> SocketAddr {
        self.gate.local_addr()
    }

    /// (6차) 고른 AI 연결로 만든 공급자(나중에 에이전트 실행기가 쓴다).
    ///
    /// API 키를 골랐으면 `pi_ai::AnthropicProvider`(자격은 [`crate::AiKeys`]), CLI를 골랐으면
    /// `pi_ai::ClaudeCliProvider`([`crate::cli_config`]: 동시 실행 2, 시간 제한은 AI 요청 시간 제한과 같다,
    /// 환경은 [`Settings::cli_env`], 작업 폴더는 [`crate::prepare_cli_work_dir`]). 고른 것이 바뀌면 새로 만든다.
    /// 고르지 않은 쪽으로 넘어가지 않는다.
    pub fn ai_provider(&self) -> Arc<dyn pi_ai::AiProvider> {
        self.desk.ai_provider()
    }

    /// 이벤트를 내보내는 손잡이(다른 모듈이 쓴다).
    pub fn publisher(&self) -> Publisher {
        self.gate.publisher()
    }

    /// 끈다(Ctrl+C): 시세 받기를 멈추고, 연결을 정리하고, 저장소를 닫는다.
    /// 같은 보관 폴더로 다시 띄우면 이벤트 번호가 이어진다(V6).
    pub async fn shutdown(self) -> Result<(), AppError> {
        self.stop.send_replace(true);
        let feed = self.feed.await.map_err(|_| AppError::Task);
        // 시세 흐름이 비정상으로 끝났어도 연결과 저장소는 정리한다.
        let gate = self.gate.shutdown().await;
        drop(self.store);
        gate?;
        feed
    }
}
