//! 결정 메모 정산과 교훈("어제 메모"의 재료) (6차, 결정 17 · additions.md R7-4 · M11 · C25 · E54 · N20).
//!
//! 판단된 매매마다 결정 메모를 정산 대기로 만들고, 결정 날에서 [`SETTLE_AFTER_TRADING_DAYS`]거래일 뒤
//! 날의 종가로 수익률을 정산한 뒤 복기 담당의 AI 턴에서 2~4문장 교훈을 받아 가린 다음 저장한다.
//! 종가 · 거래일 · AI 턴 · 가리기는 이 크레이트가 직접 부르지 않고 포트(trait)로 받는다.
//! 수익률 · 초과수익은 만분율(bp) 정수이고, 반올림은 0에서 멀어지는 쪽이다.

use pi_store::{
    DecisionMemo, DecisionMemoState, HISTORY_PAGE_MAX, MemoSettlement, NewDecisionMemo, StoreError,
    TradeId, TradeSide, UnixMillis,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::{Error, Journal};

/// 결정 날에서 정산 날까지의 거래일 수(N20).
pub const SETTLE_AFTER_TRADING_DAYS: u32 = 5;
/// 정산 날 종가가 없을 때, 정산 날에서 "정산 못 함"으로 닫기까지의 거래일 수(E54 · N20).
pub const UNSETTLEABLE_AFTER_TRADING_DAYS: u32 = 5;
/// 다음 판단에 넣는 같은 종목 메모 수(N20).
pub const SAME_SYMBOL_MEMOS: usize = 5;
/// 다음 판단에 넣는 다른 종목 교훈 수(N20).
pub const OTHER_SYMBOL_LESSONS: usize = 3;

/// 확정 종가 포트. 시장 어댑터는 조립하는 곳이 끼운다.
pub trait ClosePrices: Send + Sync {
    /// 그 종목의 그 날(`YYYY-MM-DD`, 시장 시간대) 확정 종가. 받을 수 없으면(거래정지 · 자료 오류 · 아직 미확정) `None`.
    fn close(&self, symbol: &str, date: &str) -> Option<Decimal>;

    /// 그 종목의 기준 지수의 그 날 확정 종가. 받을 수 없으면 `None`.
    fn benchmark_close(&self, symbol: &str, date: &str) -> Option<Decimal>;
}

/// 거래일 달력 포트.
pub trait TradingCalendar: Send + Sync {
    /// 그 종목 시장에서 `date`(`YYYY-MM-DD`) 바로 다음 거래일. 달력을 모르면 `None`(그 메모는 정산 대기로 둔다).
    fn next_trading_day(&self, symbol: &str, date: &str) -> Option<String>;
}

/// 교훈을 쓸 때 AI에게 넘기는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonInput {
    /// 매매 번호.
    pub trade: TradeId,
    /// 종목 코드.
    pub symbol: String,
    /// 결정 날짜(`YYYY-MM-DD`).
    pub date: String,
    /// 팀장 판단(사기 · 팔기).
    pub side: TradeSide,
    /// 결정 근거의 가설.
    pub hypothesis: Option<String>,
    /// 결정 근거의 계기.
    pub trigger: Option<String>,
    /// 수익률(만분율). 팔기는 부호를 뒤집은 값이다.
    pub return_bp: i64,
    /// 초과수익(만분율). 기준 지수 시세가 없으면 `None`.
    pub excess_bp: Option<i64>,
    /// 보유일(거래일 수).
    pub holding_days: u32,
}

/// 복기 담당의 AI 턴 포트. 2~4문장 교훈을 돌려준다.
pub trait LessonWriter: Send + Sync {
    /// 교훈 글. AI가 답하지 못하면 `None`.
    fn lesson(&self, input: &LessonInput) -> Option<String>;
}

/// 글 속 비밀 값 가리기 포트. 교훈은 이것을 거친 뒤 저장한다. 조립에서 비밀 보관의 가리기를 끼운다.
pub trait Redactor: Send + Sync {
    /// 가린 글.
    fn redact(&self, text: &str) -> String;
}

/// 정산에 쓰는 포트 묶음.
#[derive(Clone, Copy)]
pub struct SettlePorts<'a> {
    /// 확정 종가.
    pub prices: &'a dyn ClosePrices,
    /// 거래일 달력.
    pub calendar: &'a dyn TradingCalendar,
    /// 복기 담당의 AI 턴.
    pub lessons: &'a dyn LessonWriter,
    /// 가리기.
    pub redactor: &'a dyn Redactor,
}

