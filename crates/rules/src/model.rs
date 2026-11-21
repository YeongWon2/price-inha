//! 점검에 들어가는 값들. 모두 호출하는 쪽이 채워 넘긴다(이 모듈은 아무것도 가져오지 않는다).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use pi_market::{Currency, Market, MarketCalendar, Quote, StalePolicy, Symbol, TradingStatus};
use pi_paper::{ApprovedOrder, MarketRules, OrderType, Side};
use pi_template::RiskParams;
use rust_decimal::Decimal;

/// 매매의 상태. store의 `TradeState`와 1:1이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TradeStage {
    /// 논의중.
    Discussing,
    /// 판단됨. 주문 안은 이 상태에서만 받는다(E20).
    Judged,
    /// 승인대기.
    AwaitingApproval,
    /// 막힘.
    Blocked,
    /// 주문됨.
    Ordered,
    /// 보유중.
    Holding,
    /// 팔림.
    Sold,
    /// 복기됨.
    Reviewed,
}

impl TradeStage {
    /// 모든 상태.
    pub const ALL: [TradeStage; 8] = [
        TradeStage::Discussing,
        TradeStage::Judged,
        TradeStage::AwaitingApproval,
        TradeStage::Blocked,
        TradeStage::Ordered,
        TradeStage::Holding,
        TradeStage::Sold,
        TradeStage::Reviewed,
    ];
}

/// 투자 팀 상태. store의 `TeamState`와 1:1이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeamStatus {
    /// 운용 중. 사는 주문은 이 상태에서만 받는다(E31).
    Operating,
    /// 쉬는 중.
    Resting,
    /// 하루 한도 멈춤(E23 · P3).
    DailyLimitHalt,
    /// 이상 감지 멈춤(P11).
    AnomalyHalt,
    /// 전체 정지(E24 · P4).
    FullStop,
}

impl TeamStatus {
    /// 모든 상태.
    pub const ALL: [TeamStatus; 5] = [
        TeamStatus::Operating,
        TeamStatus::Resting,
        TeamStatus::DailyLimitHalt,
        TeamStatus::AnomalyHalt,
        TeamStatus::FullStop,
    ];
}

/// 주문 안의 목적. 방향은 목적에서 정해진다(사기: 새 진입 · 추가 매수, 팔기: 나머지).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Purpose {
    /// 새로 산다.
    NewEntry,
    /// 이미 가진 종목을 더 산다(P6: 이익 방향만).
    AddOn,
    /// 손절로 판다. 어떤 멈춤에서도 받는다(E34 · P11).
    StopLoss,
    /// 익절로 판다.
    TakeProfit,
    /// 청산한다. 어떤 멈춤에서도 받는다(E34 · P3). 템플릿의 손절 방식에 든 장 마감 청산 ·
    /// 최대 보유 기간(시간 손절)으로 걸어 둔 것이라 "걸어 둔 손절"로 읽는다(결정 2026-11-21 12번).
    Liquidation,
    /// 팀장이 팔기로 판단한 매도(4차). 방향은 팔기다. 전체 정지(E24 · F4 7b2) 중에는 익절처럼
    /// 막히고, 다른 상태에서는 청산과 같은 점검을 받는다(근거 · 가진 수량 · 열린 익절에 중복으로
    /// 막히지 않음 · 승인 없음).
    TeamExit,
}

impl Purpose {
    /// 이 목적의 주문 방향.
    pub fn side(self) -> Side {
        if self.is_buy() { Side::Buy } else { Side::Sell }
    }

    /// 사는 주문(새 진입 · 추가 매수)인가.
    pub fn is_buy(self) -> bool {
        matches!(self, Purpose::NewEntry | Purpose::AddOn)
    }
}

/// 주문에 쓰는 돈. 현금이 아니면 막는다(P5: 미수 · 신용 금지).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Funding {
    /// 가진 현금.
    Cash,
    /// 미수.
    Margin,
    /// 신용.
    Credit,
}

