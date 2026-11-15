//! 템플릿 값(`RiskParams`)과 스타일별 프리셋(`dev.md` C4 · V27, `principles.md` "템플릿에서 조정하는 값").
//!
//! 기본값 출처: `docs/research/stock-investing-principles.md` 9장 표 · `dev.md` N2 · N4 ·
//! `docs/dev/decisions.md` 2026-11-16 30번. 모두 **추정값(미확정)** 이며 모의투자로 검증한 뒤 확정한다.

use std::time::Duration;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::promotion::PromotionCriteria;

/// 투자 스타일. 템플릿은 셋 중 하나만 고른다(M3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    /// 단타.
    DayTrade,
    /// 스윙.
    Swing,
    /// 중장기.
    LongTerm,
}

impl Style {
    /// 고를 수 있는 스타일 전부(V27). 순서: 단타 · 스윙 · 중장기.
    pub const ALL: [Style; 3] = [Style::DayTrade, Style::Swing, Style::LongTerm];

    /// 화면에 보이는 이름: "단타" · "스윙" · "중장기".
    pub fn label(self) -> &'static str {
        match self {
            Style::DayTrade => "단타",
            Style::Swing => "스윙",
            Style::LongTerm => "중장기",
        }
    }
}

/// 퍼센트 범위(양 끝 포함). 표가 "7~8%"처럼 범위로 준 값을 그대로 담는다. 한 값이면 `low == high`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PctRange {
    /// 아래 끝(퍼센트).
    pub low: Decimal,
    /// 위 끝(퍼센트).
    pub high: Decimal,
}

/// 개수 범위(양 끝 포함). 표가 "5~8"처럼 범위로 준 값을 그대로 담는다. 한 값이면 `low == high`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountRange {
    /// 아래 끝.
    pub low: u32,
    /// 위 끝.
    pub high: u32,
}

/// 연속 손실 뒤 쉬는 기간.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cooldown {
    /// 그날 남은 시간(당일 종료).
    RestOfDay,
    /// 이 수만큼의 영업일.
    TradingDays(u32),
}

/// 연속 손실 정지: `losses`번 연속 손절하면 `cooldown` 동안 신규 진입을 멈춘다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LossStreakStop {
    /// 연속 손절 횟수.
    pub losses: u32,
    /// 쉬는 기간.
    pub cooldown: Cooldown,
}

/// ATR 의 봉 단위.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtrBar {
    /// 분봉.
    Minute,
    /// 일봉.
    Day,
}

/// ATR 배수 손절.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtrStop {
    /// ATR 배수.
    pub multiple: Decimal,
    /// 봉 단위.
    pub bar: AtrBar,
}

/// 손절 방식. 여러 방식이 함께 걸리면 먼저 닿는 쪽이다(고정 % · ATR 배수 · 구조 이탈 · 시간 손절).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopRule {
    /// 진입가 대비 고정 손실 폭(퍼센트). 중장기는 재앙 방지 손절.
    pub fixed_loss_pct: Option<PctRange>,
    /// ATR 배수 손절.
    pub atr: Option<AtrStop>,
    /// 논리 손절(가설 붕괴 · 구조 이탈).
    pub thesis_break: bool,
    /// 장 마감 전 강제 청산.
    pub flatten_before_close: bool,
}

/// 익절 방식.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TakeProfitRule {
    /// R 배수 목표(예: 2 = 2R).
    pub r_target: Option<Decimal>,
    /// 목표에서 일부만 익절하고 나머지는 남긴다.
    pub partial_at_target: bool,
    /// 트레일링.
    pub trailing: bool,
    /// 가치 도달 · 밸류에이션 상한에서 판다.
    pub valuation_exit: bool,
    /// 리밸런싱으로 판다.
    pub rebalance_exit: bool,
}

/// 최대 보유 기간(시간 손절).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaxHolding {
    /// 그날 안에 판다.
    SameDay,
    /// 이 수만큼의 영업일.
    TradingDays(u32),
    /// 정하지 않음.
    Unlimited,
}

/// 진입 신호 유형.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntrySignal {
    /// 돌파.
    Breakout,
    /// 눌림.
    Pullback,
    /// 가치 범위.
    ValueRange,
    /// 이벤트(갭 · VI 해제 · 실적 서프라이즈 등).
    Event,
}

/// 시장.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Market {
    /// 국내.
    Kr,
    /// 미국.
    Us,
}

