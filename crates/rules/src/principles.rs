//! 주문 안 밖에서 따로 부르는 원칙 판정.

use rust_decimal::Decimal;

use crate::Violation;

/// 보유 중 손절값 바꾸기(P2). 산 종목(롱)만 있으므로 손절은 올리거나 그대로 두기만 된다.
/// 내리면(넓히면) [`Violation::StopWidened`].
pub fn check_stop_change(current: Decimal, proposed: Decimal) -> Result<(), Violation> {
    let _ = (current, proposed);
    todo!()
}
