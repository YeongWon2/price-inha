//! 서버 조립: 저장소 · 비밀 보관소 · 시세 · 접속 관문을 잇는다.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pi_gate::{GateServer, Publisher};
use pi_market::{Clock, SystemClock};
use pi_secrets::SecretStore;
use pi_store::Store;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::desk::Desk;
use crate::team::{CrewParts, spawn_trigger};
use crate::trading::{TradingParts, default_wiring};
use crate::{AppError, CLI_COMMON_LOCATIONS, Crew, Settings, Trading, Upstreams};

/// (7차 11번) 띄울 때 CLI 확인을 기다려 주는 가장 긴 시간. 접속 관문은 이미 연결을 받고 있다.
/// 보통의 확인(수십 밀리초)은 이 안에 끝나 띄우자마자 결과가 보이고, 넘으면 기다리지 않고
/// 뒤에서 이어 간다(그동안 상태는 [`crate::CLI_CHECKING`]).
///
/// (8차 5번) 까닭 — 두 요구의 절충이다:
/// - 잠긴 6차 재시작 테스트는 다시 띄운 **바로 뒤** 설정 상태에서 CLI 확인 결과(등록됨)를 본다.
///   확인을 전혀 기다리지 않으면 그 순간 상태가 늘 "확인 중"이라 그 테스트가 깨진다.
/// - 7차 11번은 CLI 확인이 서버 시작을 막지 않기를 요구한다(느리거나 멈춘 CLI라도 연결은 받는다).
///   끝까지 기다리면 확인의 시간 제한(최대 수십 초)만큼 시작이 늦어진다.
///
/// 그래서 관문을 먼저 연 뒤 보통의 확인이 넉넉히 끝나는 짧은 시간만 기다리고, 넘으면 뒤에서 이어 간다.
/// 800ms는 가짜 셸 스크립트 CLI의 확인(버전 → 로그인, 두 번 띄움)이 느린 기계에서도 들어오고,
/// 사람이 시작이 늦다고 느끼지 않는 값이다.
const CLI_CHECK_GRACE: std::time::Duration = std::time::Duration::from_millis(800);

mod link;
mod parts;
mod run;

pub use parts::announce_addr;
use parts::{check_cli_briefly, key_verifier, open_desk, open_gate, session_routes, spawn_feed};
use run::DemoRun;
pub use run::{RunAssembly, run_assembly};

use crate::demo::DemoWiring;

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

/// (W5 2차) 띄울 때 고르는 것들. [`App::launch`]에 준다.
#[derive(Clone)]
pub struct Launch {
    /// 주문 길의 바깥 값.
    pub trading: crate::TradingWiring,
    /// 투자 팀(에이전트 · 자료 · 트리거 · 복기)의 바깥 값. `None`이면 팀을 잇지 않는다(1차까지와 같다:
    /// 아무도 깨우지 않고 하루 운용 판단은 [`crate::Trading::decide_day`]를 부르는 쪽이 한다).
    pub team: Option<crate::TeamWiring>,
    /// 부르는 쪽이 연 비밀 보관소(`settings.home`으로 연 것). `None`이면 여기서 연다.
    pub secrets: Option<Arc<SecretStore>>,
}

