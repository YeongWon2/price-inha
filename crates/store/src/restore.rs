//! 스냅샷과 되살리기 표시(11차 2 · 3번, V51).

use std::path::Path;

use crate::approval::ApprovalId;
use crate::model::{TeamStateRecord, UnixMillis};
use crate::{Store, StoreError};

/// [`Store::mark_restored`]가 한 일.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOutcome {
    /// 팀을 전체 정지로 바꾸며 남긴 기록. 이미 전체 정지였으면 `None`(기록을 더하지 않는다).
    pub team_record: Option<TeamStateRecord>,
    /// 만료로 닫은 승인 번호들(번호 순서).
    pub expired_approvals: Vec<ApprovalId>,
}

impl Store {
    /// 지금 저장소를 SQLite 백업 API로 `path` 한 파일에 뜬다.
    ///
    /// - 이 호출 전에 끝난 쓰기는 모두 들어 있다(WAL에만 있던 것도). 결과는 딸린 파일(`-wal` · `-shm`) 없이
    ///   그 파일 하나이고, [`Store::open`]으로 열면 같은 자료 · 같은 스키마 버전이다.
    /// - Unix에서는 만든 파일의 권한이 0600이다.
    /// - `path`에 이미 파일이 있으면 [`StoreError::SnapshotTargetExists`]이고 그 파일은 그대로다.
    /// - 쓸 수 없는 곳(없는 폴더 등)이면 오류이고 `path`에 파일이 남지 않는다.
    /// - 원본 저장소는 바뀌지 않는다.
    pub fn snapshot_to(&self, path: impl AsRef<Path>) -> Result<(), StoreError> {
        let _ = path.as_ref();
        todo!("11차 구현")
    }

    /// 백업을 새 폴더에 되살렸음을 표시한다(V51). 같은 묶음 안에서:
    ///
    /// - 팀 상태를 전체 정지로 한다. 기록 한 줄이 남는다(`at`, 사유 [`crate::TeamCause::Restored`],
    ///   `owner_approved`는 거짓). 이미 전체 정지이면 기록을 더하지 않는다. 전체 정지가 되면 실전중
    ///   템플릿이 멈춤이 되는 규칙([`Store::change_team_state`])도 똑같이 적용된다.
    /// - 기다림인 승인 요청을 모두 [`crate::ApprovalStatus::Expired`]로 닫는다(`resolved_at = at`).
    ///   이미 답이 난 승인(승인 · 거절 · 답 없음 · 만료)은 건드리지 않는다.
    /// - 전체 정지에서 나가려면 그 뒤에도 `owner_approved`가 참인 변경이 있어야 한다(V52).
    pub fn mark_restored(&self, at: UnixMillis) -> Result<RestoreOutcome, StoreError> {
        let _ = at;
        todo!("11차 구현")
    }
}
