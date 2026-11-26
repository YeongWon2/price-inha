//! pi-backup: 백업과 되살리기 (구성 요소 A14, T19). `docs/dev/modules/backup.md`를 따른다.
//!
//! 매일 장 마감 뒤(일정은 `pi-trigger`가 정한다) 저장소를 SQLite 백업 API로 임시 폴더에 뜨고,
//! manifest(판 번호 · 파일 · SHA-256)와 함께 묶어 age 수신자 공개키로 암호화한 뒤 비공개 GitHub
//! 저장소에 contents API로 올린다(D7 · C13). 비밀키는 백업에 들어가지 않는다(E39 · N8 · P22).
//! 되살리기는 빈 폴더에만, 쓰기 전에 검증하고, 끝나면 팀을 멈추고 기다리던 승인을 만료한다(V51 · F9).
//!
//! # 모양
//!
//! - [`Backup::run`]: 한 번 올리기. 순서는 토큰 읽기 → 임시 폴더에 스냅샷 → 비밀 검사 → 묶기 · 검증 →
//!   암호화 → 올리기 → 크기 확인 → 로컬 사본 → 보존 규칙 → 임시 폴더 지우기다.
//! - [`restore`]: 암호문 · 비밀키 identity · 대상 폴더를 받아 되살린다.
//! - [`seal`] · [`open`] · [`verify`]: 묶음을 암호화 · 풀기 · 검증한다.
//! - [`plan_retention`]: 보존 규칙(일 7 · 주 4 · 월 12).
//!
//! 키 파일 이름은 직접 쓰지 않는다. 스냅샷만 담으므로 키 파일은 처음부터 대상이 아니다
//! (`pi_secrets::BACKUP_EXCLUDED_FILES`, `ARCH-SECRET-FILES`).

mod archive;
mod bundle;
mod error;
mod remote;
mod restore;
mod retention;
mod run;
mod sqlite;
mod status;
mod token;

pub use bundle::{Bundle, BundleFile, Manifest, ManifestFile, open, seal, sha256_hex, verify};
pub use error::{BackupError, BundleProblem, FailureKind};
pub use restore::{RestoreReport, restore, restore_in};
pub use retention::{RetentionPlan, backup_file_name, parse_backup_file_name, plan_retention};
pub use run::{
    Backup, BackupFailure, BackupNotice, BackupReport, BackupSettings, BackupStatus, LastFailure,
};
pub use token::verify_token;

/// manifest 판 번호. [`verify`]는 이것과 다른 판을 거부한다.
pub const MANIFEST_VERSION: u32 = 1;

/// 묶음 안 스냅샷 파일 이름. 되살린 폴더에도 이 이름으로 쓴다(서버의 DB 파일 이름과 같다).
pub const SNAPSHOT_FILE: &str = "price-inha.sqlite";

/// (결정 31) 보관 파일(보관 폴더 `archive/`의 `*.jsonl`)의 묶음 안 이름 앞붙이. 되살리면 `archive/` 아래로 돌아간다.
pub const ARCHIVE_BUNDLE_PREFIX: &str = "archive--";

/// 원격 저장소 안에서 백업 파일을 두는 폴더.
pub const REMOTE_DIR: &str = "backups";

/// 백업 파일 이름의 끝.
pub const FILE_SUFFIX: &str = ".age";

/// 보존: 최근 백업이 있는 날 수(그날의 가장 새것 하나씩).
pub const KEEP_DAILY: usize = 7;

/// 보존: 최근 백업이 있는 주 수(ISO 주, 월요일 시작. 그 주의 가장 새것 하나씩).
pub const KEEP_WEEKLY: usize = 4;

/// 보존: 최근 백업이 있는 달 수(그달의 가장 새것 하나씩).
pub const KEEP_MONTHLY: usize = 12;
