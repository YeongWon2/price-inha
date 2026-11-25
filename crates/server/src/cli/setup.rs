//! (37차) `price-inha setup [구역]`: 터미널 전용 처음 설정 마법사.
//!
//! 지시서 `docs/dev/modules/server.md` "37차", 결정 24. 원본 동작 근거: `docs/research/onboarding-messenger.md`
//! 2.1(OpenClaw `onboard` · Hermes `setup`의 단계 · 다시 실행 · 비대화형 종료 코드 2 · 구역 인자),
//! 2.2(CLI 로그인 재사용을 먼저 권함), 5장 (가) · (나), `docs/research/install-and-cli.md`.
//!
//! # 계약
//!
//! - **저장 길은 하나**: 마법사는 보관 폴더의 파일을 직접 쓰지 않는다. 도는 서버의 같은 컴퓨터(루프백) 설정
//!   경로(`/api/setup/ai` · `/api/setup/keys` · `/api/setup/symbols`)를 그대로 부른다(`access-key`가 도는
//!   서버를 찾고 세션을 받는 방식과 같다). 그래서 등록 전 확인 · 시도 제한 · 원자 쓰기 · 가림 규칙이 화면과
//!   한 벌이다. 서버의 거절은 [`crate::SetupRefusal`]의 글(`Display`)로 보여 준다.
//! - **도는 서버가 없으면** [`SETUP_START_SERVER_QUESTION`]을 묻는다(빈 줄 = 예). 예면 `server_exe`를
//!   `run`과 같은 설정 인자(`--home` · `--port` 등)로 뒤에서 띄우고, 세션이 답할 때까지 기다린 뒤 이어 간다.
//!   마법사가 띄운 서버는 서비스로 등록하지 않았으면(또는 `--dry-run`이면) 마법사가 끝날 때 끈다.
//!   아니오면 [`SETUP_RUN_HINT`]를 말하고 [`SETUP_FAILED_EXIT`]로 끝난다(아무것도 저장하지 않는다).
//! - **0단계**: 보관 폴더 위치(경로 그대로)와 권한을 보여 준다. 구역을 주지 않았고 ai · broker · symbols
//!   가운데 이미 된 것이 있으면 그 줄에 [`SETUP_ALREADY`]를 붙여 보이고 "다시 할 구역"을 묻는다.
//!   답: 쉼표로 나눈 구역 이름([`SetupSection::as_str`]). 빈 줄 = 아직 안 된 구역 + `service` + `open`.
//!   고르지 않은 구역은 끝 요약에 [`SETUP_SKIPPED`](까닭 [`SETUP_ALREADY`])로 나온다.
//! - **AI 연결 구역**: `claude`를 **`PATH`에서만** 찾는다(흔한 설치 자리는 보지 않는다). 있으면 지금의 확인 길
//!   (버전 → 로그인, 모델을 부르지 않는다)로 보고 결과를 보여 준 뒤 고르게 한다. 답 [`ANSWER_CLI`] ·
//!   [`ANSWER_KEY`] · [`ANSWER_SKIP`]. 빈 줄 = 로그인된 CLI를 찾았으면 `cli`, 아니면 `skip`.
//!   - `cli`인데 로그인 안 됨: 저장하기 전에 [`SETUP_CLI_LOGIN_HINT`] 한 줄을 그대로 보이고 한 줄(Enter)을 읽은 뒤
//!     다시 확인한다. 그래도 안 됐으면 같은 안내를 되풀이한다. 로그인됐으면 서버의 `/api/setup/ai`로 CLI 방식을
//!     고른다(서버가 다시 확인한다).
//!   - `cli`인데 찾지 못함: 고를 수 없는 답이다(다시 묻는다).
//!   - 환경에 `ANTHROPIC_API_KEY`가 있는데 `cli`를 고르면 [`SETUP_CLI_BILLING_WARNING`] 한 줄(그 값은 찍지 않는다).
//!   - `key`: API 키 한 줄(터미널이면 보이지 않는 입력, `--stdin-answers`면 그 줄). 빈 줄 = 건너뛰기.
//!     거절되면 거절 글을 보이고 키를 다시 묻는다. [`crate::SetupRefusal::TooManyAttempts`]면 그 구역은
//!     [`SETUP_FAILED`]이고 더 묻지 않고 다음 구역으로 간다.
//!   - **다른 프로그램의 토큰 파일 · 키체인을 읽지 않는다. OAuth를 하지 않는다**(결정 24 · D4).
//! - **증권 키 구역**: `/keys` 화면의 칸과 같게 `client_id` → `client_secret` 차례로 한 줄씩. 첫 칸이 빈 줄이면
//!   건너뛰기. 거절되면 두 칸을 다시 묻는다(시도 제한이면 AI와 같다).
//! - **종목 구역**: 쉼표로 나눈 종목 한 줄(빈 줄 = 건너뛰기). 거절되면 다시 묻는다. 저장한 종목 가운데 업종표에
//!   없는 것이 있으면 [`SETUP_SECTOR_NOTICE`] 한 줄.
//! - **메신저 구역**(39차): 구역 전체를 돌 때(`setup`)는 묻지 않는다 — 제목 "메신저"와 [`SETUP_MESSENGER_GUIDE`]
//!   한 줄을 보이고 요약은 [`SETUP_SKIPPED`]다(37차의 [`SETUP_MESSENGER_LATER`] 줄을 이것으로 바꾼다 — 다른 구역의
//!   답 차례는 그대로). `setup messenger`일 때만 묻는다: [`SETUP_MESSENGER_TOKEN_PROMPT`] → 봇 토큰 한 줄(터미널이면
//!   보이지 않는 입력, `--stdin-answers`면 그 줄, 빈 줄 = 건너뛰기) → 도는 서버의 `crate::MESSENGER_TELEGRAM_PATH`로
//!   등록(서버가 getMe로 확인한다 — 38차의 서버 신원 확인을 거친 뒤에만 보낸다). 거절되면 거절 글을 보이고 다시
//!   묻는다(시도 제한이면 [`SETUP_FAILED`]). 됐으면 봇 이름과 [`SETUP_MESSENGER_SEND_HINT`]를 보이고
//!   [`SETUP_MESSENGER_CODE_PROMPT`] → 코드 한 줄(빈 줄 = 건너뛰기) → `crate::MESSENGER_PAIRING_APPROVE_PATH`.
//!   틀리면 `crate::MESSENGER_WRONG_CODE`를 보이고 다시 묻는다. 잠겼으면 [`SETUP_FAILED`]. 맞으면
//!   [`SETUP_MESSENGER_PAIRED`]이고 [`SETUP_DONE`]. 토큰은 출력 · 기록 어디에도 다시 찍지 않는다.
//! - **서비스 구역**: "계속 켜 둘까요?" 예/아니오(빈 줄 = 예). 예면 `service install`과 같은 일
//!   (`--dry-run`이면 `service install --dry-run`과 같은 할 일 글만 보이고 쓰지도 부르지도 않는다).
//!   아니오면 [`SETUP_RUN_HINT`].
//! - **열기 구역**: 묻지 않는다. 대시보드 주소(`http://127.0.0.1:<포트>/`, 비밀값 없음)를 한 줄로 보이고,
//!   터미널일 때만(`--stdin-answers`가 아닐 때) 브라우저로 연다.
//! - **끝 요약**: [`SETUP_SUMMARY_TITLE`] 줄 다음에 표 `| 구역 | 결과 | 까닭 |`, 이번에 다룬 구역마다
//!   `| <구역> | <됨|건너뜀|실패> | <까닭> |` 한 줄. 그다음 [`SETUP_DOCTOR_HINT`]. 비밀값은 화면 · 기록 어디에도
//!   다시 찍지 않는다(받은 키를 되돌려 보이지 않는다).
//! - **종료 코드**: 모든 구역이 됨 · 건너뜀이면 0, 실패가 있으면 [`SETUP_FAILED_EXIT`].
//!
//! # 비대화형 · 답 흘려 넣기(`--stdin-answers`)
//!
//! 표준 입력이 터미널이 아니고 `--stdin-answers`도 없으면 마법사를 돌리지 않고 [`SETUP_NOT_TERMINAL`]을 말하고
//! [`SETUP_NOT_TERMINAL_EXIT`](2)로 끝난다(원본 Hermes). 보관 폴더를 만들지 않는다.
//!
//! `--stdin-answers`(시험 · 자동 설치용 숨은 길): 표준 입력에서 **한 줄에 답 하나**를 묻는 차례대로 읽는다.
//! 줄 앞뒤 공백은 뗀다. 빈 줄은 그 질문의 기본값이다. 예/아니오는 `y` · `n`. 모르는 답이면 같은 질문을 다시
//! 읽는다. 답이 끝나면(표준 입력 끝) 그 구역은 [`SETUP_FAILED`](까닭 "답이 끝남"), 남은 구역은
//! [`SETUP_SKIPPED`]이다. 비밀값은 인자로 받지 않는다(프로세스 목록에 남지 않게) — 표준 입력으로만.
//! 질문과 결과는 표준 출력에 쓰고 줄마다 비운다(flush).

