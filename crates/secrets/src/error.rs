//! 이 모듈의 오류 타입. 어떤 변형도 비밀 값이나 자유 글자를 담지 않는다.

/// 확인기가 내는 실패. 자유 글자를 담지 않는다(오류 글에 키가 섞이지 않게).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VerifyFailure {
    /// 서버에 닿았지만 키가 맞지 않다.
    #[error("키가 맞지 않습니다")]
    Rejected,
    /// 서버에 닿지 못했다(연결 · 시간 초과 등).
    #[error("확인할 서버에 닿지 못했습니다")]
    Unreachable,
}

/// 이 모듈의 오류. 어떤 변형도 비밀 값을 담지 않는다.
/// 칸 이름은 [`KeyKind::fields`](crate::KeyKind::fields)의 `&'static str`만 쓴다
/// (호출자가 준 글자를 되돌려 싣지 않는다).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SecretsError {
    /// E4 키 확인 실패. 저장하지 않았다.
    #[error("키 확인에 실패해 저장하지 않았습니다: {0}")]
    VerifyFailed(#[source] VerifyFailure),
    /// 그 종류에 있어야 할 칸이 없다.
    #[error("칸이 빠졌습니다: {field}")]
    MissingField {
        /// 빠진 칸.
        field: &'static str,
    },
    /// 그 종류에 없는 칸이 들어왔다. 들어온 이름은 싣지 않는다.
    #[error("이 키 종류에 없는 칸이 들어왔습니다")]
    UnknownField,
    /// 값이 [`MIN_SECRET_CHARS`](crate::MIN_SECRET_CHARS)보다 짧다(가릴 수 없다).
    #[error("값이 너무 짧습니다(최소 {min}자): {field}")]
    TooShort {
        /// 짧은 값이 든 칸.
        field: &'static str,
        /// 최소 글자 수.
        min: usize,
    },
    /// 값에 파일에 쓸 수 없는 글자(줄바꿈 등)가 있다.
    #[error("값에 쓸 수 없는 글자가 있습니다: {field}")]
    InvalidValue {
        /// 문제의 칸.
        field: &'static str,
    },
    /// 홈 폴더를 알 수 없고 [`HOME_ENV`](crate::HOME_ENV)도 없다.
    #[error("보관 폴더를 정할 수 없습니다")]
    HomeNotFound,
    /// 키 파일 내용을 읽을 수 없다. 파일 내용은 싣지 않는다.
    #[error("키 파일을 읽을 수 없는 모양입니다: {file}")]
    Corrupt {
        /// 파일 이름([`ENV_FILE`](crate::ENV_FILE) 또는 [`AUTH_FILE`](crate::AUTH_FILE)).
        file: &'static str,
    },
    /// 파일 · 폴더 작업 실패. 종류만 싣는다.
    #[error("파일 작업에 실패했습니다({op}): {kind}")]
    Io {
        /// 하던 일(예: "쓰기").
        op: &'static str,
        /// 실패 종류.
        kind: std::io::ErrorKind,
    },
}

impl SecretsError {
    /// `std::io::Error`에서 종류만 남긴다(경로 · OS 글은 싣지 않는다).
    pub(crate) fn io(op: &'static str) -> impl Fn(std::io::Error) -> Self {
        move |e| Self::Io { op, kind: e.kind() }
    }
}
