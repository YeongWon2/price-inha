//! (61차, 결정 31) 오래된 기록을 덜어 낸다: 재기 · 요약 · 보관 파일 · 공간 돌려받기.
//!
//! 원본 동작 근거: OpenClaw 3.4 압축(오래된 대화를 요약으로 바꾸고 원문은 따로 둔다) · 3.6 기억은 "요약, 원문 기록
//! 아님"(`docs/research/openclaw.md`). 요약은 AI를 부르지 않는 고정 틀이다.
//!
//! - 재기([`storage_report`]): 저장소의 종류별 쓰임에서 하루 평균(묶음마다 처음과 마지막 시각 사이의 날로 나눈 것의
//!   합)과 10년 추정(하루 평균 × 3650)을 낸다. `price-inha doctor` · 설정 화면 진단 · [`SETUP_STORAGE_PATH`].
//! - 덜어 내기([`run_archive`]): 보관 기간([`ArchiveJob::keep_days`], 기본 [`ARCHIVE_KEEP_DAYS_DEFAULT`]일)이 지난
//!   주(한국 시각 월요일 0시부터 한 주)마다 원문을 보관 폴더 [`ARCHIVE_DIR`]`/<종류>-<기간>.jsonl.gz`(0600, gzip —
//!   62차 결정 33, 옛 무압축 `.jsonl`도 [`read_archive_file`]로 읽는다)로 빼내고, 풀어 본 원문의 줄 수 · SHA-256을
//!   확인한 뒤에만 요약 한 행을 남기고 원문 행을 지운다.

use std::path::PathBuf;
use std::sync::Arc;

use pi_knowledge::Knowledge;
use pi_store::{ArchiveCategory, ReclaimReport, Store};

mod daily;
mod file;
mod job;
mod measure;
mod summary;

pub use file::read_archive_file;

/// 보관 폴더 이름(보관 폴더 `--home` 아래).
pub const ARCHIVE_DIR: &str = "archive";
/// 보관 기간 기본값(일).
pub const ARCHIVE_KEEP_DAYS_DEFAULT: u32 = 90;
/// 쓰임 표 경로(세션, `GET`).
pub const SETUP_STORAGE_PATH: &str = "/api/setup/storage";

/// 덜어 내기 한 번에 필요한 것.
#[derive(Clone)]
pub struct ArchiveJob {
    /// 저장소.
    pub store: Store,
    /// 지식 창구(없으면 지식 조각은 건드리지 않는다).
    pub knowledge: Option<Arc<Knowledge>>,
    /// 보관 폴더(`--home`).
    pub home: PathBuf,
    /// 보관 기간(일).
    pub keep_days: u32,
}

/// 보관으로 옮긴 기간 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivedPeriod {
    /// 묶음.
    pub category: ArchiveCategory,
    /// 기간 이름(예: `2026-W07`).
    pub period: String,
    /// 옮긴 줄 수.
    pub rows: u64,
    /// 보관 파일.
    pub file: PathBuf,
}

/// 덜어 내기 한 번의 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveRun {
    /// 옮긴 기간들.
    pub periods: Vec<ArchivedPeriod>,
    /// 요약으로 바꾼 바깥 자료 기록 수.
    pub knowledge_records: usize,
    /// 공간 돌려받기(옮긴 것이 있을 때만).
    pub reclaimed: Option<ReclaimReport>,
}

/// 묶음 하나의 쓰임.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupReport {
    /// 표.
    pub table: String,
    /// 종류(이벤트 · 방 로그).
    pub kind: Option<String>,
    /// 줄 수.
    pub rows: u64,
    /// 바이트(어림).
    pub bytes: u64,
    /// 하루 평균 바이트.
    pub daily_bytes: u64,
    /// 덜어 낼 수 있는 종류인가.
    pub archivable: bool,
}

/// 쓰임 보고.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageReport {
    /// 파일 크기.
    pub file_bytes: u64,
    /// 하루 평균 바이트(묶음 합).
    pub daily_bytes: u64,
    /// 10년 추정(하루 평균 × 3650).
    pub ten_year_bytes: u64,
    /// 묶음들.
    pub groups: Vec<GroupReport>,
}

pub(crate) use daily::spawn_daily;
pub use job::run_archive;
pub(crate) use measure::{storage_detail, storage_routes};
pub use measure::{storage_line, storage_report};