mod ai;
mod broker;
mod prompt;
mod server;
mod service;
mod signals;
mod summary;
mod symbols;

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use self::prompt::{Mode, Prompter};
use self::server::{Connected, Link, Spawned};
use self::summary::{ANSWERS_ENDED, Outcome, any_failed, write_summary};
use super::open::{OpenScreen, open_browser, screen_url};
use super::runner::SystemRunner;
use crate::Settings;
use crate::setup::SETUP_PATH;

/// 마법사의 구역. 차례는 [`SETUP_SECTIONS`]이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetupSection {
    /// AI 연결(Claude CLI · API 키 · 건너뛰기).
    Ai,
    /// 증권 키(모의계좌라 필요한 칸만).
    Broker,
    /// 지켜볼 종목.
    Symbols,
    /// (39차) 메신저(텔레그램) 연결: 토큰 → 확인 → 페어링 코드 → 승인. 구역 전체를 돌 때는 안내 한 줄만.
    Messenger,
    /// 계속 켜 두기(서비스 등록).
    Service,
    /// 대시보드 열기.
    Open,
}

/// 구역을 주지 않았을 때 도는 차례.
pub const SETUP_SECTIONS: [SetupSection; 6] = [
    SetupSection::Ai,
    SetupSection::Broker,
    SetupSection::Symbols,
    SetupSection::Messenger,
    SetupSection::Service,
    SetupSection::Open,
];

