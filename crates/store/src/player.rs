//! 내 캐릭터 (19차, 결정 17 · additions.md M8 · T23).
//!
//! 한 줄뿐인 표(판 11). 이름 · 외형 id · 마지막 자리 · 바뀐 시각. 설정이라 덮어쓴다(지우기는 없다).
//! 저장소 파일 안에 있으므로 백업 · 되살리기에 그대로 따라간다.
//! 외형이 원본 목록에 있는지는 부르는 쪽(server)이 본다. 저장소는 빈 글자만 거부한다.

use rusqlite::{Connection, OptionalExtension, params};

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
        self.read(load)
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
        let chars = name.chars().count();
        if chars == 0 || chars > PLAYER_NAME_MAX_CHARS {
            return Err(StoreError::InvalidPlayer(format!(
                "이름은 1~{PLAYER_NAME_MAX_CHARS}자여야 한다({chars}자)"
            )));
        }
        if look.is_empty() {
            return Err(StoreError::InvalidPlayer("외형 id가 빈 글자다".to_owned()));
        }
        self.write(|conn| {
            conn.execute(
                "INSERT INTO player_profile (id, name, look, updated_at) VALUES (1, ?1, ?2, ?3) \
                 ON CONFLICT (id) DO UPDATE SET name = excluded.name, look = excluded.look, \
                 updated_at = excluded.updated_at",
                params![name, look, at.0],
            )?;
            load(conn)
        })
    }

    /// 마지막 자리를 남긴다(덮어쓴다). 이름 · 외형은 그대로다. 바뀐 시각은 `at`이 된다.
    pub fn set_player_spot(
        &self,
        x: u32,
        y: u32,
        facing: Facing,
        at: UnixMillis,
    ) -> Result<Player, StoreError> {
        self.write(|conn| {
            conn.execute(
                "INSERT INTO player_profile (id, name, spot_x, spot_y, facing, updated_at) \
                 VALUES (1, ?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT (id) DO UPDATE SET spot_x = excluded.spot_x, \
                 spot_y = excluded.spot_y, facing = excluded.facing, \
                 updated_at = excluded.updated_at",
                params![
                    PLAYER_DEFAULT_NAME,
                    i64::from(x),
                    i64::from(y),
                    facing_code(facing),
                    at.0
                ],
            )?;
            load(conn)
        })
    }
}

// ───────────── 안에서만 쓰는 것 ─────────────

/// 줄 모양: 이름 · 외형 · x · y · 방향 · 바뀐 시각.
type PlayerRow = (
    String,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<String>,
    i64,
);

fn load(conn: &Connection) -> Result<Player, StoreError> {
    let row: Option<PlayerRow> = conn
        .query_row(
            "SELECT name, look, spot_x, spot_y, facing, updated_at FROM player_profile WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()?;
    let Some((name, look, x, y, facing, updated_at)) = row else {
        return Ok(Player {
            name: PLAYER_DEFAULT_NAME.to_owned(),
            look: None,
            spot: None,
            updated_at: None,
        });
    };
    let spot = match (x, y, facing) {
        (Some(x), Some(y), Some(facing)) => Some(PlayerSpot {
            x: coord(x)?,
            y: coord(y)?,
            facing: facing_from(&facing)?,
        }),
        _ => None,
    };
    Ok(Player {
        name,
        look,
        spot,
        updated_at: Some(UnixMillis(updated_at)),
    })
}

fn coord(raw: i64) -> Result<u32, StoreError> {
    u32::try_from(raw).map_err(|_| StoreError::Incompatible(format!("이상한 칸 좌표: {raw}")))
}

fn facing_code(facing: Facing) -> &'static str {
    match facing {
        Facing::Up => "up",
        Facing::Down => "down",
        Facing::Left => "left",
        Facing::Right => "right",
    }
}

fn facing_from(code: &str) -> Result<Facing, StoreError> {
    Facing::ALL
        .into_iter()
        .find(|f| facing_code(*f) == code)
        .ok_or_else(|| StoreError::Incompatible(format!("모르는 방향 값: {code}")))
}
