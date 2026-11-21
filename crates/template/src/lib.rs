//! pi-template: `docs/dev/modules/template.md` 를 따른다.
//!
//! 구성 요소 A6 가운데 **템플릿 값 · 절대 상한 · 승격 기준**(`dev.md` M3).
//!
//! - 템플릿은 원본(Hermes)의 "프로필 배포"와 같은 동작이다: 설정 묶음을 통째로 고르고
//!   ([`preset`]), 그 사본 위에서 값을 바꾼다([`adjust`]). 프리셋 자체는 바뀌지 않는다.
//! - 값과 상한은 원본에 없는 우리 고유 기능이다. 기본값의 출처는
//!   `docs/research/stock-investing-principles.md` 8.1 · 8.2 · 9장이며 **추정값(미확정)**이다.
//! - 순수 계산이다. 파일 · 네트워크 · 시계를 쓰지 않는다. 저장 · 승인 · 상태 전이 강제는
//!   다른 모듈(store · approval)이 한다.
//! - 돈과 비율은 십진수([`rust_decimal::Decimal`])로만 둔다. 비율 칸은 **퍼센트 단위**다
//!   (`2` = 2%).

mod caps;
pub mod error;
mod json;
mod params;
mod presets;
mod promotion;
mod risk;
mod state;

pub use caps::{AbsoluteCaps, CappedField, Change, adjust};
pub use error::{Error, Invalid, InvalidReason, OverCap, ParamField, Violation};
pub use json::{FORMAT_VERSION, Template};
pub use params::{
    ApprovalWait, AtrBar, AtrStop, Cooldown, CountRange, EntrySignal, LossStreakStop, Market,
    MaxHolding, PctRange, Rebalance, RiskParams, StopRule, Style, TakeProfitRule, TradingWindow,
    TriggerKind, UnfilledCancel, Universe, preset,
};
pub use promotion::{Criterion, PromotionCriteria, PromotionStats, Verdict};
pub use risk::{AdjustPlan, RiskField, increases_risk, plan_adjust, widened_fields};
pub use state::{ALLOWED_TRANSITIONS, TemplateState, Transition, TransitionReason};