impl SetupSection {
    /// 명령 줄 · 요약 표에 쓰는 이름(`ai` · `broker` · `symbols` · `messenger` · `service` · `open`).
    pub const fn as_str(self) -> &'static str {
        match self {
            SetupSection::Ai => "ai",
            SetupSection::Broker => "broker",
            SetupSection::Symbols => "symbols",
            SetupSection::Messenger => "messenger",
            SetupSection::Service => "service",
            SetupSection::Open => "open",
        }
    }

    /// 이름 → 구역. 모르는 이름이면 `None`.
    pub fn parse(name: &str) -> Option<SetupSection> {
        SETUP_SECTIONS
            .into_iter()
            .find(|section| section.as_str() == name)
    }
}

/// 표준 입력이 터미널이 아니고 `--stdin-answers`도 없을 때의 종료 코드(원본 Hermes).
pub const SETUP_NOT_TERMINAL_EXIT: u8 = 2;
/// 실패한 구역이 있거나 도는 서버 없이 멈췄을 때의 종료 코드.
pub const SETUP_FAILED_EXIT: u8 = 1;
/// 비대화형일 때의 안내.
pub const SETUP_NOT_TERMINAL: &str =
    "터미널에서 `price-inha setup`을 실행하거나 브라우저 설정 화면을 여세요";
/// 도는 서버가 없을 때의 질문(빈 줄 = 예).
pub const SETUP_START_SERVER_QUESTION: &str = "도는 서버가 없습니다. 지금 띄울까요?";
/// 0단계에서 이미 된 항목에 붙는 말. 고르지 않아 건너뛴 구역의 까닭이기도 하다.
pub const SETUP_ALREADY: &str = "있음";
/// CLI는 있지만 로그인 안 됐을 때의 안내(한 줄 그대로).
pub const SETUP_CLI_LOGIN_HINT: &str =
    "다른 창에서 `claude auth login`을 실행한 뒤 Enter를 누르세요";
/// 환경에 `ANTHROPIC_API_KEY`가 있는데 CLI 방식을 고를 때의 경고(값은 찍지 않는다).
pub const SETUP_CLI_BILLING_WARNING: &str =
    "환경에 ANTHROPIC_API_KEY가 있어 CLI도 API 키로 과금될 수 있습니다";
