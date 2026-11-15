//! 시세 조건과 다시 걸기(결정 21). 순수 계산이다.

use rust_decimal::Decimal;

/// 시세 조건.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionKind {
    /// 가격이 이 값 **이상**이면 맞다(닿으면 맞음).
    Above(Decimal),
    /// 가격이 이 값 **이하**이면 맞다.
    Below(Decimal),
    /// 기준가에서 변동률(%)이 이 값 **이상**이면 맞다(오름 · 내림 모두).
    ///
    /// 변동률 = |가격 − 기준가| ÷ 기준가 × 100.
    Change {
        /// 기준가(0보다 커야 한다).
        reference: Decimal,
        /// 변동률(%, 0보다 커야 한다).
        percent: Decimal,
    },
}

/// 맞은 방향.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// 위로(넘음 · 오른 변동).
    Up,
    /// 아래로(내려감 · 내린 변동).
    Down,
}

/// 조건 하나의 다시 걸기 상태. 저장소에 작업 상태와 함께 둔다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionMemory {
    /// 걸려 있다. 맞으면 알린다.
    Armed,
    /// 알렸다. 다시 걸릴 때까지 알리지 않는다.
    Fired {
        /// 알린 값.
        notified: Decimal,
        /// 알린 방향.
        direction: Direction,
    },
}

/// 다시 걸기 간격의 기본값: 0.5(%).
pub fn default_rearm_percent() -> Decimal {
    Decimal::new(5, 1)
}

/// 시세 하나를 보고 생긴 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observation {
    /// 걸려 있었고 맞았다. 알린다(알린 값 = 이번 가격).
    Fire(Direction),
    /// 알린 뒤였고, 알린 값에서 반대 방향으로 간격 이상 벗어났다. 다시 건다(이번에는 알리지 않는다).
    Rearm,
    /// 아무 일도 없다.
    Nothing,
}

/// 시세 하나를 본다. 한 번에 한 가지만 바뀐다.
///
/// - `Armed`이고 조건이 맞으면 `Fire`.
/// - `Fired { notified, direction }`이면: `Up`으로 알린 것은 가격 ≤ notified × (1 − 간격/100),
///   `Down`으로 알린 것은 가격 ≥ notified × (1 + 간격/100)일 때 `Rearm`(경계 포함). 아니면 `Nothing`.
/// - `Above`는 늘 `Up`, `Below`는 늘 `Down`, `Change`는 가격이 기준가보다 크면 `Up`, 작으면 `Down`.
pub fn observe(
    kind: &ConditionKind,
    memory: &ConditionMemory,
    price: Decimal,
    rearm_percent: Decimal,
) -> Observation {
    match memory {
        ConditionMemory::Armed => match matched(kind, price) {
            Some(direction) => Observation::Fire(direction),
            None => Observation::Nothing,
        },
        ConditionMemory::Fired {
            notified,
            direction,
        } => {
            let band = *notified * rearm_percent / Decimal::ONE_HUNDRED;
            let back = match direction {
                Direction::Up => price <= *notified - band,
                Direction::Down => price >= *notified + band,
            };
            if back {
                Observation::Rearm
            } else {
                Observation::Nothing
            }
        }
    }
}

/// 조건이 맞으면 그 방향.
fn matched(kind: &ConditionKind, price: Decimal) -> Option<Direction> {
    match kind {
        ConditionKind::Above(level) => (price >= *level).then_some(Direction::Up),
        ConditionKind::Below(level) => (price <= *level).then_some(Direction::Down),
        ConditionKind::Change { reference, percent } => {
            if reference.is_zero() {
                return None;
            }
            let change = (price - *reference).abs() / *reference * Decimal::ONE_HUNDRED;
            if change < *percent {
                return None;
            }
            if price > *reference {
                Some(Direction::Up)
            } else if price < *reference {
                Some(Direction::Down)
            } else {
                None
            }
        }
    }
}

/// 조건 값이 쓸 수 있는가: 기준값 · 기준가 · 변동률이 0보다 크다.
pub(crate) fn is_valid(kind: &ConditionKind) -> bool {
    match kind {
        ConditionKind::Above(level) | ConditionKind::Below(level) => *level > Decimal::ZERO,
        ConditionKind::Change { reference, percent } => {
            *reference > Decimal::ZERO && *percent > Decimal::ZERO
        }
    }
}