/// 떠 있는 서버.
///
/// 띄울 때 하는 일:
///
/// 1. 보관 폴더에서 비밀 보관소(`SecretStore::open(home)`)와 저장소(`home/`[`DB_FILE`])를 연다.
/// 2. 지켜볼 종목을 정한다: 인자 · 환경 변수로 줬으면 그것, 아니면 보관 폴더에 저장된 것.
/// 3. 접속 관문을 `bind:port`에 연다. 공개 경로 `GET /`는 빌드한 화면 `web/dist/index.html`을 내준다
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
    /// 시세 흐름 · 하루 시작/마감을 멈추는 알림.
    stop: watch::Sender<bool>,
    feed: JoinHandle<()>,
    /// (7차 11번) 뒤에서 도는 CLI 확인(띄울 때 끝났으면 없다). 끌 때 그만둔다.
    ai_check: Option<JoinHandle<()>>,
    /// 끌 때 닫는다(놓는다).
    store: Store,
    /// (W5 1차) 주문 길.
    trading: Arc<Trading>,
    /// (W5 1차) 뒤에서 도는 하루 시작 · 마감.
    days: JoinHandle<()>,
    /// (W5 2차) 투자 팀과 깨우기 엔진(팀을 잇지 않았으면 없다).
    crew: Option<Arc<Crew>>,
    trigger: Option<JoinHandle<()>>,
    /// (W5 3차) 지식 검색 창구.
    knowledge: Arc<pi_knowledge::Knowledge>,
    /// (W5 3차) 알림 · 사무실 · 백업([`App::launch_with`]로 띄웠을 때만).
    linked: Option<Arc<crate::reach::Linked>>,
    /// (W5 3차) 사무실이 팀 상태를 따라가는 일.
    office_sync: Option<JoinHandle<()>>,
    /// (10c 13번) 뜻 검색 임베딩 다시 만들기(꺼져 있으면 아무것도 묻지 않는다).
    reembed: JoinHandle<()>,
}

/// 팀이 있으면 깨우기 엔진을 띄운다.
/// (24차) `demo`는 시연 값(꺼짐이면 지금 동작 그대로).
async fn crew_trigger(
    crew: Option<&Arc<Crew>>,
    clock: &Arc<dyn Clock>,
    stop: &watch::Sender<bool>,
    demo: &DemoWiring,
) -> Result<Option<JoinHandle<()>>, AppError> {
    match crew {
        Some(crew) => Ok(Some(
            spawn_trigger(crew, Arc::clone(clock), stop.subscribe(), demo).await?,
        )),
        None => Ok(None),
    }
}

/// 투자 팀을 잇는다(`team`이 없으면 잇지 않는다).
/// (17차 4번) 설정의 하루 AI 호출 상한을 팀 조립에 잇는다(팀 조립에 이미 적힌 값이 앞선다). (22차 6번) 그 켤 때 값을
/// 설정 책상에도 적는다(설정 상태에 보이고, 화면에서 저장한 값보다 앞선다).
fn team_with_limit(
    team: Option<crate::TeamWiring>,
    settings: &Settings,
    desk: &Desk,
) -> Option<crate::TeamWiring> {
    team.map(|mut team| {
        team.ai_daily_limit = team.ai_daily_limit.or(settings.ai_daily_limit);
        desk.set_launch_ai_limit(team.ai_daily_limit);
        team
    })
}

/// (24차) `stored.1`(offline)이 참이면 팀이 바깥 자료(DART · ECOS · FRED · 네이버)를 부르지 않는다.
/// (25차 3번) `stored.2`는 시연용 가짜 AI가 답마다 두는 뜸이다.
fn open_crew(
    team: Option<crate::TeamWiring>,
    stored: (&Store, bool, Duration),
    wiring: &Wiring,
    trading: &Arc<Trading>,
    desk: &Arc<Desk>,
    secrets: &Arc<SecretStore>,
) -> Result<Option<Arc<Crew>>, AppError> {
    let Some(team) = team else {
        return Ok(None);
    };
    let (store, offline, demo_ai_pause) = stored;
    let crew = Crew::open(CrewParts {
        store: store.clone(),
        clock: Arc::clone(&wiring.clock),
        trading: Arc::clone(trading),
        desk: Arc::clone(desk),
        secrets: Arc::clone(secrets),
        wiring: team,
        offline,
        demo_ai_pause,
    })?;
    // (28차 3번) 승인 카드 길로 온 개선안 답이 개선안 답하기와 같은 함수를 타게 잇는다.
    trading.hook_proposals(link::proposal_hook(Arc::downgrade(&crew)));
    Ok(Some(crew))
}

