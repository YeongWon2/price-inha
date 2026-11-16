//! 스냅샷과 되살리기 표시(11차 2 · 3번, V51).

use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;
use rusqlite::backup::{Backup, StepResult};

use crate::approval::ApprovalId;
use crate::model::{TeamStateRecord, UnixMillis};
use crate::{Store, StoreError, approval, team};

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
    /// - (후보 일괄 처리) 뜨는 동안 이 손잡이의 연결 잠금을 쥐지 않는다. 같은 손잡이(복제 포함)의 쓰기는
    ///   스냅샷이 끝나기를 기다리지 않고, 그 쓰기는 스냅샷에 들어가지 않을 수 있다.
    pub fn snapshot_to(&self, path: impl AsRef<Path>) -> Result<(), StoreError> {
        let path = path.as_ref();
        let file = create_private(path)?;
        drop(file);
        // 이 손잡이의 연결 잠금을 쥐지 않는다: 따로 연 연결의 읽기 묶음 하나(WAL)에서 한 번에 뜬다.
        // 그동안 이 손잡이의 쓰기는 기다리지 않고, 뜬 파일에는 한 시점의 자료만 들어간다.
        let _ = &self.path;
        let done = {
            let conn = self.lock();
            copy_to(&conn, path)
        };
        if done.is_err() {
            // 우리가 만든 파일이다. 반쯤 쓴 것을 남기지 않는다.
            let _ = fs::remove_file(path);
        }
        done
    }

    /// 백업을 새 폴더에 되살렸음을 표시한다(V51). 같은 묶음 안에서:
    ///
    /// - 팀 상태를 전체 정지로 한다. 기록 한 줄이 남는다(`at`, 사유 [`crate::TeamCause::Restored`],
    ///   `owner_approved`는 거짓). 이미 전체 정지이면 기록을 더하지 않는다. 전체 정지가 되면 실전중
    ///   템플릿이 멈춤이 되는 규칙([`Store::change_team_state`])도 똑같이 적용된다.
    /// - 기다림인 승인 요청을 모두 [`crate::ApprovalStatus::Expired`]로 닫는다(`resolved_at = at`).
    ///   이미 답이 난 승인(승인 · 거절 · 답 없음 · 만료)은 건드리지 않는다.
    /// - 팀 상태와 상관없이 되살림 시각을 표(`restore_marks`)에 남긴다. 그 시각과 같거나 앞서 만든 승인은
    ///   승인됨이어도 [`Store::consume_approval`]이 [`StoreError::ApprovalExpired`]로 거부한다(M5 "되살리면 전부 만료").
    /// - 전체 정지에서 나가려면 그 뒤에도 `owner_approved`가 참인 변경이 있어야 한다(V52).
    pub fn mark_restored(&self, at: UnixMillis) -> Result<RestoreOutcome, StoreError> {
        self.write(|conn| {
            conn.execute("INSERT INTO restore_marks (at) VALUES (?1)", [at.0])?;
            let team_record = team::restore_full_stop(conn, at)?;
            let expired_approvals = approval::expire_pending(conn, at)?;
            Ok(RestoreOutcome {
                team_record,
                expired_approvals,
            })
        })
    }
}

/// 스냅샷 파일을 새로 만든다(이미 있으면 거부). Unix에서는 처음부터 0600이다.
fn create_private(path: &Path) -> Result<fs::File, StoreError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|err| match err.kind() {
        ErrorKind::AlreadyExists => StoreError::SnapshotTargetExists(path.to_path_buf()),
        _ => StoreError::FileAccess {
            path: path.to_path_buf(),
            source: err,
        },
    })
}

/// 백업 API로 `path`에 복사하고, 딸린 파일이 남지 않게 저널을 DELETE로 바꿔 닫는다.
fn copy_to(source: &Connection, path: &Path) -> Result<(), StoreError> {
    let mut target = Connection::open(path)?;
    {
        let backup = Backup::new(source, &mut target)?;
        // 모든 쪽을 한 단계(원본의 읽기 묶음 하나)로 뜬다: 중간에 다른 연결이 써도 다시 시작하지 않는다.
        loop {
            match backup.step(-1)? {
                StepResult::Done => break,
                StepResult::More => {}
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }
    target.pragma_update(None, "journal_mode", "DELETE")?;
    target.close().map_err(|(_, err)| StoreError::Sqlite(err))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::approval::{ApprovalStatus, ApprovalUse, NewApprovalRequest};
    use crate::model::UnixMillis;
    use crate::{Store, StoreError};

    fn approved(store: &Store, created: i64) -> ApprovalUse {
        let made = store
            .create_approval(NewApprovalRequest {
                fingerprint: "sha256:x".to_string(),
                summary: json!({}),
                created_at: UnixMillis(created),
                expires_at: UnixMillis(1_000_000),
            })
            .unwrap();
        store
            .resolve_approval(made.id, ApprovalStatus::Approved, UnixMillis(created))
            .unwrap();
        ApprovalUse {
            id: made.id,
            fingerprint: "sha256:x".to_string(),
        }
    }

    #[test]
    fn 되살리기_전에_만든_승인은_이미_전체_정지여도_쓸_수_없다() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("s.sqlite")).unwrap();
        let old = approved(&store, 5);
        let same = approved(&store, 10);
        store.mark_restored(UnixMillis(10)).unwrap();
        // 이미 전체 정지인 두 번째 표시도 기준점이 남는다.
        let late = approved(&store, 20);
        store.mark_restored(UnixMillis(30)).unwrap();
        for denied in [old, same, late] {
            let err = store.consume_approval(denied, UnixMillis(40)).unwrap_err();
            assert!(matches!(err, StoreError::ApprovalExpired { .. }), "{err:?}");
        }
        let fresh = approved(&store, 31);
        store.consume_approval(fresh, UnixMillis(40)).unwrap();
    }
}