/// 다음 팀장 판단에 넣을 어제 메모(N20). 넣는 일은 server · agent가 한다.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoBrief {
    /// 같은 종목의 정산된 메모(결정 + 교훈). 최근 것부터 [`SAME_SYMBOL_MEMOS`]건까지.
    pub same_symbol: Vec<DecisionMemo>,
    /// 다른 종목의 정산된 메모의 교훈만. 최근 것부터 [`OTHER_SYMBOL_LESSONS`]건까지.
    pub other_lessons: Vec<String>,
}

impl Journal {
    /// 판단된 매매의 결정 메모를 정산 대기로 만든다(M11). 같은 매매로 이미 있으면 새로 만들지 않고
    /// 있던 메모를 그대로 돌려준다(날짜 · 판단이 달라도 바꾸지 않는다).
    ///
    /// - 없는 매매: [`Error::Store`]([`pi_store::StoreError::TradeNotFound`])
    /// - 판단 전(논의중) 매매: [`Error::Store`]([`pi_store::StoreError::TradeNotJudged`])
    /// - 날짜가 `YYYY-MM-DD` 모양이 아님: [`Error::Store`]([`pi_store::StoreError::InvalidDecisionMemo`])
    pub fn record_decision(
        &self,
        trade: TradeId,
        date: &str,
        side: TradeSide,
        at: UnixMillis,
    ) -> Result<DecisionMemo, Error> {
        let new = NewDecisionMemo {
            trade,
            date: date.to_string(),
            side,
            at,
        };
        match self.store.create_decision_memo(new) {
            Ok(memo) => Ok(memo),
            Err(StoreError::DecisionMemoExists(_)) => {
                let found = self.all_memos()?.into_iter().find(|m| m.trade == trade);
                found.ok_or(Error::Store(StoreError::DecisionMemoExists(trade)))
            }
            Err(err) => Err(err.into()),
        }
    }

    /// `today`(`YYYY-MM-DD`)까지 정산 날이 온 정산 대기 메모를 정산한다(N20 · E54). 이번에 닫은 메모들을
    /// 정산 대기 목록 순서(결정 날짜 · 번호)로 돌려준다.
    ///
    /// - 정산 날은 결정 날에서 [`SETTLE_AFTER_TRADING_DAYS`]번째 다음 거래일이다. 정산 날이 `today`보다 뒤면 건너뛴다.
    /// - 수익률은 결정 날 종가 대비 정산 날 종가(사기 `(정산 − 결정) ÷ 결정`, 팔기는 부호를 뒤집는다).
    ///   기준 지수의 같은 두 날 종가가 다 있으면 초과수익 = 수익률 − 지수 수익률, 아니면 `None`.
    ///   만분율 정수이고 반올림은 0에서 멀어지는 쪽. 보유일은 두 날 사이 거래일 수.
    /// - 정산 날 종가가 없으면 정산 대기로 둔다. 정산 날에서 [`UNSETTLEABLE_AFTER_TRADING_DAYS`]거래일이 지난
    ///   날이 `today` 이하인데도 없으면 정산 못 함으로 닫는다. 정산 날 뒤의 시세로 앞을 채우지 않는다(P10).
    /// - 교훈은 [`LessonWriter`]에서 받아 [`Redactor`]로 가린 뒤 저장한다. 답이 없거나 빈 글이면 그 메모는
    ///   정산 대기로 남고(다른 메모는 계속한다) 다음에 다시 한다.
    /// - 한 메모씩 한 쓰기로 닫는다. 이미 닫힌 메모는 건너뛴다.
    pub fn settle_memos(
        &self,
        today: &str,
        ports: &SettlePorts<'_>,
        at: UnixMillis,
    ) -> Result<Vec<DecisionMemo>, Error> {
        let mut closed = Vec::new();
        for memo in self.store.decision_memos_pending(today)? {
            let outcome = match reckon(&memo, today, ports) {
                Reckoning::Wait => continue,
                Reckoning::Unsettleable => MemoSettlement::Unsettleable,
                Reckoning::Ready {
                    return_bp,
                    excess_bp,
                } => {
                    let rationale = self.store.get_trade(memo.trade)?.rationale;
                    let input = LessonInput {
                        trade: memo.trade,
                        symbol: memo.symbol.clone(),
                        date: memo.date.clone(),
                        side: memo.side,
                        hypothesis: rationale.hypothesis,
                        trigger: rationale.trigger,
                        return_bp,
                        excess_bp,
                        holding_days: SETTLE_AFTER_TRADING_DAYS,
                    };
                    let Some(outcome) = lesson_settlement(&input, ports) else {
                        continue;
                    };
                    outcome
                }
            };
            closed.push(self.store.settle_decision_memo(memo.id, outcome, at)?);
        }
        Ok(closed)
    }