/// 상품 종류 표시. 주식과 일반 ETF만 받는다(P5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProductKind {
    /// 주식.
    Stock,
    /// 레버리지 · 인버스가 아닌 ETF.
    Etf,
    /// 레버리지 상품.
    Leveraged,
    /// 인버스 상품.
    Inverse,
    /// 파생 상품.
    Derivative,
    /// 표시를 모른다. 모르면 막는 쪽으로 본다.
    Unknown,
}

/// 근거의 칸 이름. store의 `RationaleField`와 1:1이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RationaleField {
    /// 가설.
    Hypothesis,
    /// 계기(트리거).
    Trigger,
    /// 들어간 값(진입가).
    Entry,
    /// 손절값.
    Stop,
    /// 목표값.
    Target,
    /// 기대값.
    ExpectedValue,
}

impl RationaleField {
    /// 모든 칸. 빠진 칸 목록은 이 순서로 낸다.
    pub const ALL: [RationaleField; 6] = [
        RationaleField::Hypothesis,
        RationaleField::Trigger,
        RationaleField::Entry,
        RationaleField::Stop,
        RationaleField::Target,
        RationaleField::ExpectedValue,
    ];
}

/// 매매의 근거(P9). 칸이 `None`이거나 글자 칸이 비었거나 공백뿐이면 빠진 것이다.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rationale {
    /// 가설.
    pub hypothesis: Option<String>,
    /// 계기.
    pub trigger: Option<String>,
    /// 진입가.
    pub entry: Option<Decimal>,
    /// 손절값(P2).
    pub stop: Option<Decimal>,
    /// 목표값.
    pub target: Option<Decimal>,
    /// 주당 기대값(비용을 빼기 전, 종목 통화). P13은 여기서 비용을 뺀다.
    pub expected_value: Option<Decimal>,
}

/// 판단에 쓴 자료 하나와 알게 된 시각(P10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceStamp {
    /// 자료 이름.
    pub name: String,
    /// 알게 된 시각. 판단 시각보다 늦으면 막는다(같은 것은 된다).
    pub known_at: DateTime<Utc>,
}

/// 주문 안(C9 입력).
#[derive(Debug, Clone, PartialEq)]
pub struct OrderDraft {
    /// 시장.
    pub market: Market,
    /// 종목.
    pub symbol: Symbol,
    /// 목적(방향도 여기서 정해진다).
    pub purpose: Purpose,
    /// 호가 유형.
    pub order_type: OrderType,
    /// AI가 낸 지정가. 시장가면 `None`.
    pub limit_price: Option<Decimal>,
    /// AI가 낸 수량. 규칙 엔진의 값과 다르면 작은 쪽이다(P7). `None`이면 규칙 엔진 값.
    pub ai_quantity: Option<Decimal>,
    /// 주문에 쓰는 돈(P5).
    pub funding: Funding,
    /// 상품 종류 표시(P5).
    pub product: ProductKind,
    /// 매매의 상태(E20).
    pub stage: TradeStage,
    /// 판단 시각(P10).
    pub decided_at: DateTime<Utc>,
    /// 근거(P9).
    pub rationale: Rationale,
    /// 판단에 쓴 자료(P10).
    pub sources: Vec<SourceStamp>,
}

/// 보유 종목 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    /// 시장.
    pub market: Market,
    /// 종목.
    pub symbol: Symbol,
    /// 수량.
    pub quantity: Decimal,
    /// 평균 단가.
    pub avg_price: Decimal,
    /// 지금 값(평가에 쓴다).
    pub last_price: Decimal,
}

/// 한 통화의 장부. 통화는 섞지 않는다(결정 31번).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    /// 통화.
    pub currency: Currency,
    /// 지금 평가액(현금 + 보유 평가, 실현과 평가 손익이 모두 들어 있다).
    pub equity: Decimal,
    /// 그날 시작 평가액(P3 · E23).
    pub day_start_equity: Decimal,
    /// 지금까지의 최고 평가액(P4 · E24).
    pub peak_equity: Decimal,
    /// 보유 종목들.
    pub positions: Vec<Position>,
}

