//! (65차) 새 판 알리기 — red 뼈대.

use std::path::{Path, PathBuf};

/// 보관 폴더 안 확인 기록 파일 이름.
pub const UPDATE_CHECK_FILE: &str = "update-check.json";

/// 확인 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateNotice {
    /// 이 실행 파일의 판.
    pub current: String,
    /// 확인 주소가 답한 판.
    pub latest: String,
    /// 더 새 판인가.
    pub newer: bool,
    /// 확인한 시각(유닉스 밀리초).
    pub checked_at: i64,
}

/// 확인하지 못한 까닭.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdateCheckError {
    /// https(또는 루프백 http)가 아니다.
    #[error("확인 주소는 https만 받습니다")]
    InsecureUrl,
    /// 닿지 못했다.
    #[error("확인 주소에 닿지 못했습니다")]
    Unreachable,
    /// 답이 판 글자가 아니다.
    #[error("확인 주소의 답이 판 글자가 아닙니다")]
    NotAVersion,
    /// 기록을 남기지 못했다.
    #[error("확인 기록을 남기지 못했습니다")]
    Save,
}

/// 조립 값(red 뼈대).
pub fn update_check_url() -> Option<&'static str> {
    None
}

/// 판 견주기(red 뼈대).
pub fn is_newer_version(latest: &str, current: &str) -> bool {
    let _ = (latest, current);
    false
}

/// 확인(red 뼈대).
pub async fn check_for_update(
    url: &str,
    home: &Path,
    now: i64,
) -> Result<UpdateNotice, UpdateCheckError> {
    let _ = (url, home, now);
    Err(UpdateCheckError::Unreachable)
}

/// 기록 읽기(red 뼈대).
pub fn read_update_notice(home: &Path) -> Option<UpdateNotice> {
    let _ = home;
    None
}

/// 배경 확인(red 뼈대).
pub fn spawn_update_check(
    home: PathBuf,
    url: Option<&'static str>,
    demo: bool,
) -> Option<tokio::task::JoinHandle<()>> {
    let _ = (home, url, demo);
    None
}
