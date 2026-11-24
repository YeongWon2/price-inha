//! 승인 카드의 보이는 내용에 싣는 두 칸: 투표 수 · 한도 적용 전 → 후(5차, 결정 17 · additions.md
//! R4-4 · C24 · V55).
//!
//! 두 칸은 보여 주기용이다. 승인 지문(`subject.rs`)에는 들어가지 않는다 — 같은 주문이면 두 칸을
//! 싣기 전과 뒤의 지문이 글자 하나 다르지 않다. 승인 · 소비 · 만료 규칙도 그대로다.
//!
//! 한도 적용 기록은 부르는 쪽(server)이 rules의 `LimitClamp`를 [`ClampNote`]로 옮겨 넘긴다
//! (이 크레이트는 rules에 기대지 않는다).

use pi_store::{Store, TradeId};
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};

use crate::approvals::{Approvals, Ask, Requested};
use crate::error::Error;

/// 한도 적용 기록 한 줄: 어느 한도가 값을 얼마에서 얼마로 줄였나.
///
/// 요청 summary의 `"clamps"`에 `{"limit", "before", "after"}`로 실린다.
/// `before` · `after`는 [`Decimal::normalize`] 한 글자(주문 가격과 같은 모양)다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClampNote {
    /// 한도의 안정된 영문 키.
    pub limit: String,
    /// 한도를 적용하기 전 값.
    pub before: Decimal,
    /// 한도를 적용한 뒤 값.
    pub after: Decimal,
}

/// 요청의 보이는 내용에 더 싣는 것.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShownExtras {
    /// 이 주문이 나온 매매. 있으면 그 매매의 담당 의견(`Store::trade_opinions`)에서 강세 · 약세 ·
    /// 중립 · 기권 수를 세어 summary에 `"votes": {"bull", "bear", "neutral", "abstain"}`로 싣는다.
    /// 의견 기록이 없는 매매(옛 매매 · 옛 모양 답)이거나 `None`이면 `"votes"` 칸을 싣지 않는다.
    pub trade: Option<TradeId>,
    /// 적용한 순서의 한도 기록. 비면 `"clamps"` 칸을 싣지 않는다.
    pub clamps: Vec<ClampNote>,
}

impl Approvals {
    /// [`Approvals::request`]와 같고, 요청의 보이는 내용(summary)에 투표 수 · 한도 기록을 더 싣는다.
    ///
    /// - 두 칸은 주문 대상에만 싣는다(다른 대상은 `extras`를 무시한다).
    /// - 지문 · 기다릴 시간 · 만료 · 합치기 규칙은 [`Approvals::request`]와 같다. 같은 요청이 기다리는
    ///   중이라 합쳐지면 처음 만든 요청의 summary가 그대로다.
    /// - `extras.trade`가 없는 매매 번호면 저장소 오류([`Error::Store`])이고 요청은 만들지 않는다.
    pub fn request_with_extras(&self, ask: Ask, extras: ShownExtras) -> Result<Requested, Error> {
        self.request_shown(ask, |_| shown_fields(&self.store, &extras))
    }
}

/// summary에 더할 칸을 만든다.
fn shown_fields(store: &Store, extras: &ShownExtras) -> Result<Map<String, Value>, Error> {
    let mut fields = Map::new();
    if let Some(trade) = extras.trade {
        let opinions = store.trade_opinions(trade)?.opinions;
        if !opinions.is_empty() {
            fields.insert("votes".to_string(), count_votes(&opinions));
        }
    }
    if !extras.clamps.is_empty() {
        let notes: Vec<Value> = extras
            .clamps
            .iter()
            .map(|c| {
                json!({
                    "limit": c.limit,
                    "before": c.before.normalize().to_string(),
                    "after": c.after.normalize().to_string(),
                })
            })
            .collect();
        fields.insert("clamps".to_string(), Value::Array(notes));
    }
    Ok(fields)
}

/// 담당 의견에서 강세 · 약세 · 중립 · 기권 수를 센다. 기권은 중립과 따로 센다.
fn count_votes(opinions: &[pi_store::RoleOpinion]) -> Value {
    use pi_store::Stance;
    let (mut bull, mut bear, mut neutral, mut abstain) = (0u64, 0u64, 0u64, 0u64);
    for opinion in opinions {
        match (opinion.abstained, opinion.stance) {
            (true, _) | (false, None) => abstain += 1,
            (false, Some(Stance::Bullish)) => bull += 1,
            (false, Some(Stance::Bearish)) => bear += 1,
            (false, Some(Stance::Neutral)) => neutral += 1,
        }
    }
    json!({"bull": bull, "bear": bear, "neutral": neutral, "abstain": abstain})
}
