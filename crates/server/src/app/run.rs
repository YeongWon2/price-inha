//! (24차) 실행 파일 `run`의 조립. 실행 파일(`main.rs`)과 시험이 같은 조립을 쓴다: 시험은 이 값에서 시계 ·
//! 바깥 주소 · 가짜 공시 간격만 바꿔 띄운다.

use std::sync::Arc;

use pi_secrets::SecretStore;

use super::{App, Launch, Wiring};
use crate::demo::DemoWiring;
use crate::{AppError, Reach, Settings, TeamWiring, TradingWiring};

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
}

/// 실행 파일 `run`의 조립: 실제 주소 · 실제 시계([`Wiring::default`]), 기본 주문 길, 공식 투자 팀
/// ([`TeamWiring::official`]), 빈 [`Reach`]. `settings.demo`면 시연 달력 · 시연용 가짜 공시를 켠다
/// ([`DemoWiring::binary`]). 시연이 아니면 시연 달력이 섞이지 않는다(E56). `secrets`는 부르는 쪽이 연 보관소.
pub fn run_assembly(_settings: &Settings, secrets: Option<Arc<SecretStore>>) -> RunAssembly {
    // 뼈대: 지금 실행 파일의 조립 그대로(시연 값은 아직 꺼짐).
    RunAssembly {
        wiring: Wiring::default(),
        launch: Launch {
            trading: TradingWiring::default(),
            team: Some(TeamWiring::official()),
            secrets,
        },
        reach: Reach::default(),
        demo: DemoWiring::default(),
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
        // 뼈대: 시연 값은 아직 잇지 않는다(지금 동작 그대로).
        App::launch_with(settings, assembly.wiring, assembly.launch, assembly.reach).await
    }
}
