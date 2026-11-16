//! 백업 · 되살리기 오류. 비밀 값 · 토큰 · identity를 담지 않는다(P22).

use std::path::PathBuf;

/// 백업 · 되살리기 오류. `Display` · `Debug` 어디에도 비밀 값이 나오지 않는다.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BackupError {
    /// 저장소 이름이 `소유자/이름` 모양이 아니다.
    #[error("저장소 이름이 '소유자/이름' 모양이 아니다")]
    InvalidRepo,
    /// 백업 GitHub 토큰이 등록되지 않았다.
    #[error("백업 토큰이 등록되지 않았다")]
    TokenMissing,
    /// 스냅샷에 등록된 비밀 값이 들어 있어 올리지 않았다(E39).
    #[error("스냅샷에 비밀 값이 들어 있어 올리지 않았다")]
    SecretFound,
    /// 저장소 스냅샷 · 되살린 저장소 표시 실패.
    #[error("저장소 오류: {0}")]
    Store(#[from] pi_store::StoreError),
    /// 비밀 보관소 읽기 실패.
    #[error("비밀 보관소 오류: {0}")]
    Secrets(#[from] pi_secrets::SecretsError),
    /// 수신자 공개키가 잘못됐거나 암호화하지 못했다.
    #[error("암호화하지 못했다")]
    Encrypt,
    /// identity가 잘못됐거나 맞지 않아 풀지 못했다.
    #[error("풀지 못했다")]
    Decrypt,
    /// 올리기 · 목록 · 지우기 요청이 실패했다. 응답 상태가 있으면 담는다.
    #[error("원격 요청 실패 (상태 {status:?})")]
    Remote {
        /// HTTP 상태. 닿지 못했으면 `None`.
        status: Option<u16>,
    },
    /// 올린 뒤 원격이 알려 준 크기가 보낸 크기와 다르다.
    #[error("올린 크기가 다르다 (보냄 {sent}, 원격 {stored})")]
    SizeMismatch {
        /// 보낸 바이트 수.
        sent: u64,
        /// 원격이 알려 준 바이트 수.
        stored: u64,
    },
    /// 묶음 검증 실패. 되살리기는 아무것도 쓰지 않는다.
    #[error("묶음이 온전하지 않다: {0:?}")]
    InvalidBundle(BundleProblem),
    /// 되살릴 대상이 빈 폴더가 아니다(파일이거나 무언가 들어 있다).
    #[error("빈 폴더가 아니다: {0}")]
    TargetNotEmpty(PathBuf),
    /// 파일 읽기 · 쓰기 실패.
    #[error("파일 오류: {path}")]
    Io {
        /// 그 파일.
        path: PathBuf,
        /// 원인.
        #[source]
        source: std::io::Error,
    },
}

/// 묶음 검증에서 걸린 것.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BundleProblem {
    /// 묶음 모양을 읽을 수 없다.
    Malformed,
    /// manifest 판 번호가 [`crate::MANIFEST_VERSION`]이 아니다.
    UnsupportedVersion(u32),
    /// 경로가 상대 경로 한 조각이 아니다(절대 경로 · `..` · 폴더 나눔 · 빈 이름).
    UnsafePath,
    /// manifest가 말한 파일이 묶음에 없다.
    MissingFile,
    /// 묶음에 manifest에 없는 파일이 있다.
    UnlistedFile,
    /// 크기나 SHA-256이 manifest와 다르다.
    HashMismatch,
    /// [`crate::SNAPSHOT_FILE`]이 없다.
    MissingSnapshot,
    /// 스냅샷이 온전한 SQLite 저장소가 아니다.
    CorruptDatabase,
}

/// 실패의 종류. 알림 요구와 상태 기록에 쓴다(E15). 자유 글자를 담지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FailureKind {
    /// 토큰이 없다.
    TokenMissing,
    /// 스냅샷에 비밀 값이 있다(E39).
    SecretFound,
    /// 스냅샷을 뜨지 못했다.
    Snapshot,
    /// 암호화하지 못했다.
    Encrypt,
    /// 올리지 못했다.
    Upload,
    /// 올린 크기가 다르다.
    SizeMismatch,
    /// 로컬 파일 오류.
    Local,
}

impl BackupError {
    /// 이 오류의 실패 종류.
    pub fn kind(&self) -> FailureKind {
        todo!()
    }
}
