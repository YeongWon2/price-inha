//! 담당 6명과 담당별 설정(만드는 것 1번, 결정 16번).

use std::time::Duration;

use pi_agent::{DEFAULT_MAX_ITERATIONS, Error, Role, RoleConfig, Team, TimeLimits, TurnMode};

use crate::common::{Env, model};

// 검증: M2 (담당 6명 · id)
#[test]
fn 담당은_여섯이고_id가_정해져_있다() {
    let ids: Vec<&str> = Role::ALL.iter().map(|r| r.id()).collect();
    assert_eq!(
        ids,
        ["scout", "strategy", "risk", "review", "trader", "leader"]
    );
    for role in Role::ALL {
        assert_eq!(Role::from_id(role.id()), Some(role));
    }
    assert_eq!(Role::from_id(" Strategy "), Some(Role::Strategy));
    assert_eq!(Role::from_id("LEADER"), Some(Role::Leader));
    assert_eq!(Role::from_id("ceo"), None);
    assert_eq!(Role::from_id(""), None);
}

// 검증: 결정 16 (반복 상한 20)
#[test]
fn 담당_설정의_기본값은_반복_상한_20이다() {
    let config = RoleConfig::new(Role::Risk, "m-risk");
    assert_eq!(config.role, Role::Risk);
    assert_eq!(config.model, "m-risk");
    assert_eq!(config.soul, "");
    assert_eq!(config.max_iterations, 20);
    assert_eq!(DEFAULT_MAX_ITERATIONS, 20);
    assert!(config.max_tokens > 0);
}

// 검증: E6 결정 16 (장중 60 · 180초, 채팅 180 · 600초)
#[test]
fn 시간_제한은_장중과_채팅이_다르다() {
    assert_eq!(TimeLimits::MARKET.idle, Duration::from_secs(60));
    assert_eq!(TimeLimits::MARKET.total, Duration::from_secs(180));
    assert_eq!(TimeLimits::CHAT.idle, Duration::from_secs(180));
    assert_eq!(TimeLimits::CHAT.total, Duration::from_secs(600));
    assert_eq!(TimeLimits::for_mode(TurnMode::Market), TimeLimits::MARKET);
    assert_eq!(TimeLimits::for_mode(TurnMode::Chat), TimeLimits::CHAT);
}

// 검증: M2 (팀은 6명을 한 번씩)
#[test]
fn 팀은_여섯_담당을_한_번씩_담아야_한다() {
    let env = Env::new();
    let all: Vec<RoleConfig> = Role::ALL
        .iter()
        .map(|r| RoleConfig::new(*r, model(*r)))
        .collect();
    let mut five = all.clone();
    five.pop();
    let mut doubled = five.clone();
    doubled.push(RoleConfig::new(Role::Scout, "again"));
    for roles in [Vec::new(), five, doubled] {
        let result = Team::new(env.ports(), roles, TimeLimits::MARKET);
        assert!(matches!(result, Err(Error::InvalidRoles)));
    }
    let mut reversed = all;
    reversed.reverse();
    assert!(Team::new(env.ports(), reversed, TimeLimits::MARKET).is_ok());
}
