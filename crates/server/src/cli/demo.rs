//! (67차) `price-inha demo [--no-open] [설정 인자들]`: 키 · AI · 증권사 없이 시연 서버를 켜고 사무실을 연다.
//!
//! 사용자 요청 "demo도 릴리즈해"(30초 체험). `run --demo`와 같은 조립([`crate::run_assembly`])이고 다른 것은 셋이다.
//!
//! - 보관 폴더가 실제 것과 따로다: `--home`을 주지 않으면 실제판이 쓸 폴더 이름 뒤에 [`DEMO_HOME_SUFFIX`]를
//!   붙인다(예: `~/.price-inha-demo`). 시연 기록이 실제 기록과 섞이지 않는다.
//! - 포트를 직접 주지 않았고 기본 포트가 차 있으면(실제 서버가 켜져 있어도 체험이 되게) 바로 뒤의 빈 포트를,
//!   그것도 없으면 운영체제가 고른 빈 포트를 쓴다([`demo_port`]).
//! - 뜨면 [`DEMO_NOTICE`]와 사무실 주소(`http://127.0.0.1:<포트>/office/`)를 찍고, `--no-open`이 아니면
//!   `price-inha open`과 같은 길(운영체제 여는 명령, 윈도 `cmd /c start`)로 브라우저를 연다.
//!
//! 바깥(증권사 · AI · 자료 · 메신저)으로 나가지 않는 것은 `run --demo`와 같다(E56). 주문은 모의계좌뿐이다.

use std::io::Write;
use std::net::{IpAddr, TcpListener};
use std::path::{Path, PathBuf};

use pi_store::{NewTemplate, TemplateState, UnixMillis};
use pi_template::{Style, Template};

use super::open::{OpenScreen, open_browser};
use super::runner::CommandRunner;
use crate::{App, AppError, RunAssembly, Settings, SettingsError};

/// 시연 보관 폴더 이름 뒤에 붙이는 글자.
pub const DEMO_HOME_SUFFIX: &str = "-demo";

/// 시연 서버가 뜰 때 찍는 안내.
pub const DEMO_NOTICE: &str =
    "시연 모드입니다 — 가짜 시세 · 가짜 AI, 바깥으로 나가지 않습니다. 끄려면 Ctrl+C";

/// 템플릿을 고르지 않은 시연 보관 폴더에 스윙 템플릿을 골라 두었을 때 찍는 안내.
pub const DEMO_TEMPLATE_NOTICE: &str =
    "시연용으로 스윙 템플릿(모의중)을 골라 두었습니다. 템플릿 화면에서 바꿀 수 있습니다";

/// `demo`의 선택.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DemoRequest {
    /// 브라우저를 열지 않고 주소만 찍는다.
    pub no_open: bool,
}

/// (결정 36) 짧은 하루를 켰을 때 찍는 안내의 앞부분.
pub const DEMO_DAY_NOTICE: &str = "체험판의 하루는";

/// (결정 36 · E56) 체험 폴더에서 키를 받아 둘 때 답에 싣는 글.
pub const DEMO_KEYS_NOTICE: &str = "체험판 — 실제로 확인하지 않고 받아 두었습니다";

/// (결정 36) `demo`의 계획: 선택과 짧은 하루 길이(분, 0이면 끔).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemoPlan {
    /// 선택.
    pub request: DemoRequest,
    /// 하루 길이(분). 0이면 짧은 하루를 끈다(시연 달력 그대로).
    pub day_minutes: u32,
}

/// (결정 36) `--demo-day-minutes <수>`를 빼서 읽고 나머지로 [`demo_settings`]를 부른다.
pub fn demo_plan(
    _args: &[String],
    _env: &dyn Fn(&str) -> Option<String>,
    _user_home: Option<&Path>,
    _no_open: bool,
) -> Result<(Settings, DemoPlan), SettingsError> {
    todo!()
}

/// (결정 36) 계획대로 띄운다(짧은 하루 · 체험 폴더 표시).
pub async fn start_demo_plan(
    _settings: Settings,
    _assembly: RunAssembly,
    _plan: &DemoPlan,
    _output: &mut dyn Write,
    _runner: &dyn CommandRunner,
) -> Result<App, AppError> {
    todo!()
}

/// 기본 포트가 차 있을 때 바로 뒤에서 찾아보는 포트 수.
const NEARBY_PORTS: u16 = 20;

