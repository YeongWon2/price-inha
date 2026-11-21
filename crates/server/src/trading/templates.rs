//! 템플릿 고르기(C4)와 값 맞추기(C14 · E5). 값과 상한은 `pi_template`이, 판과 상태는 저장소가 맡는다.

use pi_store::{NewTemplate, StoreError, TemplateId, TemplateState};
use pi_template::{CappedField, Change, ParamField, Style, Template, adjust};
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};

use super::Trading;
use super::base::millis;

/// 템플릿 경로의 거부.
pub(super) enum Refusal {
    /// 몸통을 읽을 수 없거나 모르는 칸 · 값(400 `invalid_input`).
    Invalid,
    /// 그 번호의 템플릿이 없다(404).
    NotFound,
    /// 모의중이 아니라 값을 바꿀 수 없다(409).
    Locked,
    /// 절대 상한 · 아래쪽 경계(400 [`super::TEMPLATE_OVER_CAP`]). 어긴 칸의 받는 이름들.
    OverCap(Vec<&'static str>),
    /// 저장소 오류(500).
    Store(StoreError),
}

impl From<StoreError> for Refusal {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::TemplateNotFound(..) => Refusal::NotFound,
            StoreError::TemplateLocked { .. } => Refusal::Locked,
            other => Refusal::Store(other),
        }
    }
}

fn state_word(state: TemplateState) -> &'static str {
    match state {
        TemplateState::Simulating => "simulating",
        TemplateState::PromotionPending => "promotion_pending",
        TemplateState::Live => "live",
        TemplateState::Stopped => "stopped",
    }
}

/// 받는 칸 이름 → 바꾸기 요청([`crate::TEMPLATE_ADJUST_PATH`]와 복기 담당의 규칙 바꾸기가 같이 쓴다).
pub(crate) fn change_of(field: &str, value: Decimal) -> Option<Change> {
    Some(match field {
        "risk_per_trade_pct" => Change::RiskPerTradePct(value),
        "daily_loss_limit_pct" => Change::DailyLossLimitPct(value),
        "max_drawdown_pct" => Change::MaxDrawdownPct(value),
        "symbol_weight_pct" => Change::SymbolWeightPct(value),
        "sector_weight_pct" => Change::SectorWeightPct(value),
        "min_cash_pct" => Change::MinCashPct(value),
        "auto_order_limit_pct" => Change::AutoOrderLimitPct(value),
        _ => return None,
    })
}

fn capped_name(field: CappedField) -> &'static str {
    match field {
        CappedField::RiskPerTrade => "risk_per_trade_pct",
        CappedField::DailyLossLimit => "daily_loss_limit_pct",
        CappedField::MaxDrawdown => "max_drawdown_pct",
        CappedField::SymbolWeight => "symbol_weight_pct",
        CappedField::ConvictionWeight => "conviction_weight_pct",
        CappedField::SectorWeight => "sector_weight_pct",
    }
}

fn param_name(field: ParamField) -> &'static str {
    match field {
        ParamField::RiskPerTrade => "risk_per_trade_pct",
        ParamField::DailyLossLimit => "daily_loss_limit_pct",
        ParamField::MaxDrawdown => "max_drawdown_pct",
        ParamField::SymbolWeight => "symbol_weight_pct",
        ParamField::SectorWeight => "sector_weight_pct",
        ParamField::MinCash => "min_cash_pct",
        ParamField::AutoOrderLimit => "auto_order_limit_pct",
        _ => "other",
    }
}

fn changes_of(changes: &Map<String, Value>) -> Option<Vec<Change>> {
    changes
        .iter()
        .map(|(field, value)| {
            let value: Decimal = value.as_str()?.trim().parse().ok()?;
            change_of(field, value)
        })
        .collect()
}

fn template_value(template: &Template) -> Result<Value, Refusal> {
    serde_json::from_str(&template.to_json()).map_err(|_| Refusal::Invalid)
}

impl Trading {
    /// 그 스타일의 기본값으로 새 템플릿을 만든다(모의중). 몸통 `{"style"}`.
    pub(super) fn choose_template(&self, body: &Value) -> Result<Value, Refusal> {
        let style: Style =
            serde_json::from_value(body["style"].clone()).map_err(|_| Refusal::Invalid)?;
        let value = template_value(&Template::new(style))?;
        let at = millis(self.clock.now());
        let saved = self.store.create_template(NewTemplate { value, at })?;
        Ok(json!({
            "id": saved.id.0,
            "style": body["style"],
            "state": state_word(saved.state),
        }))
    }

    /// 템플릿 값을 맞춘다. 몸통 `{"id", "changes": {칸: 십진수 글자}}`.
    pub(super) fn adjust_template(&self, body: &Value) -> Result<Value, Refusal> {
        let id = TemplateId(body["id"].as_i64().ok_or(Refusal::Invalid)?);
        let changes = body["changes"].as_object().ok_or(Refusal::Invalid)?;
        let changes = changes_of(changes).ok_or(Refusal::Invalid)?;
        let saved = self.store.get_template(id)?;
        let current =
            Template::from_json(&saved.value.to_string()).map_err(|_| Refusal::Invalid)?;
        let params = adjust(&current.params, &changes).map_err(|over| {
            let mut fields: Vec<&'static str> = over
                .violations
                .iter()
                .map(|v| capped_name(v.field))
                .collect();
            fields.extend(over.invalid.iter().map(|i| param_name(i.field)));
            fields.dedup();
            Refusal::OverCap(fields)
        })?;
        let next = Template { params, ..current };
        let value = template_value(&next)?;
        let revised = self
            .store
            .revise_template(id, &value, millis(self.clock.now()))?;
        Ok(json!({
            "id": revised.id.0,
            "revision": revised.revision,
            "state": state_word(revised.state),
        }))
    }
}
