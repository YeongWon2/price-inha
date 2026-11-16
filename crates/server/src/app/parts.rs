//! 서버 조립의 부품: 접속 관문 · 설정 책상 · 시세 흐름 · CLI 확인을 연다.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use pi_gate::{Gate, GateConfig, GateServer};
use pi_market::{Clock, CredentialSource, MarketAdapter, Symbol, TossClient};
use pi_secrets::SecretStore;
use pi_store::{Store, UnixMillis};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::{CLI_CHECK_GRACE, Wiring};
use crate::access_http::{self, AccessSlot};
use crate::ai_link::Leftovers;
use crate::demo::DemoAdapter;
use crate::desk::{AiParts, Desk, DeskParts, prepare_work};
use crate::feed::{Feed, Pace, Source, StaleHook};
use crate::setup::{AttemptLimit, SymbolsSource, load_watchlist};
use crate::{
    AiChoice, AppError, BrokerKeys, Crew, KeyVerifier, SecretsRedactor, Settings, Trading,
    load_ai_choice, page, setup_http,
};

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
/// 저장된 것을 쓴다.
///
/// (8차 2번) 저장된 파일을 읽지 못하거나 지금 규칙(`pi_market::Symbol::parse` 등 등록 규칙)에 맞지 않는
/// 종목이 있으면 띄우지 않는다: [`AppError::BadFile`]이고 오류에는 파일 이름([`crate::WATCHLIST_FILE`])만
/// 있다(종목 값은 넣지 않는다). 파일은 고치거나 지우지 않는다.
fn watched_symbols(settings: &Settings) -> Result<(Vec<Symbol>, SymbolsSource), AppError> {
    if !settings.symbols.is_empty() {
        return Ok((settings.symbols.clone(), SymbolsSource::Argument));
    }
    let saved = load_watchlist(&settings.home)?;
    let source = if saved.is_empty() {
        SymbolsSource::None
    } else {
        SymbolsSource::Saved
    };
    Ok((saved, source))
}

/// 설정 책상을 연다: 지켜볼 종목과 고른 AI 연결을 정하고, 고른 것이 CLI면 작업 폴더를 준비한다
/// (막히는 작업 자리에서). CLI 확인은 여기서 하지 않는다(7차 11번, [`App::start`]가 뒤에서 돌린다).
/// (8차 1번) 아직 호출이 하나도 돌지 않으므로 작업 폴더에 남은 `pi-ai` 자기 임시 파일은 지운다.
pub(super) async fn open_desk(
    settings: &Settings,
    wiring: &Wiring,
    secrets: &Arc<SecretStore>,
) -> Result<Arc<Desk>, AppError> {
    let (symbols, source) = watched_symbols(settings)?;
    // 고른 것이 없으면 API 키다. 읽을 수 없으면 고르지 않은 쪽으로 넘어가지 않고 띄우지 않는다.
    let choice = load_ai_choice(&settings.home)?.unwrap_or_else(AiChoice::api_key);
    let (home, chosen) = (settings.home.clone(), choice.clone());
    let work = tokio::task::spawn_blocking(move || prepare_work(&home, &chosen, Leftovers::Remove))
        .await
        .map_err(|_| AppError::Task)??;
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
        work,
    }));
    Ok(desk)
}