/// 보관 폴더의 비밀 보관소(부르는 쪽이 연 것이 있으면 그것)와 저장소를 연다. 밖의 주소로 열면 경고를 남긴다.
fn open_home(
    settings: &Settings,
    secrets: Option<Arc<SecretStore>>,
) -> Result<(Arc<SecretStore>, Store), AppError> {
    let secrets = match secrets {
        Some(secrets) => secrets,
        // 보관 폴더는 비밀 보관소가 만든다(없으면 0700으로).
        None => Arc::new(SecretStore::open(settings.home.clone())?),
    };
    let store = Store::open(settings.home.join(DB_FILE))?;
    if !settings.bind.is_loopback() {
        tracing::warn!(
            bind = %settings.bind,
            "그 컴퓨터 밖의 주소로 엽니다. 접속 키가 생기기 전까지 밖에서 온 요청은 모두 거부합니다"
        );
    }
    Ok((secrets, store))
}

impl App {
    /// 서버를 띄운다.
    pub async fn start(settings: Settings, wiring: Wiring) -> Result<App, AppError> {
        App::start_with(settings, wiring, default_wiring()).await
    }

    /// (W5 1차) 주문 길까지 이어 서버를 띄운다: [`App::start`]가 하는 일에 더해
    ///
    /// - 저장소로 주문 실행기(`pi_exec::Executor::open`, 계좌 스냅샷이 있으면 되살림)를 열고 맞춰 본다
    ///   (`reconcile`). 실행기가 준 점검 권한은 규칙 엔진의 길에만 건넨다.
    /// - 승인 관리자(`pi_approval::Approvals`)를 같은 저장소 · 시계로 연다.
    /// - [`crate::TEAM_PATH`] · [`crate::TEAM_RESTART_PATH`] · [`crate::APPROVAL_ANSWER_PATH`] ·
    ///   [`crate::TEMPLATE_PATH`] · [`crate::TEMPLATE_ADJUST_PATH`]를 세션이 있어야 열리는 경로에 단다.
    /// - 시세 흐름이 받은 시세를 [`crate::Trading::on_quote`]로 넘기고, 하루 시작 · 마감
    ///   ([`crate::KIND_DAY_START`] · [`crate::KIND_DAY_CLOSE`])을 시계로 뒤에서 돌린다.
    ///
    /// [`App::start`]는 이것을 기본 주문 길 값으로 부른 것과 같다.
    pub async fn start_with(
        settings: Settings,
        wiring: Wiring,
        trading: crate::TradingWiring,
    ) -> Result<App, AppError> {
        // 보관 폴더는 비밀 보관소가 만든다(없으면 0700으로).
        let secrets = Arc::new(SecretStore::open(settings.home.clone())?);
        App::start_with_secrets(settings, wiring, trading, secrets).await
    }

    /// (W5 1차 보강) [`App::start_with`]와 같되, 비밀 보관소를 부르는 쪽이 연 것(`Arc`)으로 쓴다.
    ///
    /// 실행 파일은 기록(tracing)의 가리기([`crate::RedactingWriter`])와 이 앱에 **같은** 보관소를 준다.
    /// 그래서 띄운 뒤 설정 화면에서 등록한 키(확인에 실패한 값 포함)도 표준 오류 · 기록에서 가려진다
    /// (E38 · N8). `secrets`는 `settings.home`으로 연 것이어야 한다.
    pub async fn start_with_secrets(
        settings: Settings,
        wiring: Wiring,
        trading: crate::TradingWiring,
        secrets: Arc<SecretStore>,
    ) -> Result<App, AppError> {
        let launch = Launch {
            trading,
            team: None,
            secrets: Some(secrets),
        };
        App::launch(settings, wiring, launch).await
    }