/// 시연 설정을 읽는다(모듈 문서의 보관 폴더 · 포트 규칙). 늘 시연 모드다.
pub fn demo_settings(
    args: &[String],
    env: &dyn Fn(&str) -> Option<String>,
    user_home: Option<&Path>,
) -> Result<Settings, SettingsError> {
    let mut given = args.to_vec();
    if !given.iter().any(|arg| arg == ARG_DEMO) {
        given.push(ARG_DEMO.to_string());
    }
    let mut settings = Settings::parse(&given, env, user_home)?;
    if !args.iter().any(|arg| arg == ARG_HOME) {
        let mut home = settings.home.into_os_string();
        home.push(DEMO_HOME_SUFFIX);
        settings.home = PathBuf::from(home);
    }
    let explicit = args.iter().any(|arg| arg == ARG_PORT) || env(crate::ENV_PORT).is_some();
    settings.port = demo_port(settings.bind, settings.port, explicit);
    Ok(settings)
}

/// (67차) 받은 인자와 프로세스의 환경 변수 · 홈 폴더로 [`demo_settings`]를 부른다(실행 파일의 `demo`).
pub fn demo_from_process(args: &[String]) -> Result<Settings, SettingsError> {
    let env = |name: &str| std::env::var(name).ok();
    demo_settings(args, &env, std::env::home_dir().as_deref())
}

/// 설정 인자 이름들(`crate::Settings`와 같은 글자).
const ARG_DEMO: &str = "--demo";
const ARG_HOME: &str = "--home";
const ARG_PORT: &str = "--port";

/// 시연이 열 포트: 직접 줬으면(`explicit`) 그대로, 비어 있으면 그대로, 차 있으면 바로 뒤 [`NEARBY_PORTS`]개 중
/// 첫 빈 포트, 그것도 없으면 운영체제가 고른 빈 포트. 아무것도 못 고르면 `wanted`(띄울 때 까닭을 알린다).
pub fn demo_port(bind: IpAddr, wanted: u16, explicit: bool) -> u16 {
    if explicit || wanted == 0 || is_free(bind, wanted) {
        return wanted;
    }
    let nearby = (1..=NEARBY_PORTS).filter_map(|step| wanted.checked_add(step));
    if let Some(port) = nearby.into_iter().find(|port| is_free(bind, *port)) {
        return port;
    }
    TcpListener::bind((bind, 0))
        .and_then(|listener| listener.local_addr())
        .map_or(wanted, |addr| addr.port())
}

/// 그 주소 · 포트를 지금 열 수 있는가(열어 보고 바로 닫는다).
fn is_free(bind: IpAddr, port: u16) -> bool {
    TcpListener::bind((bind, port)).is_ok()
}

/// 시연 서버를 띄우고 안내 · 사무실 주소를 찍고 브라우저를 연다. 뜬 앱을 돌려준다(끄는 것은 부르는 쪽).
pub async fn start_demo(
    settings: Settings,
    assembly: RunAssembly,
    request: &DemoRequest,
    output: &mut dyn Write,
    runner: &dyn CommandRunner,
) -> Result<App, AppError> {
    let home = settings.home.clone();
    let app = App::launch_run(settings, assembly).await?;
    let addr = crate::announce_addr(app.local_addr());
    let url = format!("http://{addr}{}", OpenScreen::Office.path());
    let _ = writeln!(output, "{DEMO_NOTICE}");
    let _ = writeln!(output, "시연 보관 폴더: {}", home.display());
    let _ = writeln!(output, "{url}");
    match seed_template(&app) {
        Ok(true) => {
            let _ = writeln!(output, "{DEMO_TEMPLATE_NOTICE}");
        }
        Ok(false) => {}
        Err(error) => tracing::warn!("시연 템플릿을 골라 두지 못했습니다: {error}"),
    }
    if !request.no_open && !open_browser(runner, &url) {
        let _ = writeln!(output, "브라우저를 열지 못했습니다. 위 주소를 직접 여세요");
    }
    Ok(app)
}

/// 고른 템플릿이 하나도 없으면(어느 상태든) 스윙 템플릿을 모의중으로 하나 만든다(템플릿 화면에서 고른 것과 같은 값).
/// 만들었는가. 쓰는 템플릿이 없으면 팀이 시세 조건 · 공시를 걸지 않아 체험 동안 흐름이 보이지 않는다.
fn seed_template(app: &App) -> Result<bool, pi_store::StoreError> {
    let store = app.store();
    for state in TemplateState::ALL {
        if !store.templates_in_state(state)?.is_empty() {
            return Ok(false);
        }
    }
    let Ok(value) = serde_json::from_str(&Template::new(Style::Swing).to_json()) else {
        return Ok(false);
    };
    let at = UnixMillis(chrono::Utc::now().timestamp_millis());
    store.create_template(NewTemplate { value, at })?;
    Ok(true)
}
