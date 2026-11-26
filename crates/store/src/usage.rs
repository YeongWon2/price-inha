//! (21차, 결정 31) 저장소 쓰임 재기와 공간 돌려받기.
//!
//! - 재기: 표마다(이벤트 · 방 로그는 종류마다) 줄 수 · 바이트 · 처음과 마지막 시각. 바이트는 칸 길이의 합으로
//!   어림한다(묶음 SQLite에 `dbstat`이 없다). 파일 크기는 `page_count × page_size`.
//! - 공간 돌려받기: 덜어 낸 뒤 빈 쪽을 파일에서 돌려준다(incremental vacuum).

use std::path::Path;

use crate::{Store, StoreError};

/// 한 묶음(표, 또는 표 안의 종류)의 쓰임.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageGroup {
    /// 표 이름.
    pub table: String,
    /// 종류(이벤트 · 방 로그만). 나머지 표는 `None`.
    pub kind: Option<String>,
    /// 줄 수.
    pub rows: u64,
    /// 칸 길이의 합(바이트, 어림).
    pub bytes: u64,
    /// 가장 이른 시각(시각 칸이 있는 표만).
    pub first_at: Option<i64>,
    /// 가장 늦은 시각.
    pub last_at: Option<i64>,
}

/// 저장소 쓰임.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageUsage {
    /// 쪽 크기.
    pub page_size: u64,
    /// 쪽 수.
    pub page_count: u64,
    /// 빈 쪽 수.
    pub freelist_count: u64,
    /// 묶음들(바이트 큰 것부터).
    pub groups: Vec<UsageGroup>,
}

/// 공간 돌려받기 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReclaimReport {
    /// 돌려받기 전 빈 쪽 수.
    pub free_before: u64,
    /// 돌려받은 뒤 빈 쪽 수.
    pub free_after: u64,
    /// 이번에 자동 정리 방식을 바꾸느라 한 번 전체 정리(VACUUM)를 했는가.
    pub converted: bool,
}

impl StorageUsage {
    /// 파일 크기(`page_count × page_size`).
    pub fn file_bytes(&self) -> u64 {
        todo!()
    }
}

/// 저장소 파일을 읽기 전용으로 열어 잰다(판을 올리지 않는다). 파일이 없으면 `None`.
pub fn measure_file(path: &Path) -> Result<Option<StorageUsage>, StoreError> {
    let _ = path;
    todo!()
}

impl Store {
    /// 열린 저장소의 쓰임.
    pub fn storage_usage(&self) -> Result<StorageUsage, StoreError> {
        todo!()
    }

    /// 빈 쪽을 많아야 `max_pages`개 파일에서 돌려준다. 자동 정리 방식이 incremental이 아니면 한 번 바꾼다.
    pub fn reclaim_space(&self, max_pages: u32) -> Result<ReclaimReport, StoreError> {
        let _ = max_pages;
        todo!()
    }
}
