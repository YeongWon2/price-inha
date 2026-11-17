//! (8차) 바깥 키 칸(`pi_secrets::KeySlot`: DART · ECOS · FRED · 백업 GitHub 토큰)의 등록 이름과 상태.

/// 키 등록의 `item`: DART 키(`KeySlot::Dart`).
pub const KEY_DART: &str = "dart";
/// 키 등록의 `item`: ECOS 키(`KeySlot::Ecos`).
pub const KEY_ECOS: &str = "ecos";
/// 키 등록의 `item`: FRED 키(`KeySlot::Fred`).
pub const KEY_FRED: &str = "fred";
/// 키 등록의 `item`: 백업 GitHub 토큰(`KeySlot::BackupGithubToken`).
pub const KEY_BACKUP_GITHUB_TOKEN: &str = "backup_github_token";

/// 칸별 등록 여부를 읽는 경로(`GET`, 세션).
pub const SETUP_SLOTS_PATH: &str = "/api/setup/slots";
