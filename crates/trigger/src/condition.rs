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
/// - 알린 기억은 알린 값이 **지금 조건의 문턱을 그 방향으로 넘은 값일 때만** 쓴다(6차 보강 1번). 아니면
///   (기준가가 바뀌었거나 다른 조건의 기억이 남은 것) `Armed`로 본다.
pub fn observe(
    kind: &ConditionKind,
    memory: &ConditionMemory,
    price: Decimal,
    rearm_percent: Decimal,
) -> Observation {
    match memory {
        ConditionMemory::Fired {
            notified,
            direction,
        } if holds(kind, *notified, *direction) => {
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
        // 걸려 있거나, 알린 기억이 지금 조건에서 성립하지 않는다.
        _ => match matched(kind, price) {
            Some(direction) => Observation::Fire(direction),
            None => Observation::Nothing,
        },
    }
}

/// 알린 기억이 지금 조건에서 성립하는가: 알린 값이 지금 조건의 문턱을 알린 방향으로 넘은 값이다(경계 포함).
fn holds(kind: &ConditionKind, notified: Decimal, direction: Direction) -> bool {
    matched(kind, notified) == Some(direction)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Decimal {
        text.parse().unwrap_or_default()
    }

    fn change(reference: &str) -> ConditionKind {
        ConditionKind::Change {
            reference: d(reference),
            percent: d("3"),
        }
    }

    fn fired(notified: &str, direction: Direction) -> ConditionMemory {
        ConditionMemory::Fired {
            notified: d(notified),
            direction,
        }
    }

    fn see(kind: &ConditionKind, memory: &ConditionMemory, price: &str) -> Observation {
        observe(kind, memory, d(price), default_rearm_percent())
    }

    #[test]
    fn 알린_값이_지금_문턱을_넘지_않았으면_걸린_것으로_본다() {
        // 기준가 103 · 3%: 문턱 106.09 이상 · 99.91 이하.
        let kind = change("103");
        let stale = fired("103", Direction::Up);
        assert_eq!(see(&kind, &stale, "105"), Observation::Nothing);
        assert_eq!(
            see(&kind, &stale, "106.09"),
            Observation::Fire(Direction::Up)
        );
        // 알린 값에서 되돌아온 값이어도 다시 걸기로 보지 않는다(기억 자체를 쓰지 않는다).
        assert_eq!(see(&kind, &stale, "102"), Observation::Nothing);
    }

    #[test]
    fn 방향이_다른_알린_기억은_쓰지_않는다() {
        // 110은 오른 쪽 문턱을 넘은 값이지 내린 쪽 문턱을 넘은 값이 아니다.
        let kind = change("100");
        let wrong = fired("110", Direction::Down);
        assert_eq!(see(&kind, &wrong, "97"), Observation::Fire(Direction::Down));
    }

    #[test]
    fn 기준가가_바뀌어도_알린_값이_새_문턱을_넘은_값이면_유지한다() {
        // 조정자 결정: 알린 값 110은 새 기준가 103의 문턱(106.09)도 넘는다.
        let kind = change("103");
        let kept = fired("110", Direction::Up);
        assert_eq!(see(&kind, &kept, "111"), Observation::Nothing);
        assert_eq!(see(&kind, &kept, "109.45"), Observation::Rearm);
    }

    #[test]
    fn 넘음과_내려감_조건도_알린_값이_문턱에_닿았을_때만_유지한다() {
        let above = ConditionKind::Above(d("100"));
        assert_eq!(
            see(&above, &fired("100", Direction::Up), "101"),
            Observation::Nothing
        );
        assert_eq!(
            see(&above, &fired("99", Direction::Up), "101"),
            Observation::Fire(Direction::Up)
        );
        let below = ConditionKind::Below(d("100"));
        assert_eq!(
            see(&below, &fired("100", Direction::Down), "99"),
            Observation::Nothing
        );
        assert_eq!(
            see(&below, &fired("101", Direction::Down), "99"),
            Observation::Fire(Direction::Down)
        );
    }
}
