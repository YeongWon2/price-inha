//! 결정 메모 정산과 교훈("어제 메모"의 재료) (6차, 결정 17 · additions.md R7-4 · M11 · C25 · E54 · N20).
//!
//! 판단된 매매마다 결정 메모를 정산 대기로 만들고, 결정 날에서 [`SETTLE_AFTER_TRADING_DAYS`]거래일 뒤
//! 날의 종가로 수익률을 정산한 뒤 복기 담당의 AI 턴에서 2~4문장 교훈을 받아 가린 다음 저장한다.
//! 종가 · 거래일 · AI 턴 · 가리기는 이 크레이트가 직접 부르지 않고 포트(trait)로 받는다.
//! 수익률 · 초과수익은 만분율(bp) 정수이고, 반올림은 0에서 멀어지는 쪽이다.

use pi_store::{DecisionMemo, TradeId, TradeSide, UnixMillis};
use rust_decimal::Decimal;

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
        todo!()
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
        todo!()
    }

    /// 다음 팀장 판단에 넣을 어제 메모를 고른다(N20). 정산됨이고 `as_of` 이전(같은 시각 포함)에 정산된 메모만
    /// 쓴다. 최근 것(결정 날짜가 늦은 것, 같으면 번호가 큰 것)부터다.
    pub fn memo_brief(&self, symbol: &str, as_of: UnixMillis) -> Result<MemoBrief, Error> {
        todo!()
    }
}