/// 유니버스.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Universe {
    /// 시장.
    pub markets: Vec<Market>,
    /// 시가총액 하한(그 시장 통화).
    pub min_market_cap: Option<Decimal>,
    /// 거래대금 하한(그 시장 통화).
    pub min_turnover: Option<Decimal>,
    /// 제외 업종.
    pub excluded_sectors: Vec<String>,
}

/// 거래 시간대.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradingWindow {
    /// 장 시작 직후 제외하는 분.
    pub skip_open_minutes: u32,
    /// 애프터마켓 사용 여부.
    pub after_market: bool,
}

/// 구독 트리거 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// 시세.
    Quote,
    /// 공시.
    Disclosure,
    /// 뉴스.
    News,
    /// 일정.
    Schedule,
}

/// 리밸런싱 주기 · 이탈 폭(중장기).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rebalance {
    /// 주기(영업일).
    pub every_trading_days: u32,
    /// 목표 비중에서 이 폭(퍼센트포인트) 넘게 벗어나면 맞춘다.
    pub drift_pct: Decimal,
}

/// 승인 대기 시간(N2). 이 시간 안에 답이 없으면 FN2 "답 없음"으로 간다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalWait {
    /// 손절 신호(기본 1분).
    pub stop_loss: Duration,
    /// 익절 신호(기본 5분).
    pub take_profit: Duration,
    /// 사는 주문(기본 5분).
    pub buy: Duration,
}

/// 미체결 취소 시간(N4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnfilledCancel {
    /// 이 시간 안에 하나도 체결되지 않으면 취소한다.
    After(Duration),
    /// 장 마감까지 둔다.
    AtClose,
}

/// 템플릿에서 조정하는 값 전부(`principles.md` "템플릿에서 조정하는 값" + N2 · N4 + 승격 기준).
///
/// 비율 칸(`*_pct`)은 퍼센트 단위다(`2` = 2%). 상한이 있는 칸은 [`crate::AbsoluteCaps`] 를 넘을 수 없다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskParams {
    /// 1회 위험 비율 R%(P1, 상한 2%).
    pub risk_per_trade_pct: Decimal,
    /// 일일 손실 한도(P3, 상한 5%).
    pub daily_loss_limit_pct: Decimal,
    /// 최대 낙폭 한도(P4, 상한 20%).
    pub max_drawdown_pct: Decimal,
    /// 연속 손실 정지. `None` 은 쓰지 않음.
    pub loss_streak: Option<LossStreakStop>,
    /// 최대 동시 보유 종목 수.
    pub max_positions: CountRange,
    /// 종목당 최대 비중(P8, 상한 25%).
    pub symbol_weight_pct: Decimal,
    /// 확신 종목의 종목당 최대 비중(P8, 상한 25%). `None` 은 예외 없음.
    pub conviction_weight_pct: Option<Decimal>,
    /// 업종당 최대 비중(P8, 상한 50%).
    pub sector_weight_pct: Decimal,
    /// 최소 현금 비중.
    pub min_cash_pct: Decimal,
    /// 손절 방식.
    pub stop: StopRule,
    /// 익절 방식.
    pub take_profit: TakeProfitRule,
    /// 최대 보유 기간.
    pub max_holding: MaxHolding,
    /// 진입 신호 유형.
    pub entry_signals: Vec<EntrySignal>,
    /// 유니버스.
    pub universe: Universe,
    /// 자동 주문 금액 한도(계좌 대비 퍼센트). 넘으면 승인 요청.
    pub auto_order_limit_pct: Decimal,
    /// 거래 시간대.
    pub trading_window: TradingWindow,
    /// 추격 허용 폭(신호가 대비 퍼센트). `None` 은 해당 없음.
    pub chase_limit_pct: Option<Decimal>,
    /// 일일 최대 매매 횟수.
    pub max_trades_per_day: u32,
    /// 구독 트리거.
    pub triggers: Vec<TriggerKind>,
    /// 리밸런싱. `None` 은 하지 않음.
    pub rebalance: Option<Rebalance>,
    /// 승인 대기 시간(N2).
    pub approval_wait: ApprovalWait,
    /// 미체결 취소 시간(N4).
    pub unfilled_cancel: UnfilledCancel,
    /// 승격 기준(P14 · M3).
    pub promotion: PromotionCriteria,
}

/// 스타일의 기본값이 든 템플릿 값(C4). 부를 때마다 새 사본을 준다.
pub fn preset(style: Style) -> RiskParams {
    match style {
        Style::DayTrade => crate::presets::day_trade(),
        Style::Swing => crate::presets::swing(),
        Style::LongTerm => crate::presets::long_term(),
    }
}
