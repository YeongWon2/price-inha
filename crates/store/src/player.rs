//! 내 캐릭터 (19차, 결정 17 · additions.md M8 · T23).
//!
//! 한 줄뿐인 표(판 11). 이름 · 외형 id · 마지막 자리 · 바뀐 시각. 설정이라 덮어쓴다(지우기는 없다).
//! 저장소 파일 안에 있으므로 백업 · 되살리기에 그대로 따라간다.
//! 외형이 원본 목록에 있는지는 부르는 쪽(server)이 본다. 저장소는 빈 글자만 거부한다.

use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 내 캐릭터 이름의 가장 긴 길이(글자 수). 원본 DeskRPG 캐릭터 이름 상한과 같다.
pub const PLAYER_NAME_MAX_CHARS: usize = 50;

/// 한 번도 정하지 않았을 때의 이름.
pub const PLAYER_DEFAULT_NAME: &str = "나";

/// 바라보는 방향.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Facing {
    /// 위
    Up,
    /// 아래
    Down,
    /// 왼쪽
    Left,
    /// 오른쪽
    Right,
}

impl Facing {
    /// 모든 방향.
    pub const ALL: [Facing; 4] = [Facing::Up, Facing::Down, Facing::Left, Facing::Right];
}

/// 마지막 자리: 칸 좌표와 바라보는 방향.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerSpot {
    /// 칸 x.
    pub x: u32,
    /// 칸 y.
    pub y: u32,
    /// 바라보는 방향.
    pub facing: Facing,
}

/// 내 캐릭터.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    /// 이름(1~[`PLAYER_NAME_MAX_CHARS`]자). 정한 적이 없으면 [`PLAYER_DEFAULT_NAME`].
    pub name: String,
    /// 외형 id. 정한 적이 없으면 `None`.
    pub look: Option<String>,
    /// 마지막 자리. 남긴 적이 없으면 `None`.
    pub spot: Option<PlayerSpot>,
    /// 마지막으로 바뀐 시각. 바뀐 적이 없으면 `None`.
    pub updated_at: Option<UnixMillis>,
}

impl Store {
    /// 내 캐릭터. 새 저장소(와 판 10 이하에서 올라온 저장소)에서는 이름 [`PLAYER_DEFAULT_NAME`],
    /// 외형 · 자리 · 바뀐 시각은 `None`이다.
    pub fn player(&self) -> Result<Player, StoreError> {
        let _ = self;
        Err(StoreError::Incompatible("player 미구현".to_owned()))
    }

    /// 이름과 외형을 정한다(덮어쓴다). 자리는 그대로다. 바뀐 시각은 `at`이 된다.
    ///
    /// 이름이 0자이거나 [`PLAYER_NAME_MAX_CHARS`]자보다 길면, 외형이 빈 글자면
    /// [`StoreError::InvalidPlayer`]이고 아무것도 바꾸지 않는다.
    pub fn set_player_profile(
        &self,
        name: &str,
        look: &str,
        at: UnixMillis,
    ) -> Result<Player, StoreError> {
        let _ = (self, name, look, at);
        Err(StoreError::Incompatible(
            "set_player_profile 미구현".to_owned(),
        ))
    }

    /// 마지막 자리를 남긴다(덮어쓴다). 이름 · 외형은 그대로다. 바뀐 시각은 `at`이 된다.
    pub fn set_player_spot(
        &self,
        x: u32,
        y: u32,
        facing: Facing,
        at: UnixMillis,
    ) -> Result<Player, StoreError> {
        let _ = (self, x, y, facing, at);
        Err(StoreError::Incompatible(
            "set_player_spot 미구현".to_owned(),
        ))
    }
}
