//! 실행 파일의 오류 타입. 어떤 변형도 비밀 값을 담지 않는다.

/// 설정을 읽지 못했다. 인자의 **값**은 싣지 않는다(비밀 값을 인자로 잘못 줬을 때 새지 않게).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SettingsError {
    /// 모르는 인자다. 비밀 값을 받는 인자는 없으므로 그런 인자도 여기에 걸린다.
    #[error("모르는 인자입니다")]
    UnknownArgument,
    /// 값을 받는 인자인데 값이 없다.
    #[error("인자에 값이 없습니다: {name}")]
    MissingValue {
        /// 인자 이름(예: `--port`).
        name: &'static str,
    },
    /// 값을 읽을 수 없다(숫자가 아닌 포트, 주소가 아닌 글자, 모르는 시장 등).
    #[error("값을 읽을 수 없습니다: {name}")]
    InvalidValue {
        /// 인자 이름 또는 환경 변수 이름.
        name: &'static str,
    },
    /// 보관 폴더를 정할 수 없다(인자 · 환경 변수 · 홈 폴더가 모두 없다).
    #[error("보관 폴더를 정할 수 없습니다")]
    HomeNotFound,
}

/// 서버를 띄우거나 끄다가 난 오류.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AppError {
    /// 저장소를 열거나 쓰지 못했다.
    #[error("저장소 오류: {0}")]
    Store(#[from] pi_store::StoreError),
    /// 비밀 보관소를 열거나 읽지 못했다.
    #[error("비밀 보관 오류: {0}")]
    Secrets(#[from] pi_secrets::SecretsError),
    /// 접속 관문을 열거나 닫지 못했다.
    #[error("접속 관문 오류: {0}")]
    Gate(#[from] pi_gate::GateError),
    /// 보관 폴더를 만들지 못했다. 종류만 싣는다.
    #[error("보관 폴더를 준비하지 못했습니다: {kind}")]
    Home {
        /// 실패 종류.
        kind: std::io::ErrorKind,
    },
    /// (7차 12번) 보관 폴더의 설정 파일을 읽을 수 없다. 어느 파일인지(이름)와 종류만 싣는다.
    /// 파일의 내용(값)은 싣지 않는다.
    #[error("보관 폴더의 {file} 파일을 읽을 수 없습니다: {kind}")]
    BadFile {
        /// 파일 이름(예: [`crate::AI_CHOICE_FILE`]).
        file: &'static str,
        /// 실패 종류.
        kind: std::io::ErrorKind,
    },
    /// 안에서 돌던 작업이 끝나지 못했다.
    #[error("서버 작업이 비정상으로 끝났습니다")]
    Task,
}

/// 설정 요청(키 등록 · 종목 등록)을 거부한 까닭. **받은 값은 어디에도 싣지 않는다.**
///
/// 응답은 [`SetupRefusal::status`]와 `{"error": "<`[`SetupRefusal::reason`]`>"}`뿐이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SetupRefusal {
    /// 키가 맞지 않다: 증권사 · AI 공급자가 그 키를 받지 않았다(E4). 저장하지 않는다.
    #[error("키가 맞지 않습니다")]
    KeyRejected,
    /// 서버에 닿지 않는다: 확인할 곳에 연결하지 못했거나 확인을 끝내지 못했다(E4). 저장하지 않는다.
    #[error("서버에 닿지 않습니다")]
    Unreachable,
    /// 입력이 잘못됐다: 읽을 수 없는 본문 · 모르는 항목 · 빠지거나 모르는 칸 · 빈 값 · 너무 짧은 값.
    /// 확인기를 부르지 않는다.
    #[error("입력이 잘못됐습니다")]
    InvalidInput,
    /// 시도가 너무 많다: 정한 시간에 정한 횟수를 넘었다. 확인기를 부르지 않는다.
    #[error("시도가 너무 많습니다. 잠시 뒤에 다시 하세요")]
    TooManyAttempts,
    /// 종목을 인자 · 환경 변수로 줘서 이 실행 동안에는 바꿀 수 없다.
    #[error("종목을 인자로 줘서 바꿀 수 없습니다")]
    FixedByArgument,
    /// 저장하지 못했다(파일 쓰기 실패 등). 종류만 알린다.
    #[error("저장하지 못했습니다")]
    SaveFailed,
    /// 본문이 너무 크다: 등록 요청 본문이 크기 상한을 넘었다. 읽지 않고 거부하며 확인기를 부르지 않는다.
    /// (5차, 원본 Hermes api_server의 `body_too_large` · 413)
    #[error("본문이 너무 큽니다")]
    BodyTooLarge,
    /// (6차) CLI가 설치돼 있지 않다: 그 경로에 실행 파일이 없다.
    #[error("Claude Code CLI가 설치돼 있지 않습니다")]
    CliNotInstalled,
    /// (6차) 설치된 CLI의 버전이 너무 낮다(`pi_ai::CLI_MIN_VERSION`).
    #[error("Claude Code CLI 버전이 너무 낮습니다")]
    CliTooOld,
    /// (6차) CLI에 로그인돼 있지 않다. 터미널에서 `claude auth login`.
    #[error("Claude Code CLI에 로그인돼 있지 않습니다. 터미널에서 claude auth login")]
    CliNotLoggedIn,
    /// (6차) CLI가 구독이 아니라 API 키로 로그인돼 있다.
    #[error("Claude Code CLI가 구독이 아닌 API 키로 로그인돼 있습니다")]
    CliNotSubscription,
    /// (6차) CLI를 확인하지 못했다(시간 초과 · 읽지 못함).
    #[error("Claude Code CLI를 확인하지 못했습니다")]
    CliUnchecked,
    /// (7차 10번) 이미 있는 CLI 작업 폴더([`crate::CLI_WORK_DIR`])가 비어 있지 않아 CLI를 쓰지 않는다.
    /// 그 폴더 안의 것은 지우지 않는다.
    #[error("CLI 작업 폴더가 비어 있지 않습니다")]
    CliWorkDirNotEmpty,
    /// (8차 3번) 서버가 꺼지는 중이다: 꺼지기 시작한 뒤의 AI 연결 고르기. 아무것도 저장하지 않고
    /// CLI를 띄우지 않는다.
    #[error("서버가 꺼지는 중입니다")]
    ShuttingDown,
}

impl SetupRefusal {
    /// 응답의 `error`에 적는 글자.
    pub const fn reason(self) -> &'static str {
        match self {
            SetupRefusal::KeyRejected => "key_rejected",
            SetupRefusal::Unreachable => "unreachable",
            SetupRefusal::InvalidInput => "invalid_input",
            SetupRefusal::TooManyAttempts => "too_many_attempts",
            SetupRefusal::FixedByArgument => "fixed_by_argument",
            SetupRefusal::SaveFailed => "save_failed",
            SetupRefusal::BodyTooLarge => "body_too_large",
            SetupRefusal::CliNotInstalled => "cli_not_installed",
            SetupRefusal::CliTooOld => "cli_too_old",
            SetupRefusal::CliNotLoggedIn => "cli_not_logged_in",
            SetupRefusal::CliNotSubscription => "cli_not_subscription",
            SetupRefusal::CliUnchecked => "cli_unchecked",
            SetupRefusal::CliWorkDirNotEmpty => "cli_work_dir_not_empty",
            SetupRefusal::ShuttingDown => "shutting_down",
        }
    }

    /// 응답의 HTTP 상태.
    pub const fn status(self) -> u16 {
        match self {
            SetupRefusal::KeyRejected => 422,
            SetupRefusal::Unreachable => 502,
            SetupRefusal::InvalidInput => 400,
            SetupRefusal::TooManyAttempts => 429,
            SetupRefusal::FixedByArgument => 409,
            SetupRefusal::SaveFailed => 500,
            SetupRefusal::BodyTooLarge => 413,
            SetupRefusal::CliNotInstalled
            | SetupRefusal::CliTooOld
            | SetupRefusal::CliNotLoggedIn
            | SetupRefusal::CliNotSubscription => 422,
            SetupRefusal::CliUnchecked => 502,
            SetupRefusal::CliWorkDirNotEmpty => 409,
            SetupRefusal::ShuttingDown => 503,
        }
    }
}