    /// (W5 2차) [`App::start_with_secrets`]가 하는 일에 더해, `launch.team`이 있으면 투자 팀을 잇는다:
    ///
    /// - 에이전트 실행기(담당 6명)를 고른 AI 연결(시연 모드면 [`crate::DemoAi`])로 만든다. 공급자는 가리기
    ///   공급자로 감싼다(받은 답 · 오류 글도 가린다). 도구는 자료 받기와 주문 안 넘기기(주문 길)다.
    /// - 깨우기 엔진을 저장소(작업 상태 · 영수증)와 시장 달력으로 띄운다: 장 시작에 하루 운용 판단
    ///   ([`crate::Crew::plan_day`])과 새 공시 보기([`crate::Crew::scan`]), 마감 뒤에 하루 복기
    ///   ([`crate::Crew::review_day`]).
    /// - [`crate::TEAM_MEMBERS_PATH`] · [`crate::TRADE_BASIS_PATH`] · [`crate::PROPOSALS_PATH`] ·
    ///   [`crate::PROPOSAL_ANSWER_PATH`]를 세션이 있어야 열리는 경로에 단다.
    pub async fn launch(
        settings: Settings,
        wiring: Wiring,
        launch: Launch,
    ) -> Result<App, AppError> {
        // 띄우기의 미래 값이 커서 힙에 둔다(부르는 쪽의 미래 값을 작게).
        Box::pin(App::assemble(
            settings,
            wiring,
            launch,
            None,
            DemoRun::default(),
        ))
        .await
    }

    /// 띄우기의 몸통. `reach`가 있으면 3차 조립까지 한다([`App::launch_with`]). (24차) `demo`는 실행 파일 조립의
    /// 시연 값이다([`App::launch_run`]만 켠다): 시연 달력 · 시연용 가짜 공시 · 바깥 자료 부르지 않기.
    pub(super) async fn assemble(
        settings: Settings,
        wiring: Wiring,
        mut launch: Launch,
        reach: Option<crate::Reach>,
        demo: DemoRun,
    ) -> Result<App, AppError> {
        // 되살리기 중 꺼져 남은 작업 폴더(보관 폴더 옆)를 지운다.
        crate::reach::clear_restore_leftover(&settings.home);
        demo.fix_calendar(settings.market, &mut launch);
        let (secrets, store) = open_home(&settings, launch.secrets)?;
        let verifier = key_verifier(&wiring, launch.team.as_ref(), reach.as_ref());
        let desk = open_desk(&settings, &wiring, &secrets, verifier).await?;
        let parts = TradingParts {
            store: store.clone(),
            clock: Arc::clone(&wiring.clock),
            market: settings.market,
            desk: Arc::clone(&desk),
            wiring: launch.trading,
        };
        let trading = Trading::open(parts).await?;
        let team = team_with_limit(launch.team, &settings, &desk);
        let crew = open_crew(
            team,
            (&store, demo.offline, demo.ai_pause),
            &wiring,
            &trading,
            &desk,
            &secrets,
        )?;
        let linked = link::open_linked(reach, &store, &secrets, &wiring.clock)?;
        let memory = link::open_memory(&settings.home, &store, &secrets, crew.as_ref());
        let (extra, alert) = link::session_parts(linked.as_ref(), &trading, crew.as_ref(), &store);
        let extra = link::with_semantic(extra, &memory.semantic);
        let extra = link::with_looks(extra, &settings.home, &store, &wiring.clock);
        let protected = session_routes(&trading, crew.as_ref(), extra);
        let gate = open_gate(
            &settings, &wiring, &store, &secrets, &desk, protected, alert,
        )
        .await?;
        trading.attach(gate.publisher());
        if let Some(linked) = &linked {
            link::attach(linked, &gate, &trading, crew.as_ref(), &memory);
        }
        let ai_check = check_cli_briefly(&desk).await;
        // 띄운 그때 이미 그 날 세션 안이면 하루 시작을 바로 남긴다.
        trading.tick().await;
        let (stop, stopping) = watch::channel(false);
        let days = trading.spawn_days(stop.subscribe());
        let trigger = crew_trigger(crew.as_ref(), &wiring.clock, &stop, &demo.wiring).await?;
        let office_sync = linked
            .as_ref()
            .map(|l| link::spawn_office_sync(l, store.clone(), stop.subscribe()));
        let reembed = link::spawn_semantic(&memory.semantic, crew.as_ref(), &wiring.clock, &stop);
        let stale = linked.as_ref().map(link::stale_hook);
        let feeding = (&trading, stale, demo.wiring.calendar);
        let feed = spawn_feed(&settings, wiring, secrets, &gate, &desk, feeding, stopping);
        Ok(App {
            gate,
            desk,
            stop,
            feed,
            ai_check,
            store,
            trading,
            days,
            crew,
            trigger,
            knowledge: memory.knowledge,
            linked,
            office_sync,
            reembed,
        })
    }