/// 업종표에 없는 종목을 저장했을 때의 안내(35차).
pub const SETUP_SECTOR_NOTICE: &str =
    "업종표에 없는 종목은 규칙 화면에서 업종을 넣어야 살 수 있습니다";
/// 메신저 구역 자리의 한 줄.
pub const SETUP_MESSENGER_LATER: &str = "메신저는 나중에 `price-inha setup messenger`로 연결합니다";
/// (39차) 구역 전체를 돌 때 메신저 구역 자리의 안내(묻지 않는다).
pub const SETUP_MESSENGER_GUIDE: &str =
    "메신저(텔레그램)는 `price-inha setup messenger`로 따로 연결합니다";
/// (39차) 봇 토큰을 묻는 말(빈 줄 = 건너뛰기).
pub const SETUP_MESSENGER_TOKEN_PROMPT: &str =
    "텔레그램 봇 토큰(BotFather가 준 것, 빈 줄 = 건너뛰기)";
/// (39차) 토큰이 확인된 뒤의 안내.
pub const SETUP_MESSENGER_SEND_HINT: &str =
    "봇에게 아무 말이나 보내세요. 받은 코드를 아래에 넣습니다";
/// (39차) 페어링 코드를 묻는 말(빈 줄 = 건너뛰기).
pub const SETUP_MESSENGER_CODE_PROMPT: &str = "페어링 코드";
/// (39차) 승인이 끝났을 때.
pub const SETUP_MESSENGER_PAIRED: &str = "메신저가 연결됐습니다";
/// 계속 켜 두지 않을 때 · 서버를 띄우지 않을 때의 안내.
pub const SETUP_RUN_HINT: &str = "`price-inha run`으로 켭니다";
/// 끝 요약의 제목 줄.
pub const SETUP_SUMMARY_TITLE: &str = "요약";
/// 요약의 결과: 됨.
pub const SETUP_DONE: &str = "됨";
/// 요약의 결과: 건너뜀.
pub const SETUP_SKIPPED: &str = "건너뜀";
/// 요약의 결과: 실패.
pub const SETUP_FAILED: &str = "실패";
/// 요약 뒤의 안내.
pub const SETUP_DOCTOR_HINT: &str = "남은 일은 `price-inha doctor`로 봅니다";
/// AI 구역의 답: Claude CLI.
pub const ANSWER_CLI: &str = "cli";
/// AI 구역의 답: API 키 입력.
pub const ANSWER_KEY: &str = "key";
/// AI 구역의 답: 건너뛰기.
pub const ANSWER_SKIP: &str = "skip";

/// 서버의 답을 읽지 못했을 때의 까닭.
const BROKEN: &str = "서버의 답을 읽지 못함";

/// `setup`의 선택.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SetupRequest {
    /// 한 구역만(`setup ai`). `None`이면 [`SETUP_SECTIONS`] 차례로 모두.
    pub section: Option<SetupSection>,
    /// 표준 입력에서 답을 흘려 받는다(모듈 문서의 형식).
    pub stdin_answers: bool,
    /// 서비스 구역을 `service install --dry-run`처럼만 한다.
    pub dry_run: bool,
    /// 설정 인자(`--home` · `--port` 등). 마법사가 서버를 띄울 때 `run`에 그대로 싣는다.
    pub args: Vec<String>,
}

/// 마법사가 바깥과 닿는 자리. 실행 파일은 실제 표준 입출력 · 프로세스 환경 · 자기 경로를 준다.
pub struct SetupContext<'a> {
    /// 답을 읽는 곳.
    pub input: &'a mut dyn BufRead,
    /// 질문 · 결과 · 요약을 쓰는 곳.
    pub output: &'a mut dyn Write,
    /// 환경 변수 읽기(`PATH` · `ANTHROPIC_API_KEY` · `HOME`).
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// 표준 입력이 터미널인가.
    pub interactive: bool,
    /// 도는 서버가 없을 때 띄울 실행 파일(보통 자기 자신).
    pub server_exe: PathBuf,
}

