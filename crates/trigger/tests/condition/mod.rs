//! 시세 조건과 다시 걸기(결정 21).

use pi_trigger::{
    ConditionKind, ConditionMemory, Direction, Observation, default_rearm_percent, observe,
};

use crate::support::d;

fn band() -> rust_decimal::Decimal {
    d("0.5")
}

// 검증: F2
#[test]
fn 넘음은_기준값에_닿으면_맞는다() {
    let kind = ConditionKind::Above(d("100"));
    let armed = ConditionMemory::Armed;
    assert_eq!(
        observe(&kind, &armed, d("99.99"), band()),
        Observation::Nothing
    );
    assert_eq!(
        observe(&kind, &armed, d("100"), band()),
        Observation::Fire(Direction::Up)
    );
    assert_eq!(
        observe(&kind, &armed, d("120"), band()),
        Observation::Fire(Direction::Up)
    );
}

// 검증: F2
#[test]
fn 내려감은_기준값에_닿으면_맞는다() {
    let kind = ConditionKind::Below(d("100"));
    let armed = ConditionMemory::Armed;
    assert_eq!(
        observe(&kind, &armed, d("100.01"), band()),
        Observation::Nothing
    );
    assert_eq!(
        observe(&kind, &armed, d("100"), band()),
        Observation::Fire(Direction::Down)
    );
}

// 검증: F2
#[test]
fn 변동률은_기준가에서_오르든_내리든_그_비율에_닿으면_맞는다() {
    let kind = ConditionKind::Change {
        reference: d("100"),
        percent: d("3"),
    };
    let armed = ConditionMemory::Armed;
    assert_eq!(
        observe(&kind, &armed, d("102.99"), band()),
        Observation::Nothing
    );
    assert_eq!(
        observe(&kind, &armed, d("97.01"), band()),
        Observation::Nothing
    );
    assert_eq!(
        observe(&kind, &armed, d("103"), band()),
        Observation::Fire(Direction::Up)
    );
    assert_eq!(
        observe(&kind, &armed, d("97"), band()),
        Observation::Fire(Direction::Down)
    );
}

// 검증: F2
#[test]
fn 알린_뒤에는_계속_맞아도_다시_알리지_않는다() {
    let kind = ConditionKind::Above(d("100"));
    let fired = ConditionMemory::Fired {
        notified: d("100"),
        direction: Direction::Up,
    };
    assert_eq!(
        observe(&kind, &fired, d("100"), band()),
        Observation::Nothing
    );
    assert_eq!(
        observe(&kind, &fired, d("105"), band()),
        Observation::Nothing
    );
}

// 검증: F2
#[test]
fn 넘음은_알린_값에서_0_5퍼센트_내려가야_다시_걸린다() {
    let kind = ConditionKind::Above(d("100"));
    let fired = ConditionMemory::Fired {
        notified: d("100"),
        direction: Direction::Up,
    };
    // 0.4% 내려감: 아직 아니다.
    assert_eq!(
        observe(&kind, &fired, d("99.6"), band()),
        Observation::Nothing
    );
    // 정확히 0.5%: 다시 건다(경계 포함). 이번에는 알리지 않는다.
    assert_eq!(
        observe(&kind, &fired, d("99.5"), band()),
        Observation::Rearm
    );
    assert_eq!(observe(&kind, &fired, d("90"), band()), Observation::Rearm);
}

// 검증: F2
#[test]
fn 내려감은_알린_값에서_0_5퍼센트_올라가야_다시_걸린다() {
    let kind = ConditionKind::Below(d("200"));
    let fired = ConditionMemory::Fired {
        notified: d("200"),
        direction: Direction::Down,
    };
    assert_eq!(
        observe(&kind, &fired, d("200.9"), band()),
        Observation::Nothing
    );
    assert_eq!(observe(&kind, &fired, d("201"), band()), Observation::Rearm);
}

// 검증: F2
#[test]
fn 변동률은_알린_방향의_반대로_벗어나야_다시_걸린다() {
    let kind = ConditionKind::Change {
        reference: d("100"),
        percent: d("3"),
    };
    let up = ConditionMemory::Fired {
        notified: d("104"),
        direction: Direction::Up,
    };
    // 104 × 0.995 = 103.48
    assert_eq!(
        observe(&kind, &up, d("103.49"), band()),
        Observation::Nothing
    );
    assert_eq!(observe(&kind, &up, d("103.48"), band()), Observation::Rearm);
    let down = ConditionMemory::Fired {
        notified: d("96"),
        direction: Direction::Down,
    };
    // 96 × 1.005 = 96.48
    assert_eq!(
        observe(&kind, &down, d("96.47"), band()),
        Observation::Nothing
    );
    assert_eq!(
        observe(&kind, &down, d("96.48"), band()),
        Observation::Rearm
    );
}

// 검증: F2
#[test]
fn 다시_걸기_간격은_설정_값을_따르고_기본은_0_5다() {
    assert_eq!(default_rearm_percent(), d("0.5"));
    let kind = ConditionKind::Above(d("100"));
    let fired = ConditionMemory::Fired {
        notified: d("100"),
        direction: Direction::Up,
    };
    assert_eq!(
        observe(&kind, &fired, d("99.5"), d("1")),
        Observation::Nothing
    );
    assert_eq!(observe(&kind, &fired, d("99"), d("1")), Observation::Rearm);
}