    /// 다음 팀장 판단에 넣을 어제 메모를 고른다(N20). 정산됨이고 `as_of` 이전(같은 시각 포함)에 정산된 메모만
    /// 쓴다. 최근 것(결정 날짜가 늦은 것, 같으면 번호가 큰 것)부터다.
    pub fn memo_brief(&self, symbol: &str, as_of: UnixMillis) -> Result<MemoBrief, Error> {
        let mut settled: Vec<DecisionMemo> = self
            .all_memos()?
            .into_iter()
            .filter(|m| m.state == DecisionMemoState::Settled)
            .filter(|m| m.settled_at.is_some_and(|s| s <= as_of))
            .collect();
        settled.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.0.cmp(&a.id.0)));
        let (same, other): (Vec<DecisionMemo>, Vec<DecisionMemo>) =
            settled.into_iter().partition(|m| m.symbol == symbol);
        Ok(MemoBrief {
            same_symbol: same.into_iter().take(SAME_SYMBOL_MEMOS).collect(),
            other_lessons: other
                .into_iter()
                .filter_map(|m| m.lesson)
                .take(OTHER_SYMBOL_LESSONS)
                .collect(),
        })
    }

    /// 모든 결정 메모(번호가 큰 것부터). 저장소의 쪽 단위 목록을 끝까지 읽는다.
    fn all_memos(&self) -> Result<Vec<DecisionMemo>, Error> {
        let mut out: Vec<DecisionMemo> = Vec::new();
        loop {
            let before = out.last().map(|m| m.id);
            let page = self.store.decision_memos_page(before, HISTORY_PAGE_MAX)?;
            if page.is_empty() {
                return Ok(out);
            }
            out.extend(page);
        }
    }
}

// ───────────── 안에서만 쓰는 것 ─────────────

/// 메모 하나의 셈 결과(AI를 부르기 전).
#[derive(Debug, PartialEq, Eq)]
enum Reckoning {
    /// 아직 닫지 않는다(정산 날 전 · 달력 모름 · 시세 기다림).
    Wait,
    /// 정산 날 종가를 끝내 받지 못했다.
    Unsettleable,
    /// 셈이 끝났다. 교훈만 받으면 닫는다.
    Ready {
        /// 수익률(만분율).
        return_bp: i64,
        /// 초과수익(만분율).
        excess_bp: Option<i64>,
    },
}

/// 메모 하나를 `today` 기준으로 셈한다. 정산 날 뒤의 시세는 쓰지 않는다(P10).
fn reckon(memo: &DecisionMemo, today: &str, ports: &SettlePorts<'_>) -> Reckoning {
    let calendar = ports.calendar;
    let Some(settle_day) = nth_trading_day(
        calendar,
        &memo.symbol,
        &memo.date,
        SETTLE_AFTER_TRADING_DAYS,
    ) else {
        return Reckoning::Wait;
    };
    if settle_day.as_str() > today {
        return Reckoning::Wait;
    }
    if let Some((return_bp, excess_bp)) = returns(memo, &settle_day, ports.prices) {
        return Reckoning::Ready {
            return_bp,
            excess_bp,
        };
    }
    let deadline = nth_trading_day(
        calendar,
        &memo.symbol,
        &settle_day,
        UNSETTLEABLE_AFTER_TRADING_DAYS,
    );
    match deadline {
        Some(day) if day.as_str() <= today => Reckoning::Unsettleable,
        _ => Reckoning::Wait,
    }
}

/// 교훈을 받아 가린 뒤 정산 결과로 만든다. 답이 없거나 공백뿐이면 `None`.
fn lesson_settlement(input: &LessonInput, ports: &SettlePorts<'_>) -> Option<MemoSettlement> {
    let answer = ports.lessons.lesson(input)?;
    if answer.trim().is_empty() {
        return None;
    }
    let lesson = ports.redactor.redact(&answer);
    if lesson.trim().is_empty() {
        return None;
    }
    Some(MemoSettlement::Settled {
        return_bp: input.return_bp,
        excess_bp: input.excess_bp,
        holding_days: input.holding_days,
        lesson,
    })
}