/// (W5 1차) 주문 길([`crate::Trading`])의 오류. 비밀 값을 담지 않는다.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TradingError {
    /// 필수 설정(증권사 · AI 키, 종목)이 빠져 있어 운용하지 않는다(V31). 시연 모드에서는 나지 않는다.
    #[error("필수 설정이 빠져 있어 운용하지 않습니다")]
    SetupRequired,
    /// 고른 템플릿이 없다(C4 먼저).
    #[error("고른 템플릿이 없습니다")]
    NoTemplate,
    /// 팀이 멈춰 있어 오늘 매매를 시작할 수 없다. 지금 상태를 담는다.
    #[error("팀이 멈춰 있습니다: {0:?}")]
    Halted(pi_store::TeamState),
    /// 저장소에 그 매매가 없다.
    #[error("매매를 찾을 수 없습니다")]
    TradeNotFound,
    /// 증권사(모의계좌)가 주문을 받지 않거나 응답하지 않았다(E13). 팀은 이상 감지 멈춤이 됐다.
    #[error("증권사 주문 실패(이상 감지 멈춤): {0}")]
    Broker(String),
    /// 주문 실행기의 그 밖의 오류.
    #[error("주문 실행기 오류: {0}")]
    Exec(String),
    /// 저장소 오류.
    #[error("저장소 오류: {0}")]
    Store(#[from] pi_store::StoreError),
}
