//! 근거 카드 칸(50차 · additions.md C23 · D13 · E53 · R6-4 · R6-5 · V68 · V69).
//!
//! 매매 하나의 담당 의견 · 팀장 요약(`pi_store::Store::trade_opinions`)을 화면이 그대로 그릴 칸으로 만든다.
//! 세기(찬반 수) · 정렬(팀 차례)은 여기서 하고 화면은 받은 차례대로 그린다. 근거 경로(`GET /api/trades/{id}/basis`)와
//! 히스토리의 매매 하나(`GET /api/trades/{id}`의 `basis`)가 같은 칸을 쓴다.
//!
//! - `"opinions"`: `[{"agent", "name", "score", "direction": "bull" | "bear" | "neutral" | null,
//!   "confidence": "low" | "mid" | "high" | null, "reason", "horizon_days": 수 | null, "abstained"}]` — 팀 차례
//!   (뉴스 → 전략 → 리스크 → 복기 → 매매 → 팀장, 모르는 담당은 뒤에 남은 차례로). 기권은 방향 · 확신도가 없다.
//! - `"leader"`: `{"decision": "buy" | "sell" | "watch", "summary", "flip_condition"}` | null(기록 없음).
//!   결정은 매매 계획의 방향이고 방향이 없으면 `watch`다.
//! - `"votes"`: `{"bull", "bear", "neutral", "abstain"}` — 기권은 중립과 따로 센다(승인 카드의 투표 수와 같은 셈).
//! - 칸이 생기기 전 매매는 빈 목록 · null · 0이다. 빈 칸을 추정해 채우지 않는다(E53).

use pi_agent::Role;
use pi_store::{Conviction, RoleOpinion, Stance, Trade, TradeOpinions, TradeSide};
use serde_json::{Map, Value, json};

use super::build::role_name;

/// 근거 카드에 더하는 칸(`opinions` · `leader` · `votes`). 글은 `hide`로 가린 뒤 싣는다.
pub(crate) fn card_fields(
    trade: &Trade,
    side: Option<TradeSide>,
    record: &TradeOpinions,
    hide: &dyn Fn(&str) -> String,
) -> Map<String, Value> {
    let mut sorted: Vec<&RoleOpinion> = record.opinions.iter().collect();
    sorted.sort_by_key(|o| team_order(&o.role));
    let opinions: Vec<Value> = sorted
        .into_iter()
        .map(|o| opinion_line(trade, o, hide))
        .collect();
    let leader = record.summary.as_ref().map(|s| {
        json!({
            "decision": decision_word(side),
            "summary": hide(&s.summary),
            "flip_condition": hide(&s.reversal),
        })
    });
    let mut fields = Map::new();
    fields.insert("opinions".to_string(), Value::Array(opinions));
    fields.insert("leader".to_string(), leader.unwrap_or(Value::Null));
    fields.insert("votes".to_string(), votes(&record.opinions));
    fields
}

/// 담당의 팀 차례. 모르는 담당은 뒤로(정렬이 안정적이라 받은 차례가 남는다).
fn team_order(role: &str) -> usize {
    Role::from_id(role)
        .and_then(|role| Role::ALL.iter().position(|r| *r == role))
        .unwrap_or(usize::MAX)
}

fn opinion_line(trade: &Trade, o: &RoleOpinion, hide: &dyn Fn(&str) -> String) -> Value {
    let name = Role::from_id(&o.role).map_or_else(|| o.role.clone(), |r| role_name(r).to_string());
    let score = trade
        .scores
        .iter()
        .find(|s| s.role == o.role)
        .map(|s| s.score);
    let (direction, confidence) = if o.abstained {
        (None, None)
    } else {
        (o.stance.map(stance_word), o.conviction.map(conviction_word))
    };
    json!({
        "agent": o.role,
        "name": name,
        "score": score,
        "direction": direction,
        "confidence": confidence,
        "reason": hide(&o.reason),
        "horizon_days": o.horizon_days,
        "abstained": o.abstained,
    })
}

fn stance_word(stance: Stance) -> &'static str {
    match stance {
        Stance::Bullish => "bull",
        Stance::Bearish => "bear",
        Stance::Neutral => "neutral",
    }
}

fn conviction_word(conviction: Conviction) -> &'static str {
    match conviction {
        Conviction::Low => "low",
        Conviction::Medium => "mid",
        Conviction::High => "high",
    }
}

fn decision_word(side: Option<TradeSide>) -> &'static str {
    match side {
        Some(TradeSide::Buy) => "buy",
        Some(TradeSide::Sell) => "sell",
        None => "watch",
    }
}

/// 강세 · 약세 · 중립 · 기권 수. 기권(또는 방향 없음)은 중립과 따로 센다.
fn votes(opinions: &[RoleOpinion]) -> Value {
    let (mut bull, mut bear, mut neutral, mut abstain) = (0u32, 0u32, 0u32, 0u32);
    for o in opinions {
        match (o.abstained, o.stance) {
            (true, _) | (false, None) => abstain += 1,
            (false, Some(Stance::Bullish)) => bull += 1,
            (false, Some(Stance::Bearish)) => bear += 1,
            (false, Some(Stance::Neutral)) => neutral += 1,
        }
    }
    json!({"bull": bull, "bear": bear, "neutral": neutral, "abstain": abstain})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(role: &str, stance: Option<Stance>) -> RoleOpinion {
        RoleOpinion {
            role: role.to_string(),
            stance,
            conviction: stance.map(|_| Conviction::Medium),
            reason: String::new(),
            horizon_days: None,
            abstained: stance.is_none(),
        }
    }

    #[test]
    fn 모르는_담당은_팀_차례_뒤에_남는다() {
        assert!(team_order("scout") < team_order("leader"));
        assert_eq!(team_order("누구"), usize::MAX);
    }

    #[test]
    fn 기권은_중립과_따로_센다() {
        let list = [
            line("scout", None),
            line("risk", Some(Stance::Neutral)),
            line("strategy", Some(Stance::Bullish)),
        ];
        assert_eq!(
            votes(&list),
            json!({"bull": 1, "bear": 0, "neutral": 1, "abstain": 1})
        );
        assert_eq!(decision_word(None), "watch");
    }
}