/// `date`에서 `n`번째 다음 거래일. 달력이 모르면 `None`.
fn nth_trading_day(
    calendar: &dyn TradingCalendar,
    symbol: &str,
    date: &str,
    n: u32,
) -> Option<String> {
    let mut day = date.to_string();
    for _ in 0..n {
        day = calendar.next_trading_day(symbol, &day)?;
    }
    Some(day)
}

/// (수익률, 초과수익) 만분율. 결정 날 · 정산 날 종목 종가 가운데 하나라도 없으면 `None`.
fn returns(
    memo: &DecisionMemo,
    settle_day: &str,
    prices: &dyn ClosePrices,
) -> Option<(i64, Option<i64>)> {
    let stock = raw_return(
        prices.close(&memo.symbol, &memo.date)?,
        prices.close(&memo.symbol, settle_day)?,
    )?;
    let index = match (
        prices.benchmark_close(&memo.symbol, &memo.date),
        prices.benchmark_close(&memo.symbol, settle_day),
    ) {
        (Some(from), Some(to)) => raw_return(from, to),
        _ => None,
    };
    let (signed, excess) = match memo.side {
        TradeSide::Buy => (stock, index.map(|i| stock - i)),
        TradeSide::Sell => (-stock, index.map(|i| i - stock)),
    };
    Some((to_bp(signed)?, excess.and_then(to_bp)))
}

/// 정확한 수익률(만분율, 반올림 전). 결정 종가가 0 이하이면 셈할 수 없어 `None`.
fn raw_return(from: Decimal, to: Decimal) -> Option<Decimal> {
    if from <= Decimal::ZERO {
        return None;
    }
    (to - from)
        .checked_mul(Decimal::from(BP_PER_UNIT))?
        .checked_div(from)
}

/// 만분율 정수로 반올림한다(0에서 멀어지는 쪽).
fn to_bp(value: Decimal) -> Option<i64> {
    value
        .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero)
        .to_i64()
}

/// 1을 만분율로.
const BP_PER_UNIT: i64 = 10_000;

#[cfg(test)]
mod tests {
    use super::*;

    struct OnePrice;

    impl ClosePrices for OnePrice {
        fn close(&self, _symbol: &str, date: &str) -> Option<Decimal> {
            match date {
                "2027-01-01" => Some(Decimal::new(10_000, 0)),
                "2027-01-02" => Some(Decimal::new(10_100, 0)),
                _ => None,
            }
        }

        fn benchmark_close(&self, _symbol: &str, date: &str) -> Option<Decimal> {
            match date {
                "2027-01-01" => Some(Decimal::new(1_000, 0)),
                "2027-01-02" => Some(Decimal::new(1_030, 0)),
                _ => None,
            }
        }
    }

    fn memo(side: TradeSide) -> DecisionMemo {
        DecisionMemo {
            id: pi_store::DecisionMemoId(1),
            trade: TradeId(1),
            date: "2027-01-01".to_string(),
            symbol: "005930".to_string(),
            side,
            state: DecisionMemoState::Pending,
            return_bp: None,
            excess_bp: None,
            holding_days: None,
            lesson: None,
            created_at: UnixMillis(0),
            settled_at: None,
        }
    }

    #[test]
    fn 팔기의_초과수익은_지수_수익률에서_종목_수익률을_뺀다() {
        // 종목 +1%, 지수 +3%: 사기는 −2%, 팔기는 수익률 −1% · 초과수익 +2%.
        assert_eq!(
            returns(&memo(TradeSide::Buy), "2027-01-02", &OnePrice),
            Some((100, Some(-200)))
        );
        assert_eq!(
            returns(&memo(TradeSide::Sell), "2027-01-02", &OnePrice),
            Some((-100, Some(200)))
        );
    }

    #[test]
    fn 결정_날_종가가_없거나_0이면_셈하지_않는다() {
        assert_eq!(
            returns(&memo(TradeSide::Buy), "2027-01-03", &OnePrice),
            None
        );
        assert_eq!(raw_return(Decimal::ZERO, Decimal::ONE), None);
    }

    #[test]
    fn 초과수익은_정확한_값끼리_뺀_뒤_한_번_반올림한다() {
        // 0.4bp − (−0.4bp) = 0.8bp → 1 (따로 반올림하면 0 − 0 = 0).
        let stock = Decimal::new(4, 1);
        let index = Decimal::new(-4, 1);
        assert_eq!(to_bp(stock - index), Some(1));
        assert_eq!(to_bp(Decimal::new(-5, 1)), Some(-1));
    }
}