    /// (W5 2차) 조립된 투자 팀. 팀을 잇지 않고 띄웠으면 `None`.
    pub fn crew(&self) -> Option<Arc<crate::Crew>> {
        self.crew.clone()
    }

    /// (W5 1차) 이 앱의 저장소(같은 연결을 나눠 쓴다).
    pub fn store(&self) -> Store {
        self.store.clone()
    }

    /// (W5 1차) 주문 길.
    pub fn trading(&self) -> Arc<crate::Trading> {
        Arc::clone(&self.trading)
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
    ///
    /// (7차 14번) 화면에서 연결을 다시 고르면 쥔 공급자가 바뀐다. 그래서 돌려받은 것을 오래 쥐지 말고
    /// **쓸 때마다 다시 받는다**(한 번 받은 것은 다시 고르기 전의 연결이다). 작업 폴더가 비어 있지 않아
    /// CLI를 쓰지 않을 때는 부르면 띄우지 않고 실패하는 공급자다.
    pub fn ai_provider(&self) -> Arc<dyn pi_ai::AiProvider> {
        self.desk.ai_provider()
    }

    /// 이벤트를 내보내는 손잡이(다른 모듈이 쓴다).
    pub fn publisher(&self) -> Publisher {
        self.gate.publisher()
    }

    /// 끈다(Ctrl+C): 시세 받기를 멈추고, CLI 공급자를 끝내고(남은 자식을 끝내 거둘 때까지 기다린다,
    /// 7차 5번), 연결을 정리하고, 저장소를 닫는다.
    /// 같은 보관 폴더로 다시 띄우면 이벤트 번호가 이어진다(V6).
    ///
    /// (8차 3번) 맨 먼저 꺼지기 시작 표시를 세운다(고르기 잠금을 기다리기 전에). 그 뒤에 고르기 잠금을
    /// 얻은 AI 고르기는 거부되고, 공급자 끝내기는 고르기와 같은 잠금을 잡고 한다.
    ///
    /// (gate 8차 검토의 참고) 끄기가 늦어질 수 있는 상한: 도는 AI 고르기의 CLI 확인(최대 30초)만큼
    /// 끄기가 늦어질 수 있다. 끄기는 고르기 잠금을 기다리므로, 그 잠금을 쥔 고르기가 CLI 확인을 끝낼 때까지다.
    pub async fn shutdown(self) -> Result<(), AppError> {
        self.desk.begin_closing();
        // (25차 3번) 시연용 가짜 AI의 뜸을 바로 끝낸다(끄기가 뜸을 기다리지 않게).
        if let Some(demo) = self.crew.as_ref().and_then(|crew| crew.demo_ai()) {
            demo.close();
        }
        self.stop.send_replace(true);
        let feed = self.feed.await.map_err(|_| AppError::Task);
        let days = self.days.await.map_err(|_| AppError::Task);
        if let Some(trigger) = self.trigger {
            let _ = trigger.await;
        }
        if let Some(sync) = self.office_sync {
            let _ = sync.await;
        }
        let _ = self.reembed.await;
        drop(self.linked);
        drop(self.crew);
        // 뒤에서 돌던 CLI 확인을 그만두고(확인 프로세스는 `pi-ai`가 시간 제한으로 거둔다),
        // 쥔 CLI 공급자의 자식을 끝낸다.
        if let Some(check) = self.ai_check {
            check.abort();
            let _ = check.await;
        }
        self.desk.shutdown_ai().await;
        // 시세 흐름이 비정상으로 끝났어도 연결과 저장소는 정리한다.
        let gate = self.gate.shutdown().await;
        drop(self.trading);
        drop(self.store);
        gate?;
        days?;
        feed
    }
}
