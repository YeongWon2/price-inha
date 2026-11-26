//! 승인 카드 보강(50차 · additions.md C24 · R4-4 · V55 · 결정 15).
//!
//! - 주문 승인 요청을 만들 때 규칙 엔진이 수량을 줄인 기록(`pi_rules::limit_clamps`)을 승인 모듈의
//!   [`ClampNote`]로 옮겨 넘긴다(투표 수는 승인 모듈이 그 매매의 담당 의견에서 센다). 두 칸은 보여 주기용이라
//!   지문에 들어가지 않는다.
//! - "후" 수량은 승인할 주문의 수량과 같아야 한다. 다르면 카드를 만들지 않는다(승인 확인 실패 쪽 — P19).
//! - 승인 카드(`GET /api/approvals/{id}` · `office.approval`)에는 요청의 summary에 든 두 칸을 그대로 꺼내 싣고,
//!   한도 줄마다 사람이 읽는 사유(`"reason"`)를 더한다: `"votes": {"bull", "bear", "neutral", "abstain"}`,
//!   `"clamps": [{"limit", "before", "after", "reason"}]`. 두 칸이 없는 요청이면 싣지 않는다.

use pi_approval::ClampNote;
use pi_rules::{LimitClamp, LimitKind};
use pi_store::ApprovalStatus;
use rust_decimal::Decimal;
use serde_json::{Map, Value};

use super::Trading;
use super::base::exec_text;
use crate::TradingError;

/// 한도의 안정된 영문 키.
fn limit_key(kind: LimitKind) -> &'static str {
    match kind {
        LimitKind::RiskPerTrade => "risk_per_trade",
        LimitKind::SymbolWeight => "symbol_weight",
        LimitKind::ConvictionWeight => "conviction_weight",
        LimitKind::SectorWeight => "sector_weight",
        LimitKind::AutoOrderLimit => "auto_order_limit",
        LimitKind::MinCash => "min_cash",
        LimitKind::MaxPositions => "max_positions",
    }
}

/// 한도 키 → 사람이 읽는 사유. 모르는 키면 `None`.
fn limit_reason(key: &str) -> Option<&'static str> {
    Some(match key {
        "risk_per_trade" => "1회 위험 한도",
        "symbol_weight" => "종목 비중 한도",
        "conviction_weight" => "확신 종목 비중 한도",
        "sector_weight" => "업종 비중 한도",
        "auto_order_limit" => "자동 주문 금액 한도",
        "min_cash" => "최소 현금",
        "max_positions" => "동시 보유 수",
        _ => return None,
    })
}

/// 규칙 엔진의 한도 기록을 승인 모듈의 기록으로 옮긴다. 마지막 "후"가 주문 수량과 다르면 오류다.
pub(super) fn clamp_notes(
    clamps: &[LimitClamp],
    quantity: Decimal,
) -> Result<Vec<ClampNote>, TradingError> {
    if let Some(last) = clamps.last()
        && last.after != quantity.trunc()
    {
        return Err(exec_text(
            "한도 적용 기록의 뒤 수량이 승인할 주문 수량과 다릅니다",
        ));
    }
    Ok(clamps
        .iter()
        .map(|c| ClampNote {
            limit: limit_key(c.limit).to_string(),
            before: c.before,
            after: c.after,
        })
        .collect())
}

impl Trading {
    /// 승인 카드 몸통(객체)에 그 요청의 투표 수 · 한도 기록을 더한다. 읽지 못하면 몸통 그대로(기록만 남긴다).
    pub(crate) fn with_approval_extras(&self, number: &str, mut body: Value) -> Value {
        match self.approval_extras(number) {
            Ok(extras) => {
                if let Value::Object(map) = &mut body {
                    map.extend(extras);
                }
            }
            Err(error) => {
                tracing::warn!(%error, "승인 카드의 투표 수 · 한도 기록을 읽지 못했습니다")
            }
        }
        body
    }

    /// 그 요청의 summary에 든 `votes` · `clamps`(한도 줄마다 사유를 더한 것).
    fn approval_extras(&self, number: &str) -> Result<Map<String, Value>, TradingError> {
        let mut out = Map::new();
        let Some(summary) = self.request_summary(number)? else {
            return Ok(out);
        };
        if let Some(votes) = summary.get("votes").filter(|v| v.is_object()) {
            out.insert("votes".to_string(), votes.clone());
        }
        if let Some(Value::Array(list)) = summary.get("clamps") {
            let clamps: Vec<Value> = list.iter().map(with_reason).collect();
            out.insert("clamps".to_string(), Value::Array(clamps));
        }
        Ok(out)
    }

    /// 승인 번호의 요청 summary(저장소 그대로). 없으면 `None`.
    fn request_summary(&self, number: &str) -> Result<Option<Value>, TradingError> {
        for status in ApprovalStatus::ALL {
            let found = self
                .store
                .approvals_in_status(status)?
                .into_iter()
                .find(|row| row.summary.get("number").and_then(Value::as_str) == Some(number));
            if let Some(row) = found {
                return Ok(Some(row.summary));
            }
        }
        Ok(None)
    }
}

/// 한도 줄 하나에 사유를 더한다(모르는 키는 키 그대로).
fn with_reason(clamp: &Value) -> Value {
    let mut line = clamp.clone();
    let key = clamp
        .get("limit")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let reason = limit_reason(key).map_or_else(|| key.to_string(), str::to_string);
    if let Value::Object(map) = &mut line {
        map.insert("reason".to_string(), Value::String(reason));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 한도_키마다_사유가_있다() {
        for kind in [
            LimitKind::RiskPerTrade,
            LimitKind::SymbolWeight,
            LimitKind::ConvictionWeight,
            LimitKind::SectorWeight,
            LimitKind::AutoOrderLimit,
            LimitKind::MinCash,
            LimitKind::MaxPositions,
        ] {
            assert!(limit_reason(limit_key(kind)).is_some(), "{kind:?}");
        }
    }

    #[test]
    fn 뒤_수량이_주문과_다르면_옮기지_않는다() {
        let clamp = LimitClamp {
            limit: LimitKind::SymbolWeight,
            before: Decimal::from(15),
            after: Decimal::from(10),
        };
        assert!(clamp_notes(std::slice::from_ref(&clamp), Decimal::from(9)).is_err());
        let notes = clamp_notes(&[clamp], Decimal::from(10)).unwrap();
        assert_eq!(notes[0].limit, "symbol_weight");
        assert!(clamp_notes(&[], Decimal::from(3)).unwrap().is_empty());
    }
}
