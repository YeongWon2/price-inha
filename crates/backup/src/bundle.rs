//! 묶음: manifest와 파일들, 그리고 age 암호화 · 풀기 · 검증 (D7, 원본 동작: OpenClaw manifest · verify).

use pi_secrets::SecretString;
use pi_store::UnixMillis;

use crate::BackupError;

/// manifest. 묶음 안에 함께 든다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// 판 번호([`crate::MANIFEST_VERSION`]).
    pub version: u32,
    /// 스냅샷을 뜬 시각.
    pub created_at: UnixMillis,
    /// 담긴 파일들.
    pub files: Vec<ManifestFile>,
}

/// manifest의 파일 한 줄.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestFile {
    /// 묶음 안 경로. 상대 경로 한 조각이어야 한다.
    pub path: String,
    /// 바이트 수.
    pub size: u64,
    /// SHA-256(소문자 16진수 64글자).
    pub sha256: String,
}

/// 풀어 놓은 묶음.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// manifest.
    pub manifest: Manifest,
    /// 파일들.
    pub files: Vec<BundleFile>,
}

/// 묶음 안의 파일 하나.
#[derive(Clone, PartialEq, Eq)]
pub struct BundleFile {
    /// 묶음 안 경로.
    pub path: String,
    /// 내용.
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for BundleFile {
    /// 내용은 찍지 않는다(경로와 길이만).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BundleFile")
            .field("path", &self.path)
            .field("len", &self.bytes.len())
            .finish()
    }
}

/// 바이트의 SHA-256을 소문자 16진수 64글자로.
pub fn sha256_hex(bytes: &[u8]) -> String {
    todo!()
}

/// 묶음을 age 수신자 공개키(`age1…`)로 암호화한다. **검증하지 않고** 그대로 담는다.
///
/// 공개키가 잘못됐으면 [`BackupError::Encrypt`].
pub fn seal(bundle: &Bundle, recipient: &str) -> Result<Vec<u8>, BackupError> {
    todo!()
}

/// 암호문을 비밀키 identity(`AGE-SECRET-KEY-1…`)로 풀어 묶음으로 읽는다. **검증하지 않는다.**
///
/// identity가 잘못됐거나 맞지 않으면 [`BackupError::Decrypt`], 묶음 모양을 읽을 수 없으면
/// [`BackupError::InvalidBundle`]([`crate::BundleProblem::Malformed`]). identity는 어디에도 남기지 않는다.
pub fn open(encrypted: &[u8], identity: &SecretString) -> Result<Bundle, BackupError> {
    todo!()
}

/// 묶음을 검증한다. 하나라도 걸리면 [`BackupError::InvalidBundle`].
///
/// 판 번호 → 경로(상대 경로 한 조각만) → manifest가 말한 파일이 다 있는가 → manifest에 없는 파일이
/// 없는가 → 크기 · SHA-256 → [`crate::SNAPSHOT_FILE`]이 있는가 → 그 스냅샷이 온전한 SQLite 저장소인가.
pub fn verify(bundle: &Bundle) -> Result<(), BackupError> {
    todo!()
}
