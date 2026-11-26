//! (기능 L · 결정 30) 팀 지침 장부 파일: 보관 폴더의 [`TEAM_GUIDELINES_FILE`] 하나(주인만 읽고 쓰기 0600, 임시 파일에
//! 쓰고 이름 바꾸기). 저장소 판을 올리지 않는다 — 담당 모델 설정(`team-models.json`, `setup_models.rs`)과 같은 방식이다.
//! 모양은 `pi_journal::GuidelineBook`의 JSON이다. 파일이 깨졌으면 빈 장부로 돌고 경고를 남긴다(팀은 멈추지 않는다).

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
        GuidelineBook::default()
    }

    /// 장부를 읽어 `change`로 바꾸고 쓴다.
    pub(crate) fn update<R>(&self, change: impl FnOnce(&mut GuidelineBook) -> R) -> R {
        let _turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        let mut book = GuidelineBook::default();
        change(&mut book)
    }
}
