//! (기능 L · 결정 30) 팀 지침 장부 파일: 보관 폴더의 [`TEAM_GUIDELINES_FILE`] 하나(주인만 읽고 쓰기 0600, 임시 파일에
//! 쓰고 이름 바꾸기). 저장소 판을 올리지 않는다 — 담당 모델 설정(`team-models.json`, `setup_models.rs`)과 같은 방식이다.
//! 모양은 `pi_journal::GuidelineBook`의 JSON이다. 파일이 깨졌으면 빈 장부로 돌고 경고를 남긴다(팀은 멈추지 않는다).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use pi_journal::GuidelineBook;

/// 장부 파일 이름(보관 폴더 안).
pub const TEAM_GUIDELINES_FILE: &str = "team-guidelines.json";

/// 장부 파일 손잡이. 읽고 바꾸고 쓰기를 한 줄로 세운다.
pub(crate) struct GuideBook {
    path: PathBuf,
    turn: Mutex<()>,
}

impl GuideBook {
    /// 보관 폴더의 장부 파일을 가리킨다(아직 읽지 않는다).
    pub(crate) fn open(home: &Path) -> GuideBook {
        GuideBook {
            path: home.join(TEAM_GUIDELINES_FILE),
            turn: Mutex::new(()),
        }
    }

    /// 지금 장부. 없으면 빈 장부.
    pub(crate) fn read(&self) -> GuidelineBook {
        let _turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        load(&self.path)
    }

    /// 장부를 읽어 `change`로 바꾸고, 바뀌었으면 쓴다(쓰지 못하면 경고만 남긴다).
    pub(crate) fn update<R>(&self, change: impl FnOnce(&mut GuidelineBook) -> R) -> R {
        let _turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        let mut book = load(&self.path);
        let before = book.clone();
        let out = change(&mut book);
        if book != before
            && let Err(error) = save(&self.path, &book)
        {
            tracing::warn!(%error, "팀 지침 장부를 저장하지 못했습니다");
        }
        out
    }
}

/// 파일을 읽는다. 없으면 빈 장부, 깨졌으면 빈 장부와 경고.
fn load(path: &Path) -> GuidelineBook {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            tracing::warn!("팀 지침 장부를 읽을 수 없어 빈 장부로 봅니다");
            GuidelineBook::default()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => GuidelineBook::default(),
        Err(error) => {
            tracing::warn!(%error, "팀 지침 장부를 열 수 없어 빈 장부로 봅니다");
            GuidelineBook::default()
        }
    }
}

/// 임시 파일(0600)에 쓰고 디스크에 내린 뒤 이름을 바꾼다.
fn save(path: &Path, book: &GuidelineBook) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(book).map_err(std::io::Error::other)?;
    let temp = path.with_extension("json.tmp");
    let _ = std::fs::remove_file(&temp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options.open(&temp).and_then(|mut file| {
        file.write_all(&bytes)?;
        file.sync_all()
    });
    let moved = written.and_then(|()| std::fs::rename(&temp, path));
    if moved.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    moved?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
