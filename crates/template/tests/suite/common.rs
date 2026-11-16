//! 시험 공용 도우미.

use rust_decimal::Decimal;

/// 글에서 Decimal 을 만든다.
pub fn d(s: &str) -> Decimal {
    s.parse().unwrap()
}
