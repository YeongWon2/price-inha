//! 한 번 올리기와 상태 기록 (D7 · C13 · E15 · E39, 원본 동작: OpenClaw 올리기 순서 · Hermes 스냅샷).

use std::path::PathBuf;
use std::sync::Arc;

use pi_secrets::SecretStore;
use pi_store::{Store, UnixMillis};

use crate::{BackupError, FailureKind, Manifest};

/// 백업 설정. 저장소 이름과 토큰은 사용자가 화면에서 등록한다(결정 2026-11-16 23번).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSettings {
    /// GitHub API 기본 주소(끝에 `/` 없이). 운영에서는 GitHub의 주소, 시험에서는 로컬 가짜 서버.
    pub api_base: String,
    /// 비공개 저장소 이름 `소유자/이름`.
    pub repo: String,
    /// age 수신자 공개키(`age1…`). 서버에는 공개키만 둔다.
    pub recipient: String,
    /// 암호화된 로컬 사본과 상태 파일을 두는 폴더. 없으면 만든다.
    pub local_dir: PathBuf,
    /// 평문 스냅샷을 잠깐 뜨는 폴더의 부모. 한 번 돌 때마다 그 안에 새 폴더를 만들고 끝나면(실패해도) 지운다.
    pub temp_root: PathBuf,
}

/// 백업기.
pub struct Backup {
    settings: BackupSettings,
    store: Store,
    secrets: Arc<SecretStore>,
}

/// 성공한 올리기.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupReport {
    /// 이번 백업 시각(`run`에 넘긴 `now`).
    pub at: UnixMillis,
    /// 원격 경로 `REMOTE_DIR/이름`.
    pub remote_path: String,
    /// 올린 암호문의 바이트 수(원격이 확인해 준 크기).
    pub size: u64,
    /// 묶음의 manifest.
    pub manifest: Manifest,
    /// 보존 규칙으로 지운 백업 시각들(로컬과 원격에서 같게).
    pub removed: Vec<UnixMillis>,
}

/// 실패한 올리기. 오류와 알림 요구를 함께 낸다(E15).
#[derive(Debug)]
pub struct BackupFailure {
    /// 무엇이 잘못됐나.
    pub error: BackupError,
    /// 사용자에게 보낼 알림 요구.
    pub notice: BackupNotice,
}

/// 백업 실패 알림 요구(E15). 비밀 값이 없다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupNotice {
    /// 실패 시각.
    pub at: UnixMillis,
    /// 실패 종류.
    pub kind: FailureKind,
}

/// 마지막 실패.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastFailure {
    /// 시각.
    pub at: UnixMillis,
    /// 종류.
    pub kind: FailureKind,
}

/// 백업 상태. `local_dir`에 남아 다시 열어도 같다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BackupStatus {
    /// 마지막 성공 시각(하루 정리에 보인다, V50).
    pub last_success: Option<UnixMillis>,
    /// 마지막 실패.
    pub last_failure: Option<LastFailure>,
    /// 마지막 시도가 실패해 다음 장 마감에 다시 올려야 하는가(E15).
    pub retry_pending: bool,
}

impl Backup {
    /// 백업기를 만든다. 저장소 이름이 `소유자/이름`(두 조각 모두 비지 않음)이 아니면
    /// [`BackupError::InvalidRepo`]. 네트워크는 쓰지 않는다.
    pub fn new(
        settings: BackupSettings,
        store: Store,
        secrets: Arc<SecretStore>,
    ) -> Result<Self, BackupError> {
        todo!()
    }

    /// 한 번 올린다. `now`가 이번 백업 시각이다(파일 이름 · manifest · 상태 기록).
    ///
    /// 순서: 토큰([`pi_secrets::KeySlot::BackupGithubToken`]) 읽기 → 임시 폴더에 스냅샷 → 비밀 검사
    /// ([`SecretStore::contains_registered`]가 참이면 [`BackupError::SecretFound`], 아무것도 올리지 않는다)
    /// → 묶기 · 검증 → 암호화 → contents API로 `REMOTE_DIR/`[`crate::backup_file_name`]`(now)`에 올리기
    /// → 원격 크기 확인 → 로컬 사본 → 보존 규칙(로컬과 원격 같게, 이 이름 모양의 파일만) → 상태 기록.
    /// 임시 폴더는 성공해도 실패해도 지운다.
    ///
    /// 실패하면 상태에 실패와 "다시 올림"을 남기고 알림 요구와 함께 돌려준다. 오래된 백업은 지우지 않는다
    /// (온전한 백업 뒤에만 지운다).
    pub async fn run(&self, now: UnixMillis) -> Result<BackupReport, BackupFailure> {
        todo!()
    }

    /// 지금 상태. 기록이 없으면 [`BackupStatus::default`].
    pub fn status(&self) -> Result<BackupStatus, BackupError> {
        todo!()
    }
}