/// 마법사를 돌리고 종료 코드를 돌려준다(모듈 문서의 계약).
pub async fn setup(settings: &Settings, request: &SetupRequest, context: SetupContext<'_>) -> u8 {
    let SetupContext {
        input,
        output,
        env,
        interactive,
        server_exe,
    } = context;
    if !interactive && !request.stdin_answers {
        let _ = writeln!(output, "{SETUP_NOT_TERMINAL}");
        let _ = output.flush();
        return SETUP_NOT_TERMINAL_EXIT;
    }
    let terminal = !request.stdin_answers;
    let mode = if terminal {
        signals::watch();
        Mode::Terminal { hide: true }
    } else {
        Mode::Lines
    };
    let runner = SystemRunner;
    let mut prompter = Prompter::new(input, output, mode, &runner);
    prompter.title("price-inha 처음 설정");
    show_home(&mut prompter, &settings.home);
    let (link, spawned) = match server::connect(settings, request, &server_exe, &mut prompter).await
    {
        Connected::Ready(link, spawned) => (link, spawned),
        Connected::Stop(code) => return code,
    };
    let mut run = Wizard {
        settings,
        request,
        link,
        spawned,
        env,
        terminal,
    };
    let (rows, picked) = run.sections(&mut prompter).await;
    if let Some(server) = run.spawned.take() {
        server.stop().await;
        prompter.say("마법사가 띄운 서버를 껐습니다");
    }
    write_summary(&mut prompter, &rows);
    if any_failed(&rows) || !picked {
        SETUP_FAILED_EXIT
    } else {
        0
    }
}

/// 0단계: 보관 폴더 위치(경로 그대로)와 권한.
fn show_home(prompter: &mut Prompter<'_>, home: &Path) {
    let Ok(meta) = std::fs::metadata(home) else {
        prompter.say(&format!(
            "보관 폴더: {} (아직 없음 — 서버가 켤 때 0700으로 만듭니다)",
            home.display()
        ));
        return;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode() & 0o777;
        let open = if mode & 0o077 == 0 {
            "나만 읽고 씀"
        } else {
            "다른 사용자도 볼 수 있음 — 0700을 권합니다"
        };
        prompter.say(&format!(
            "보관 폴더: {} (권한 {mode:o}, {open})",
            home.display()
        ));
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        prompter.say(&format!("보관 폴더: {}", home.display()));
    }
}

/// 한 번 도는 마법사의 상태.
struct Wizard<'a> {
    settings: &'a Settings,
    request: &'a SetupRequest,
    link: Link,
    spawned: Option<Spawned>,
    env: &'a dyn Fn(&str) -> Option<String>,
    terminal: bool,
}

