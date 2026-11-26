//! (24차) 실행 파일 `run`의 조립. 실행 파일(`main.rs`)과 시험이 같은 조립을 쓴다: 시험은 이 값에서 시계 ·
//! 바깥 주소 · 가짜 공시 간격만 바꿔 띄운다.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use pi_market::{Market, Symbol};
use pi_secrets::SecretStore;

use super::{App, Launch, Wiring};
use crate::demo::{DEMO_AI_PAUSE, DemoWiring, demo_calendar};
use crate::setup::load_watchlist;
use crate::trading::{demo_sectors, load_rules_config};
use crate::{AppError, CalendarFn, MessengerWiring, Reach, Settings, TeamWiring, TradingWiring};

/// 실행 파일 `run`이 띄우는 조립 값 전부. [`run_assembly`]로 만들고 [`App::launch_run`]에 준다.
#[derive(Clone)]
pub struct RunAssembly {
    /// 바깥 자리(실제 서버 주소 · 실제 시계).
    pub wiring: Wiring,
    /// 주문 길 · 투자 팀(공식 주소) · 비밀 보관소.
    pub launch: Launch,
    /// 검색 · 백업 · 사무실 · 폰(백업은 설정 화면에서 고른 것을 앱이 켤 때 잇는다 — 여기 칸은 비운다).
    pub reach: Reach,
    /// (24차) 시연 모드의 조립 값. `settings.demo`면 [`DemoWiring::binary`], 아니면 꺼짐(`Default`).
    pub demo: DemoWiring,
    /// (25차 3번) 시연용 가짜 AI가 답마다 두는 뜸. `settings.demo`면 [`crate::DEMO_AI_PAUSE`], 아니면
    /// `Duration::ZERO`. 시연 모드로 띄우면 팀의 가짜 AI가 [`crate::DemoAi::with_pause`]로 이 값을 쓴다.
    /// 시험은 이 칸만 줄인다([`DemoWiring`]은 그대로).
    pub demo_ai_pause: Duration,
    /// (39차) 메신저 조립 값. 실행 파일은 [`MessengerWiring::default`](공식 텔레그램 주소). 시험은 가짜 서버 주소와
    /// 짧은 시간을 넣는다. 시연 모드면 메신저 루프를 돌리지 않는다(`crate::messenger` 문서).
    pub messenger: MessengerWiring,
}

/// 실행 파일 `run`의 조립: 실제 주소 · 실제 시계([`Wiring::default`]), 기본 주문 길, 공식 투자 팀
/// ([`TeamWiring::official`]), 빈 [`Reach`]. `settings.demo`면 시연 달력 · 시연용 가짜 공시를 켠다
/// ([`DemoWiring::binary`]). 시연이 아니면 시연 달력이 섞이지 않는다(E56). `secrets`는 부르는 쪽이 연 보관소.
///
/// (35차 4번) `settings.demo`면 지켜보는 종목 전부에 시연용 업종("시연 업종 A" 식)을 조립 값으로 넣는다
/// (`crate::RULES_CONFIG_FILE`에 쓰지 않는다). 시연 어댑터는 종목 상태(VI · 거래정지)를 "아님"으로 알린다. 그래서
/// 시연에서 팀이 사기로 정하면 주문 전 점검(P8 · P12)을 지나 주문(또는 승인 기다림)까지 간다 — 모의계좌뿐이다.
pub fn run_assembly(settings: &Settings, secrets: Option<Arc<SecretStore>>) -> RunAssembly {
    let demo = if settings.demo {
        DemoWiring::binary()
    } else {
        DemoWiring::default()
    };
    let mut trading = TradingWiring::default();
    if settings.demo {
        // 시연 종목에 시연 업종을 조립 값으로(파일에 쓰지 않는다, E56). 읽지 못하는 종목 파일은 띄울 때 알린다.
        let watched = if settings.symbols.is_empty() {
            load_watchlist(&settings.home).unwrap_or_default()
        } else {
            settings.symbols.clone()
        };
        // (다듬기 E 1번) 종목이 없으면 시연 기본 종목에 시연 업종을 넣는다(파일에 쓰지 않는다).
        let watched = if watched.is_empty() {
            crate::demo::demo_symbols(settings.market)
        } else {
            watched
        };
        trading.rules.sectors = demo_sectors(&watched);
    }
    RunAssembly {
        wiring: Wiring::default(),
        launch: Launch {
            trading,
            team: Some(TeamWiring::official()),
            secrets,
        },
        reach: Reach::default(),
        demo,
        demo_ai_pause: if settings.demo {
            DEMO_AI_PAUSE
        } else {
            Duration::ZERO
        },
        messenger: MessengerWiring::default(),
    }
}

