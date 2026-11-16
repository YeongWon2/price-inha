//! 되살리기 (V51 · F9, 원본 동작: OpenClaw `backup restore` — 빈 폴더에만, 쓰기 전에 검증).

use std::path::Path;

use pi_secrets::SecretString;
use pi_store::{RestoreOutcome, UnixMillis};

use crate::{BackupError, Manifest};

/// 되살리기 결과. identity는 담지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    /// 되살린 묶음의 manifest.
    pub manifest: Manifest,
    /// [`pi_store::Store::mark_restored`]가 한 일(팀 전체 정지 · 만료한 승인).
    pub outcome: RestoreOutcome,
}

/// 암호문을 `target` 폴더에 되살린다.
///
/// 1. `target`은 없거나 빈 폴더여야 한다. 아니면 [`BackupError::TargetNotEmpty`]이고 아무것도 쓰지 않는다
///    (제자리 되살리기 · 덮어쓰기는 없다).
/// 2. `identity`로 풀고([`crate::open`]) 검증한다([`crate::verify`]). 실패하면 아무것도 쓰지 않는다
///    (`target`이 없었으면 만들지도 않는다).
/// 3. 파일을 쓰고, `target/`[`crate::SNAPSHOT_FILE`] 저장소에 `mark_restored(at)`을 부른다:
///    팀은 전체 정지가 되고 기다리던 승인은 만료된다.
///
/// `identity`는 인자로만 쓰고 파일 · 기록 · 오류 어디에도 남기지 않는다.
pub fn restore(
    encrypted: &[u8],
    identity: &SecretString,
    target: &Path,
    at: UnixMillis,
) -> Result<RestoreReport, BackupError> {
    todo!()
}
