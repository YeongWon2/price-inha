//! (39차) 페어링 장부 · 받은 자리 파일. 홈 폴더에 0600 · 원자적 쓰기로 둔다(저장소 판을 올리지 않는다).
//! 장부는 `pi_channel::PairingBook`을 구현한다. 토큰은 여기 두지 않는다.

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use pi_channel::{
    BookError, ChatId, Cursor, IssueRecord, PairingBook, PairingCode, PairingState, PendingRequest,
    SenderId,
};
use serde_json::{Value, json};

/// 페어링 장부 파일(허용된 사람 · 기다리는 요청 · 코드를 준 기록 · 틀린 횟수 · 잠금).
pub const MESSENGER_BOOK_FILE: &str = "messenger-pairing.json";
/// 받은 자리(`pi_channel::Cursor`) 파일. 다시 켜도 이어 받는다.
pub const MESSENGER_CURSOR_FILE: &str = "messenger-cursor.json";

/// 홈 폴더의 장부 파일로 만든 [`PairingBook`].
#[derive(Debug)]
pub(crate) struct FileBook {
    path: PathBuf,
}

impl FileBook {
    pub(crate) fn new(home: &Path) -> FileBook {
        FileBook {
            path: home.join(MESSENGER_BOOK_FILE),
        }
    }
}

impl PairingBook for FileBook {
    fn load(&self) -> Result<PairingState, BookError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => {
                let value: Value = serde_json::from_slice(&bytes)
                    .map_err(|_| BookError("장부 파일을 읽을 수 없다".to_string()))?;
                state_of(&value).ok_or_else(|| BookError("장부 파일의 모양이 다르다".to_string()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(PairingState::default())
            }
            Err(error) => Err(BookError(error.kind().to_string())),
        }
    }

    fn save(&self, state: &PairingState) -> Result<(), BookError> {
        let bytes = state_json(state).to_string();
        write_private(&self.path, bytes.as_bytes()).map_err(|e| BookError(e.kind().to_string()))
    }
}

fn ms(at: DateTime<Utc>) -> i64 {
    at.timestamp_millis()
}

fn at(value: &Value) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(value.as_i64()?).single()
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

fn state_json(state: &PairingState) -> Value {
    let pending: Vec<Value> = state
        .pending
        .iter()
        .map(|p| {
            json!({
                "code": p.code.as_str(),
                "sender": p.sender.0,
                "chat": p.chat.0,
                "created_at": ms(p.created_at),
            })
        })
        .collect();
    let issued: Vec<Value> = state
        .issued
        .iter()
        .map(|r| json!({ "sender": r.sender.0, "at": ms(r.at) }))
        .collect();
    json!({
        "allowed": state.allowed.iter().map(|s| s.0.clone()).collect::<Vec<_>>(),
        "pending": pending,
        "issued": issued,
        "failed_approvals": state.failed_approvals,
        "locked_until": state.locked_until.map(ms),
    })
}

fn state_of(value: &Value) -> Option<PairingState> {
    let allowed = value["allowed"]
        .as_array()?
        .iter()
        .map(|s| text(s).map(SenderId))
        .collect::<Option<Vec<_>>>()?;
    let pending = value["pending"]
        .as_array()?
        .iter()
        .map(|p| {
            Some(PendingRequest {
                code: PairingCode::new(text(&p["code"])?),
                sender: SenderId(text(&p["sender"])?),
                chat: ChatId(text(&p["chat"])?),
                created_at: at(&p["created_at"])?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let issued = value["issued"]
        .as_array()?
        .iter()
        .map(|r| {
            Some(IssueRecord {
                sender: SenderId(text(&r["sender"])?),
                at: at(&r["at"])?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let failed_approvals = u32::try_from(value["failed_approvals"].as_u64()?).ok()?;
    let locked_until = match &value["locked_until"] {
        Value::Null => None,
        other => Some(at(other)?),
    };
    Some(PairingState {
        allowed,
        pending,
        issued,
        failed_approvals,
        locked_until,
    })
}

/// 받은 자리를 읽는다. 없거나 읽을 수 없으면 `None`(처음부터 받는다).
pub(crate) fn load_cursor(home: &Path) -> Option<Cursor> {
    let bytes = std::fs::read(home.join(MESSENGER_CURSOR_FILE)).ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    value["offset"].as_i64().map(Cursor)
}

/// 받은 자리를 쓴다(0600 · 원자적).
pub(crate) fn save_cursor(home: &Path, cursor: Cursor) -> std::io::Result<()> {
    let bytes = json!({ "offset": cursor.0 }).to_string();
    write_private(&home.join(MESSENGER_CURSOR_FILE), bytes.as_bytes())
}

/// 장부 · 받은 자리 파일을 지운다(없으면 그대로).
pub(crate) fn remove_files(home: &Path) -> std::io::Result<()> {
    for name in [MESSENGER_BOOK_FILE, MESSENGER_CURSOR_FILE] {
        match std::fs::remove_file(home.join(name)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    Ok(())
}

/// 같은 폴더의 임시 파일(0600)에 쓰고 디스크에 내린 뒤 이름을 바꾼다.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("messenger");
    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options.open(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(error) = written.and_then(|()| std::fs::rename(&temp, path)) {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 장부는_쓴_그대로_다시_읽힌다() {
        let dir = tempfile::tempdir().unwrap();
        let book = FileBook::new(dir.path());
        assert_eq!(book.load().unwrap(), PairingState::default());
        let now = Utc.timestamp_millis_opt(1_790_000_000_000).unwrap();
        let state = PairingState {
            allowed: vec![SenderId("11".into())],
            pending: vec![PendingRequest {
                code: PairingCode::new("ABCD2345".into()),
                sender: SenderId("22".into()),
                chat: ChatId("22".into()),
                created_at: now,
            }],
            issued: vec![IssueRecord {
                sender: SenderId("22".into()),
                at: now,
            }],
            failed_approvals: 2,
            locked_until: Some(now),
        };
        book.save(&state).unwrap();
        assert_eq!(book.load().unwrap(), state);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(MESSENGER_BOOK_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn 받은_자리는_이어_읽히고_지우면_없다() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_cursor(dir.path()), None);
        save_cursor(dir.path(), Cursor(42)).unwrap();
        assert_eq!(load_cursor(dir.path()), Some(Cursor(42)));
        remove_files(dir.path()).unwrap();
        assert_eq!(load_cursor(dir.path()), None);
        remove_files(dir.path()).unwrap();
    }
}