/// 접속 관문을 연다. 공개 경로는 첫 화면과 웹 자산(사무실 · 폰), 세션 경로는 설정 상태와 `protected`
/// (주문 길 · 팀 · 3차 경로), 그 컴퓨터에서만 열리는 경로는 키 · 종목 등록이다. 가리기 자리에는 비밀
/// 보관소를, 잠금 알림 자리에는 `lock_alert`(있으면)를 끼운다. (7차) 접속 키 경로도 여기서 잇는다.
pub(super) async fn open_gate(
    settings: &Settings,
    wiring: &Wiring,
    store: &Store,
    secrets: &Arc<SecretStore>,
    desk: &Arc<Desk>,
    protected: axum::Router,
    lock_alert: Option<Arc<dyn pi_gate::LockAlert>>,
) -> Result<GateServer, AppError> {
    let config = GateConfig {
        bind: SocketAddr::new(settings.bind, settings.port),
        ..GateConfig::default()
    };
    let access: AccessSlot = Arc::default();
    let mut builder = Gate::builder(store.clone(), config)
        .clock(Arc::new(GateClock(Arc::clone(&wiring.clock))))
        .redactor(Arc::new(SecretsRedactor::new(Arc::clone(secrets))))
        .public_routes(page::routes())
        .public_routes(crate::reach::asset_routes())
        .protected_routes(setup_http::status_routes(Arc::clone(desk)))
        .protected_routes(page::keys_routes())
        .protected_routes(protected)
        // 키 · 종목 등록은 접속 관문의 확인과 별개로 그 경로 자체가 그 컴퓨터인지 본다.
        .local_routes(setup_http::register_routes(Arc::clone(desk)))
        // (7차) 접속 키 발급 · 잠금 풀기는 그 컴퓨터에서만, 접속 기록 보기는 세션이면.
        .local_routes(access_http::local_routes(Arc::clone(&access)))
        .protected_routes(access_http::session_routes(
            Arc::clone(&access),
            store.clone(),
        ));
    if let Some(alert) = lock_alert {
        builder = builder.lock_alert(alert);
    }
    if let Some(peer) = wiring.peer_override.clone() {
        builder = builder.peer_override(peer);
    }
    let gate = builder.build().serve().await?;
    // 관문을 연 뒤에야 접속 키 손잡이를 얻는다. 자리는 여기서 한 번만 채운다.
    let _ = access.set(gate.access_keys());
    Ok(gate)
}

/// 세션 경로: 주문 길, 팀(있으면), 3차 경로(있으면).
pub(super) fn session_routes(
    trading: &Arc<Trading>,
    crew: Option<&Arc<Crew>>,
    extra: Option<axum::Router>,
) -> axum::Router {
    let mut routes = crate::trading::routes(Arc::clone(trading));
    if let Some(crew) = crew {
        routes = routes.merge(crate::team::routes(Arc::clone(crew)));
    }
    if let Some(extra) = extra {
        routes = routes.merge(extra);
    }
    routes
}

/// 연결을 먼저 받고(관문은 이미 떠 있다), CLI 확인은 짧게만 기다린 뒤 뒤에서 이어 간다
/// (끝나기 전 상태는 "확인 중"). 뒤에서 이어 가면 그 손잡이를 돌려준다.
pub(super) async fn check_cli_briefly(desk: &Arc<Desk>) -> Option<JoinHandle<()>> {
    let checking = Arc::clone(desk);
    let mut check = Box::pin(async move { checking.check_cli().await });
    match tokio::time::timeout(CLI_CHECK_GRACE, &mut check).await {
        Ok(()) => None,
        Err(_) => Some(tokio::spawn(check)),
    }
}

/// 시세 흐름을 뒤에서 돌린다: `--demo`면 가짜 어댑터, 아니면 토스 어댑터. 받은 시세는 주문 길로도 간다.
pub(super) fn spawn_feed(
    settings: &Settings,
    wiring: Wiring,
    secrets: Arc<SecretStore>,
    gate: &GateServer,
    desk: &Arc<Desk>,
    feeding: (&Arc<Trading>, Option<StaleHook>),
    mut stopping: watch::Receiver<bool>,
) -> JoinHandle<()> {
    let (trading, stale) = feeding;
    let source = if settings.demo {
        let demo = DemoAdapter::new(settings.market, Arc::clone(&wiring.clock));
        Source::Demo(Arc::new(demo))
    } else {
        let keys: Arc<dyn CredentialSource> = Arc::new(BrokerKeys::new(secrets));
        let client = TossClient::new(wiring.upstreams.toss, keys, Arc::clone(&wiring.clock));
        let adapter: Arc<dyn MarketAdapter> = Arc::new(client.adapter(settings.market));
        Source::Broker(adapter)
    };
    let pace = Pace {
        interval: settings.quote_interval,
        retry_initial: settings.retry_initial,
        retry_max: settings.retry_max,
        calendar_retry: settings.calendar_retry,
    };
    let feed = Feed::new(
        gate.publisher(),
        wiring.clock,
        settings.market,
        Arc::clone(desk),
        pace,
    )
    .feeding(Arc::clone(trading))
    .watching(stale);
    tokio::spawn(async move {
        tokio::select! {
            // 알리는 쪽이 사라져도 멈춘다.
            _ = stopping.wait_for(|stop| *stop) => {}
            () = feed.run(source) => {}
        }
    })
}
