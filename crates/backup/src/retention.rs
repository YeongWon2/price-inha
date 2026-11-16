//! 보존 규칙과 백업 파일 이름 (원본 동작: OpenClaw `--keep-daily 7 --keep-weekly 4 --keep-monthly 12`).

use pi_store::UnixMillis;

/// 보존 계획. `keep`과 `remove`는 겹치지 않고 합치면 입력 전부다(중복 입력은 하나로 본다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionPlan {
    /// 남길 것(새것부터).
    pub keep: Vec<UnixMillis>,
    /// 지울 것(새것부터).
    pub remove: Vec<UnixMillis>,
}

/// 백업 시각들에 보존 규칙을 적용한다.
///
/// 새것부터 보며, 백업이 있는 최근 [`crate::KEEP_DAILY`]일 · [`crate::KEEP_WEEKLY`]주(ISO, 월요일 시작) ·
/// [`crate::KEEP_MONTHLY`]달마다 그 칸의 가장 새것 하나를 남긴다(세 규칙의 합집합). 날 · 주 · 달의
/// 경계는 한국 시각(UTC+9)으로 센다. 나머지는 지운다.
pub fn plan_retention(times: &[UnixMillis]) -> RetentionPlan {
    todo!()
}

/// 그 시각의 백업 파일 이름. `price-inha-backup-YYYYMMDDTHHMMSSmmmZ.age`(UTC)이고 이름 순서가 시각 순서다.
pub fn backup_file_name(at: UnixMillis) -> String {
    todo!()
}

/// [`backup_file_name`]이 만든 이름에서 시각을 읽는다. 그 모양이 아니면 `None`.
pub fn parse_backup_file_name(name: &str) -> Option<UnixMillis> {
    todo!()
}
