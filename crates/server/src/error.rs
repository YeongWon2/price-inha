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
    /// 안에서 돌던 작업이 끝나지 못했다.
    #[error("서버 작업이 비정상으로 끝났습니다")]
    Task,
}
