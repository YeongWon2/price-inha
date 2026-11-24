//! (30차 1번 · C31 · V86 · R3-1 · R16-5) 모의계좌 읽기: 현금 · 보유 · 통화별 평가액 · 최고 · 낙폭.
//!
//! - 계좌는 모의계좌(pi-paper)뿐이다. 실제 증권사를 읽지 않는다(AGENTS 6번 · 결정 17).
//! - 평가액 · 그날 시작 · 최고는 주문 길이 하루 손실 · 낙폭 판정에 쓰는 장부([`Trading::books`] —
//!   `count_books`)를 **그대로** 쓰고, 낙폭은 판정과 같은 셈(`pi_rules::drawdown_pct`)이다. 따로 다시 세지 않으므로
//!   화면의 낙폭과 전체 정지 판정이 어긋나지 않는다. 낙폭 한도도 판정과 같이 `min(템플릿 값, 절대 상한)`이다.
//! - 보유 줄은 같은 장부의 보유(같은 순간의 모의계좌 읽기)에서 만든다. 지금 값은 받은 시세가 있을 때만 있다.
//! - 현금 줄은 모의계좌의 공개 읽기(`account_snapshot`)의 쓸 수 있는 · 묶인 · 결제 전 돈이다.
//! - 값은 모두 십진수 글자다(부동소수점 수를 쓰지 않는다).

use pi_market::{Currency, Market};
use pi_rules::{Book, Position};
use pi_template::AbsoluteCaps;
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::{ACCOUNT_PAPER, Trading};
use crate::TradingError;

/// 응답에 쓰는 통화 글자.
fn currency_word(currency: &Currency) -> &str {
    match currency {
        Currency::Krw => "KRW",
        Currency::Usd => "USD",
        Currency::Other(code) => code,
        _ => "",
    }
}

/// 응답에 쓰는 시장 글자.
fn market_word(market: Market) -> &'static str {
    match market {
        Market::Us => "us",
        _ => "kr",
    }
}

/// 십진수 글자(뒤의 0을 걷어 낸 모양).
fn text(value: Decimal) -> Value {
    Value::String(value.normalize().to_string())
}

/// 보유 한 줄. `quote`는 받은 시세(없으면 지금 값 · 평가 · 평가 손익이 `null`).
fn holding_json(position: &Position, currency: &Currency, quote: Option<Decimal>) -> Value {
    let value = quote.map(|price| position.quantity.saturating_mul(price));
    let pnl = value.map(|value| value - position.quantity.saturating_mul(position.avg_price));
    json!({
        "symbol": position.symbol.as_str(),
        "market": market_word(position.market),
        "currency": currency_word(currency),
        "quantity": text(position.quantity),
        "average_price": text(position.avg_price),
        "last_price": quote.map_or(Value::Null, text),
        "value": value.map_or(Value::Null, text),
        "pnl": pnl.map_or(Value::Null, text),
    })
}

/// 평가액 한 줄: 판정 장부의 값 그대로, 낙폭은 판정과 같은 셈(소수 둘째 자리 반올림).
fn equity_json(book: &Book, limit: Option<Decimal>) -> Value {
    let drawdown = pi_rules::drawdown_pct(book).round_dp(2);
    json!({
        "currency": currency_word(&book.currency),
        "total": text(book.equity),
        "day_start": text(book.day_start_equity),
        "day_pnl": text(book.equity - book.day_start_equity),
        "peak": text(book.peak_equity),
        "drawdown_pct": text(drawdown),
        "drawdown_limit_pct": limit.map_or(Value::Null, text),
    })
}

/// 현금 한 줄.
fn cash_json(currency: &str, cash: &pi_paper::CashBalance) -> Value {
    json!({
        "currency": currency,
        "available": text(cash.available),
        "held": text(cash.held),
        "unsettled": text(cash.unsettled),
    })
}

impl Trading {
    /// `GET /api/account`의 몸통(모양은 [`super::ACCOUNT_PATH`] 문서).
    pub(super) async fn account_view(&self) -> Result<Value, TradingError> {
        let now = self.clock.now();
        let books = self.books(now).await?;
        let snapshot = self.executor.broker().account_snapshot(now);
        // 판정의 낙폭 한도: 쓰는 템플릿(멈춤이어도)의 값을 절대 상한으로 다시 막는다(`pi_rules` 판정과 같다).
        let limit = self.params()?.map(|params| {
            params
                .max_drawdown_pct
                .min(AbsoluteCaps::FIXED.max_drawdown_pct)
        });
        let mut holdings = Vec::new();
        for book in &books {
            for position in book.positions.iter().filter(|p| !p.quantity.is_zero()) {
                let quote = self
                    .last_quote(position.symbol.as_str())
                    .map(|quote| quote.price);
                holdings.push(holding_json(position, &book.currency, quote));
            }
        }
        let equity: Vec<Value> = books.iter().map(|book| equity_json(book, limit)).collect();
        Ok(json!({
            "account": ACCOUNT_PAPER,
            "cash": [cash_json("KRW", &snapshot.krw), cash_json("USD", &snapshot.usd)],
            "holdings": holdings,
            "equity": equity,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(equity: &str, peak: &str) -> Book {
        Book {
            currency: Currency::Krw,
            equity: equity.parse().unwrap(),
            day_start_equity: "10000000".parse().unwrap(),
            peak_equity: peak.parse().unwrap(),
            positions: Vec::new(),
        }
    }

    #[test]
    fn 낙폭은_판정과_같은_셈을_소수_둘째_자리로_내고_최고_위면_0이다() {
        let line = equity_json(&book("9974925", "10000000"), Some(Decimal::from(20)));
        assert_eq!(line["drawdown_pct"], "0.25");
        assert_eq!(line["day_pnl"], "-25075");
        assert_eq!(line["drawdown_limit_pct"], "20");
        let line = equity_json(&book("10500000", "10000000"), None);
        assert_eq!(line["drawdown_pct"], "0");
        assert_eq!(line["drawdown_limit_pct"], Value::Null);
    }

    #[test]
    fn 시세가_없으면_지금_값_평가_손익이_없다() {
        let position = Position {
            market: Market::Kr,
            symbol: pi_market::Symbol::new("000660"),
            quantity: Decimal::from(5),
            avg_price: Decimal::from(100_015),
            last_price: Decimal::from(100_015),
        };
        let line = holding_json(&position, &Currency::Krw, None);
        assert_eq!(line["last_price"], Value::Null);
        assert_eq!(line["pnl"], Value::Null);
        let line = holding_json(&position, &Currency::Krw, Some(Decimal::from(95_000)));
        assert_eq!(line["value"], "475000");
        assert_eq!(line["pnl"], "-25075");
        assert_eq!(line["market"], "kr");
    }
}
