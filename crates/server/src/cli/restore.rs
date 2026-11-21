//! 터미널의 되살리기(`price-inha restore <백업 파일> --identity <age 비밀키 파일> --into <빈 폴더>`).
//! [`crate::restore_backup`]을 부른다. 비밀키 파일 내용은 어디에도 남기지 않는다.

use std::path::PathBuf;

use pi_backup::RestoreReport;
use pi_store::UnixMillis;

use crate::error::CliError;

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
    let _ = (request, at);
    todo!()
}