impl Wizard<'_> {
    /// 구역들을 돌고 요약 줄들을 돌려준다. 구역 고르기에서 답이 끝났으면 두 번째 값이 거짓.
    async fn sections(
        &mut self,
        prompter: &mut Prompter<'_>,
    ) -> (Vec<(SetupSection, Outcome)>, bool) {
        let listed: Vec<SetupSection> = match self.request.section {
            Some(section) => vec![section],
            None => SETUP_SECTIONS.to_vec(),
        };
        let picked = self.pick(prompter).await;
        let mut ended = picked.is_none();
        let chosen = picked.clone().unwrap_or_default();
        let mut rows = Vec::new();
        for section in listed {
            let outcome = if ended {
                Outcome::skipped(ANSWERS_ENDED)
            } else if !chosen.contains(&section) {
                Outcome::skipped(SETUP_ALREADY)
            } else {
                self.section(section, prompter).await
            };
            ended = ended || outcome.input_ended();
            if section == SetupSection::Symbols && self.request.section.is_none() && !ended {
                prompter.title("메신저");
                prompter.say(SETUP_MESSENGER_LATER);
            }
            rows.push((section, outcome));
        }
        (rows, picked.is_some())
    }

    async fn section(&mut self, section: SetupSection, prompter: &mut Prompter<'_>) -> Outcome {
        match section {
            SetupSection::Ai => {
                let outside = ai::AiEnv { env: self.env };
                ai::ai(&self.link, prompter, &outside).await
            }
            SetupSection::Broker => broker::broker(&self.link, prompter).await,
            SetupSection::Symbols => symbols::symbols(&self.link, prompter).await,
            SetupSection::Messenger => todo!(),
            SetupSection::Service => {
                service::service(
                    self.settings,
                    &self.request.args,
                    self.request.dry_run,
                    &mut self.spawned,
                    prompter,
                )
                .await
            }
            SetupSection::Open => self.open(prompter),
        }
    }

    /// 0단계의 나머지: 이미 된 것을 보이고, 구역을 주지 않았으면 다시 할 구역을 고르게 한다.
    /// 답이 끝났으면 `None`.
    async fn pick(&self, prompter: &mut Prompter<'_>) -> Option<Vec<SetupSection>> {
        if let Some(section) = self.request.section {
            return Some(vec![section]);
        }
        let status = self.link.get(SETUP_PATH).await.unwrap_or(Value::Null);
        let saved = [
            SetupSection::Ai,
            SetupSection::Broker,
            SetupSection::Symbols,
        ];
        let done: Vec<SetupSection> = saved
            .into_iter()
            .filter(|section| registered(&status, section.as_str()))
            .collect();
        prompter.say("지금 설정 상태:");
        for section in saved {
            let state = if done.contains(&section) {
                SETUP_ALREADY
            } else {
                "없음"
            };
            prompter.say(&format!("  {}: {state}", section.as_str()));
        }
        if done.is_empty() {
            return Some(SETUP_SECTIONS.to_vec());
        }
        let names: Vec<&str> = SETUP_SECTIONS.iter().map(|s| s.as_str()).collect();
        loop {
            let question = format!(
                "다시 할 구역을 쉼표로 고르세요({}, 빈 줄 = 아직 안 된 구역과 service · open):",
                names.join(", ")
            );
            let answer = prompter.line(&question)?;
            match choose(&answer, &done) {
                Ok(chosen) => return Some(chosen),
                Err(unknown) => prompter.say(&format!("모르는 구역입니다: {unknown}")),
            }
        }
    }

    /// 열기 구역: 묻지 않는다. 주소를 보이고 터미널이면 브라우저로 연다.
    fn open(&self, prompter: &mut Prompter<'_>) -> Outcome {
        prompter.title("대시보드 열기");
        let url = screen_url(self.link.port, OpenScreen::Home);
        prompter.say(&format!("대시보드: {url}"));
        if self.spawned.is_some() {
            prompter.say(&format!(
                "마법사가 띄운 서버는 끝날 때 끕니다. {SETUP_RUN_HINT} — 그 뒤 이 주소를 여세요"
            ));
            return Outcome::done("주소를 보임");
        }
        if !self.terminal {
            return Outcome::done("주소를 보임");
        }
        if open_browser(&SystemRunner, &url) {
            Outcome::done("브라우저로 엶")
        } else {
            prompter.say("브라우저를 열지 못했습니다. 위 주소를 직접 여세요");
            Outcome::done("주소를 보임")
        }
    }
}

/// 설정 상태에서 그 항목이 등록됐는가.
fn registered(status: &Value, name: &str) -> bool {
    status
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| items.iter().find(|item| item["name"] == json!(name)))
        .and_then(|item| item["registered"].as_bool())
        .unwrap_or(false)
}

/// "다시 할 구역" 답 → 고른 구역(도는 차례). 빈 줄 = 아직 안 된 구역 + `service` + `open`.
/// 모르는 이름이 있으면 그 이름.
fn choose(answer: &str, done: &[SetupSection]) -> Result<Vec<SetupSection>, String> {
    let names: Vec<&str> = answer
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    if names.is_empty() {
        return Ok(SETUP_SECTIONS
            .into_iter()
            .filter(|section| !done.contains(section))
            .collect());
    }
    let mut chosen = Vec::new();
    for name in names {
        let section = SetupSection::parse(&name.to_lowercase()).ok_or_else(|| name.to_string())?;
        chosen.push(section);
    }
    Ok(SETUP_SECTIONS
        .into_iter()
        .filter(|section| chosen.contains(section))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 구역_고르기() {
        let done = [SetupSection::Symbols];
        assert_eq!(
            choose("", &done).unwrap(),
            [
                SetupSection::Ai,
                SetupSection::Broker,
                SetupSection::Service,
                SetupSection::Open
            ]
        );
        assert_eq!(
            choose("open, symbols", &done).unwrap(),
            [SetupSection::Symbols, SetupSection::Open]
        );
        assert_eq!(choose("ai,rockets", &done), Err("rockets".to_string()));
    }

    #[test]
    fn 등록_여부를_읽는다() {
        let status = json!({"items": [{"name": "ai", "registered": true}, {"name": "broker", "registered": false}]});
        assert!(registered(&status, "ai"));
        assert!(!registered(&status, "broker"));
        assert!(!registered(&Value::Null, "symbols"));
    }
}
