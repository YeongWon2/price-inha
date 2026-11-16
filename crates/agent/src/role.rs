//! 담당 6명과 담당별 설정(만드는 것 1번, 결정 16번).

use std::time::Duration;

/// 반복 상한의 기본값(결정 16번).
pub const DEFAULT_MAX_ITERATIONS: u32 = 20;

/// 한 번의 공급자 호출에 주는 최대 출력 토큰의 기본값.
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

/// 투자 팀의 담당. 딱 6명이다.
///
/// 방 로그 · 세션의 담당 이름과 멘션(`@<id>`)에는 [`Role::id`]를 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    /// 뉴스 공시 시세 담당. id `scout`.
    Scout,
    /// 전략 담당. id `strategy`.
    Strategy,
    /// 리스크 담당. id `risk`.
    Risk,
    /// 복기 담당. id `review`.
    Review,
    /// 매매 담당. id `trader`.
    Trader,
    /// 팀장. id `leader`.
    Leader,
}

impl Role {
    /// 6명 모두. 이 순서가 명단 순서다.
    pub const ALL: [Role; 6] = [
        Role::Scout,
        Role::Strategy,
        Role::Risk,
        Role::Review,
        Role::Trader,
        Role::Leader,
    ];

    /// 담당 id(`scout` · `strategy` · `risk` · `review` · `trader` · `leader`).
    pub fn id(self) -> &'static str {
        match self {
            Role::Scout => "scout",
            Role::Strategy => "strategy",
            Role::Risk => "risk",
            Role::Review => "review",
            Role::Trader => "trader",
            Role::Leader => "leader",
        }
    }

    /// id로 담당을 찾는다. 앞뒤 공백 · 대소문자는 보지 않는다. 모르는 id면 `None`.
    pub fn from_id(id: &str) -> Option<Role> {
        let wanted = id.trim().to_ascii_lowercase();
        Role::ALL.into_iter().find(|role| role.id() == wanted)
    }
}

/// 턴이 도는 자리. 시간 제한이 다르다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnMode {
    /// 장중 팀 논의 · 점수 · 판단 턴.
    Market,
    /// 사용자와의 채팅.
    Chat,
}

/// 턴 시간 제한(결정 16번).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeLimits {
    /// 공급자 호출 하나를 기다리는 최대 시간(무응답).
    pub idle: Duration,
    /// 한 턴 전체의 최대 시간.
    pub total: Duration,
}

impl TimeLimits {
    /// 장중 담당 턴: 무응답 60초 · 전체 180초.
    pub const MARKET: TimeLimits = TimeLimits {
        idle: Duration::from_secs(60),
        total: Duration::from_secs(180),
    };

    /// 채팅: 무응답 180초 · 전체 600초.
    pub const CHAT: TimeLimits = TimeLimits {
        idle: Duration::from_secs(180),
        total: Duration::from_secs(600),
    };

    /// 자리에 맞는 시간 제한.
    pub fn for_mode(mode: TurnMode) -> TimeLimits {
        match mode {
            TurnMode::Market => TimeLimits::MARKET,
            TurnMode::Chat => TimeLimits::CHAT,
        }
    }
}

/// 담당 한 명의 설정.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleConfig {
    /// 담당.
    pub role: Role,
    /// 공급자에 보내는 모델 이름. 그대로 보낸다.
    pub model: String,
    /// 담당 SOUL(시스템 글의 담당 층). 비어 있어도 된다.
    pub soul: String,
    /// 한 턴의 반복(공급자 호출) 상한.
    pub max_iterations: u32,
    /// 한 번의 호출에 주는 최대 출력 토큰.
    pub max_tokens: u32,
}

impl RoleConfig {
    /// 기본값으로 만든다: SOUL 빈 글, 반복 상한 [`DEFAULT_MAX_ITERATIONS`],
    /// 최대 토큰 [`DEFAULT_MAX_TOKENS`].
    pub fn new(role: Role, model: impl Into<String>) -> RoleConfig {
        RoleConfig {
            role,
            model: model.into(),
            soul: String::new(),
            max_iterations: DEFAULT_MAX_ITERATIONS,
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }
}
