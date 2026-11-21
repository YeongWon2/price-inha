//! 조건으로 찾기(종목 · 기간 · 손익). 원본에 없는 우리 고유 기능(4.3).

use pi_store::UnixMillis;
use rust_decimal::Decimal;

/// 조건 검색이 견주는 매매 한 건의 요약. 호출하는 쪽(조립하는 곳)이 저장소의 매매에서 만든다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeRecord {
    /// 색인에 올린 기록 키(`NewKnowledgeRecord::record_ref`와 같은 글자).
    pub record_ref: String,
    /// 종목 코드.
    pub symbol: String,
    /// 닫힌(팔린) 시각.
    pub closed_at: UnixMillis,
    /// 손익. 아직 모르면 `None`.
    pub pnl: Option<Decimal>,
}

/// 매매 요약을 내는 포트.
pub trait TradeCatalog {
    /// 모든 매매의 요약. 순서는 정해지지 않았다.
    fn trades(&self) -> Vec<TradeRecord>;

    /// 그 매매(`record_ref`)의 실수 유형들(journal 실수의 기계 글자 — `chased_entry` · `late_stop` ·
    /// `oversized_position` · `no_rationale` · `panic_exit` — 자유 글은 그 글). 기본은 빈 목록이다
    /// (실수를 모르는 카탈로그에 실수 조건을 주면 아무 매매도 맞지 않는다).
    fn mistakes(&self, record_ref: &str) -> Vec<String> {
        let _ = record_ref;
        Vec::new()
    }
}

/// 조건. 정한 것만 거른다(모두 `None`이면 모든 매매). 양끝은 모두 **포함**한다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Condition {
    /// 이 종목만.
    pub symbol: Option<String>,
    /// 이 시각 이후(포함)에 닫힌 것.
    pub from: Option<UnixMillis>,
    /// 이 시각 이전(포함)에 닫힌 것.
    pub to: Option<UnixMillis>,
    /// 손익이 이 값 이상(포함). 손익을 모르는 매매는 빠진다.
    pub min_pnl: Option<Decimal>,
    /// 손익이 이 값 이하(포함). 손익을 모르는 매매는 빠진다.
    pub max_pnl: Option<Decimal>,
    /// 가장 많이 줄 개수. `None`이면 설정의 기본 개수.
    pub max: Option<usize>,
    /// 이 실수 유형이 있는 매매만. [`TradeCatalog::mistakes`] 가운데 앞뒤 공백을 뗀 글자가
    /// 이 값(앞뒤 공백을 뗀 것)과 똑같은 것이 하나라도 있어야 맞는다(대소문자 그대로, 부분 일치 아님).
    /// 실수 목록이 비면 맞지 않는다. 공백을 뗀 값이 비면 조건이 없는 것으로 본다.
    pub mistake: Option<String>,
}

impl Condition {
    /// 매매가 모든 조건에 맞는가(정한 것만 본다. 손익 조건이 있는데 손익을 모르면 맞지 않는다).
    pub(crate) fn matches(&self, trade: &TradeRecord) -> bool {
        let symbol_ok = self.symbol.as_ref().is_none_or(|s| *s == trade.symbol);
        let from_ok = self.from.is_none_or(|t| trade.closed_at.0 >= t.0);
        let to_ok = self.to.is_none_or(|t| trade.closed_at.0 <= t.0);
        let pnl_needed = self.min_pnl.is_some() || self.max_pnl.is_some();
        let pnl_ok = match trade.pnl {
            None => !pnl_needed,
            Some(pnl) => {
                self.min_pnl.is_none_or(|min| pnl >= min)
                    && self.max_pnl.is_none_or(|max| pnl <= max)
            }
        };
        symbol_ok && from_ok && to_ok && pnl_ok
    }
}
