//! 터미널의 되살리기(`price-inha restore <백업 파일> --identity <age 비밀키 파일> --into <빈 폴더>`).
//! [`crate::restore_backup`]을 부른다. 비밀키 파일 내용은 어디에도 남기지 않는다.

use std::path::{Path, PathBuf};

use pi_backup::RestoreReport;
use pi_secrets::SecretString;
use pi_store::UnixMillis;
use zeroize::Zeroizing;

use crate::error::CliError;
use crate::restore_backup;

/// `restore`의 인자.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreRequest {
    /// 암호화된 백업 파일.
    pub backup: PathBuf,
    /// age 비밀키 파일(`age-keygen` 출력 모양: `#` 주석 줄과 비밀키 한 줄).
    pub identity: PathBuf,
    /// 되살릴 폴더. 없거나 비어 있어야 한다.
    pub into: PathBuf,
}

/// 되살린 결과.
#[derive(Debug)]
pub struct RestoreSummary {
    /// 되살리기 보고.
    pub report: RestoreReport,
    /// 터미널에 보여 줄 글: "팀은 멈춤, 기다리던 승인은 만료(V51 · M5)"와 다음에 띄울 명령.
    pub text: String,
}

/// 백업을 빈 폴더에 되살린다. 빈 폴더가 아니면 [`CliError::NotEmpty`]이고 아무것도 바꾸지 않는다.
pub fn restore(request: &RestoreRequest, at: UnixMillis) -> Result<RestoreSummary, CliError> {
    let into = absolute(&request.into)?;
    if !empty_or_missing(&into) {
        return Err(CliError::NotEmpty { into });
    }
    let encrypted = std::fs::read(&request.backup).map_err(|error| CliError::ReadFile {
        what: "백업",
        kind: error.kind(),
    })?;
    let identity = read_identity(&request.identity)?;
    let report = restore_backup(&encrypted, &identity, &into, at)?;
    let expired = report.outcome.expired_approvals.len();
    let home = into.display();
    let text = format!(
        "{home} 에 되살렸습니다.\n\
팀은 멈춤 상태이고, 기다리던 승인 {expired}건은 만료로 닫았습니다(V51 · M5). \
팀은 화면에서 직접 다시 켭니다.\n\
다음에 띄울 명령: price-inha run --home {home}\n"
    );
    Ok(RestoreSummary { report, text })
}

/// 상대 경로면 지금 폴더 아래로.
fn absolute(path: &Path) -> Result<PathBuf, CliError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let here = std::env::current_dir().map_err(|error| CliError::ReadFile {
        what: "지금 폴더",
        kind: error.kind(),
    })?;
    Ok(here.join(path))
}

/// 없거나 빈 폴더인가.
fn empty_or_missing(path: &Path) -> bool {
    match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

/// `age-keygen` 출력 모양의 파일에서 비밀키 한 줄을 읽는다. 읽은 글은 쓰고 나면 지운다(내용은 어디에도 남기지 않는다).
fn read_identity(path: &Path) -> Result<SecretString, CliError> {
    let text =
        Zeroizing::new(
            std::fs::read_to_string(path).map_err(|error| CliError::ReadFile {
                what: "비밀키",
                kind: error.kind(),
            })?,
        );
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with(AGE_SECRET_PREFIX))
        .map(SecretString::new)
        .ok_or(CliError::BadIdentity)
}

/// age 비밀키 줄의 앞머리.
const AGE_SECRET_PREFIX: &str = "AGE-SECRET-KEY-";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 비밀키가_없는_파일은_거부한다() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("id.txt");
        std::fs::write(&path, "# 주석뿐\n").unwrap();
        assert!(matches!(read_identity(&path), Err(CliError::BadIdentity)));
    }

    #[test]
    fn 빈_폴더와_없는_폴더만_받는다() {
        let dir = tempfile::tempdir().unwrap();
        assert!(empty_or_missing(dir.path()));
        assert!(empty_or_missing(&dir.path().join("none")));
        std::fs::write(dir.path().join("a"), b"x").unwrap();
        assert!(!empty_or_missing(dir.path()));
    }
}