/// (24차) 띄우기 몸통([`App::assemble`])에 건네는 시연 값. 실행 파일 조립([`App::launch_run`])이 시연 설정일 때만
/// 켜고, 그 밖의 띄우기([`App::launch`] · [`App::launch_with`])는 늘 꺼짐(`Default`)이다 — 기존 동작 그대로.
#[derive(Debug, Clone, Default)]
pub(crate) struct DemoRun {
    /// 시연 달력 · 시연용 가짜 공시.
    pub(crate) wiring: DemoWiring,
    /// 바깥 자료(DART · ECOS · FRED · 네이버)를 부르지 않는다(키가 등록돼 있어도).
    pub(crate) offline: bool,
    /// (25차 3번) 시연용 가짜 AI가 답마다 두는 뜸([`RunAssembly::demo_ai_pause`]). 그 밖의 띄우기는 0.
    pub(crate) ai_pause: Duration,
    /// (35차 4번) 조립이 준 업종표를 시연 업종으로 따로 쥐고, 업종이 없는 지켜보는 종목에 더한다.
    pub(crate) sectors: bool,
    /// (다듬기 E 1번) 지켜보는 종목이 없으면 시연 기본 종목([`crate::demo_symbols`])을 조립 값으로 쓴다.
    pub(crate) default_symbols: bool,
}

impl DemoRun {
    /// 설정 책상을 연다([`super::open_desk`]). (다듬기 E 1번) 시연 실행이고 지켜보는 종목이 없으면 시연 기본 종목
    /// ([`crate::demo_symbols`])을 조립 값으로 쓴다(파일에 쓰지 않는다, E56).
    pub(crate) async fn open_desk(
        &self,
        settings: &Settings,
        wiring: &Wiring,
        secrets: &Arc<SecretStore>,
        verifier: crate::KeyVerifier,
    ) -> Result<Arc<crate::desk::Desk>, AppError> {
        let desk = super::open_desk(settings, wiring, secrets, verifier).await?;
        if self.default_symbols {
            desk.use_demo_symbols(crate::demo::demo_symbols(settings.market));
        }
        Ok(desk)
    }

    /// 시연 달력이 켜져 있으면 그 시장의 시연 달력을 주문 길의 달력으로 쓴다(조립이 이미 정한 달력이 앞선다).
    /// 그래서 하루 시작 · 마감과 깨우기 엔진이 시세 흐름이 받아 오는 달력을 기다리지 않고 처음부터 그것을 쓴다.
    fn fix_calendar(&self, market: Market, launch: &mut Launch) {
        if self.wiring.calendar && launch.trading.calendar.is_none() {
            let calendar: CalendarFn = Arc::new(move |date| demo_calendar(market, date));
            launch.trading.calendar = Some(calendar);
        }
    }

    /// 띄우기 전 주문 길 값을 맞춘다: 시연 달력([`DemoRun::fix_calendar`]), 그리고 (35차 1 · 4번) 보관 폴더의
    /// [`crate::RULES_CONFIG_FILE`]이 있으면 그 값이 조립의 금지 종목 · 업종표를 대신한다(깨졌으면
    /// [`AppError::BadFile`]). 시연 실행이면 조립이 준 업종표를 시연 업종으로 떼어 돌려준다(주문 길이 따로 쥔다).
    pub(crate) fn prepare(
        &self,
        settings: &Settings,
        launch: &mut Launch,
    ) -> Result<Option<HashMap<Symbol, String>>, AppError> {
        self.fix_calendar(settings.market, launch);
        let demo = self
            .sectors
            .then(|| std::mem::take(&mut launch.trading.rules.sectors));
        if let Some(saved) = load_rules_config(&settings.home)? {
            launch.trading.rules = saved;
        }
        Ok(demo)
    }
}

impl App {
    /// (24차) 실행 파일 조립([`run_assembly`])으로 띄운다. [`App::launch_with`]가 하는 일에 더해
    /// `assembly.demo`를 시연 어댑터 · 깨우기 엔진 · 주문 길에 잇는다(시연 모드일 때만):
    ///
    /// - 시연 달력이 켜져 있으면 시연 어댑터가 [`crate::demo_calendar`]를 내고, 띄운 뒤 첫 tick부터 그 달력을 쓴다.
    /// - 가짜 공시 간격이 있으면 그 간격마다 지켜보는 종목을 돌아가며 "시연" 공시 한 건을 내 종목 논의를 연다.
    /// - 시연 모드는 모의계좌 · 시연용 가짜 AI만 쓰고 실제 증권사 · AI · DART · 네이버로 요청을 보내지 않는다.
    pub async fn launch_run(settings: Settings, assembly: RunAssembly) -> Result<App, AppError> {
        // 시연이 아니면 시연 값을 섞지 않는다(E56).
        let demo = if settings.demo {
            DemoRun {
                wiring: assembly.demo,
                offline: true,
                ai_pause: assembly.demo_ai_pause,
                sectors: true,
                default_symbols: true,
            }
        } else {
            DemoRun::default()
        };
        let reach = Some((assembly.reach, assembly.messenger));
        Box::pin(App::assemble(
            settings,
            assembly.wiring,
            assembly.launch,
            reach,
            demo,
        ))
        .await
    }
}