/// 설정 파일의 표(결정 19번): 금지 종목과 업종표.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RulesConfig {
    /// 금지 종목(P5).
    pub banned_symbols: Vec<(Market, Symbol)>,
    /// 종목 → 업종(P8). 표에 없는 종목은 업종을 모르는 것이다(사는 주문을 막는다).
    pub sectors: HashMap<Symbol, String>,
}

/// 종목의 VI · 거래정지 표시(P12). `None`은 모름이고, 모르면 새 주문을 막는다(결정 19번).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolStatus {
    /// 변동성 완화장치가 걸려 있는가.
    pub vi: Option<bool>,
    /// 거래정지인가.
    pub halted: Option<bool>,
}

impl SymbolStatus {
    /// market이 준 거래 상태(공급자 값 또는 시세 추정, `pi_market::resolve_status`)를 점검의
    /// 표시로 바꾼다(결정 2026-11-21 5번).
    ///
    /// - `Normal` → `{ vi: Some(false), halted: Some(false) }`(통과).
    /// - `Vi` → `{ vi: Some(true), halted: None }`.
    /// - `Halted` → `{ vi: None, halted: Some(true) }`.
    /// - `Unknown`과 앞으로 더해질 모르는 변형 → `{ vi: None, halted: None }`(모르면 막는다).
    pub fn from_trading_status(status: TradingStatus) -> SymbolStatus {
        match status {
            TradingStatus::Normal => SymbolStatus {
                vi: Some(false),
                halted: Some(false),
            },
            TradingStatus::Vi => SymbolStatus {
                vi: Some(true),
                halted: None,
            },
            TradingStatus::Halted => SymbolStatus {
                vi: None,
                halted: Some(true),
            },
            _ => SymbolStatus {
                vi: None,
                halted: None,
            },
        }
    }
}

/// 자료와 연결의 상태(P11 · N6). 하나라도 나쁘면 새 주문(사기)을 막는다. 손절 · 청산은 받는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    /// 계좌가 기록과 맞는가.
    pub account_in_sync: bool,
    /// 바깥 API 오류가 있는가.
    pub api_error: bool,
    /// 자료 이상이 있는가.
    pub data_anomaly: bool,
}

/// API 요청 한도(P12). 센 값은 호출하는 쪽이 넘긴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiBudget {
    /// 지금 창에서 쓴 요청 수.
    pub used: u32,
    /// 창의 요청 한도. `used`가 이 값에 닿으면 막는다.
    pub limit: u32,
}

/// 점검에 필요한 바깥 값 전부. 시각도 여기서 받는다(시계를 읽지 않는다).
#[derive(Debug, Clone)]
pub struct CheckContext<'a> {
    /// 지금 시각.
    pub now: DateTime<Utc>,
    /// 템플릿 값. 상한은 [`pi_template::AbsoluteCaps::FIXED`]로 다시 막는다.
    pub params: &'a RiskParams,
    /// 설정 파일의 표.
    pub config: &'a RulesConfig,
    /// 그 시장의 숫자 규칙(호가 단위 · 가격제한 · 수수료 · 세금 · 미끄러짐 · 최대 주문 금액 · 세션).
    pub market_rules: &'a MarketRules,
    /// 그 시장의 달력(E18).
    pub calendar: &'a MarketCalendar,
    /// 그 종목의 시세.
    pub quote: &'a Quote,
    /// 늦은 시세 기준(N6).
    pub stale: StalePolicy,
    /// 가격제한폭의 기준가.
    pub reference_price: Decimal,
    /// VI · 거래정지 표시.
    pub status: SymbolStatus,
    /// 투자 팀 상태(E31 · E34).
    pub team: TeamStatus,
    /// 그 통화의 장부.
    pub book: &'a Book,
    /// 자료와 연결의 상태.
    pub health: Health,
    /// 아직 끝나지 않은 주문들(중복 점검).
    pub open_orders: &'a [ApprovedOrder],
    /// 오늘 이미 한 매매 횟수.
    pub trades_today: u32,
    /// API 요청 한도.
    pub api: ApiBudget,
}
