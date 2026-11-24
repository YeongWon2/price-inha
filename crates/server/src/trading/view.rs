//! (27차 1 · 2번, C28 · E58 · F6-9a) 템플릿 화면 읽기와 실전 템플릿 멈추기.
//!
//! - 칸은 `pi_template::template_view`, 판 이력은 저장소의 `template_history`와 앞 판과의
//!   `pi_template::diff_params`, 승격 기준 줄은 `pi_template::promotion_lines`가 만든다. 여기서는 모아서 낸다.
//! - 승격 기준 줄의 모의 결과는 장 마감 뒤 승격 판정이 쓰는 것과 같은 길([`Trading::promotion_stats`])로 얻는다.
//! - 멈추기는 저장소의 사용자 멈춤 전이(`pi_store::Store::stop_template`)다. 승인을 거치지 않는다.
//! - 모양은 [`super::TEMPLATES_VIEW_PATH`] · [`super::TEMPLATE_STOP_PATH`]의 문서에 있다.

use pi_approval::Subject;
use pi_store::{ApprovalStatus, StoreError, TemplateId, TemplateState};
use pi_template::{CriterionLine, FieldChange, RiskParams, ViewField};
use serde_json::{Value, json};

use super::Trading;
use super::base::millis;
use super::chosen::STATES;
use crate::TradingError;

/// 멈추기의 결과.
pub(super) enum Stopped {
    /// 실전중 → 멈춤이 됐다.
    Done(TemplateId),
    /// 실전중이 아니라 그대로다(지금 상태 글자).
    NotLive(&'static str),
    /// 그 번호의 템플릿이 없다.
    Missing,
}

/// 상태 글자(템플릿 경로들과 같은 글자).
pub(super) fn state_word(state: TemplateState) -> &'static str {
    match state {
        TemplateState::Simulating => "simulating",
        TemplateState::PromotionPending => "promotion_pending",
        TemplateState::Live => "live",
        TemplateState::Stopped => "stopped",
    }
}

/// 값을 넣을 수 있는 상태인가(27차 보강): 모의중 · 실전중은 참, 승격대기 · 멈춤은 잠겨 거짓.
fn editable_in(state: TemplateState) -> bool {
    matches!(state, TemplateState::Simulating | TemplateState::Live)
}

fn parsed_of(value: &Value) -> Option<pi_template::Template> {
    pi_template::Template::from_json(&value.to_string()).ok()
}

fn field_json(field: ViewField, editable: bool) -> Value {
    json!({
        "key": field.key,
        "value": field.value,
        "cap": field.cap.map(|cap| cap.to_string()),
        "editable": field.editable && editable,
    })
}

/// 판 이력 · 개선안의 바뀐 칸 하나(`{"key", "before", "after", "widened"}`).
pub(crate) fn change_json(change: FieldChange) -> Value {
    json!({
        "key": change.key,
        "before": change.before,
        "after": change.after,
        "widened": change.widened,
    })
}

fn line_json(line: CriterionLine) -> Value {
    json!({
        "criterion": serde_json::to_value(line.criterion).unwrap_or(Value::Null),
        "need": line.need.to_string(),
        "have": line.have.map(|have| have.to_string()),
        "met": line.met,
    })
}

impl Trading {
    /// 템플릿 화면 읽기(C28). 아무것도 바꾸지 않는다.
    pub(super) async fn templates_view(&self) -> Result<Value, TradingError> {
        let mut all = Vec::new();
        for state in STATES {
            all.extend(self.store.templates_in_state(state)?);
        }
        all.sort_by_key(|template| template.id);
        let current = all.last().map(|template| template.id.0);
        let mut templates = Vec::with_capacity(all.len());
        for template in &all {
            templates.push(self.template_entry(template).await?);
        }
        Ok(json!({ "current": current, "templates": templates }))
    }

    /// 템플릿 하나의 보기.
    async fn template_entry(&self, template: &pi_store::Template) -> Result<Value, TradingError> {
        let parsed = parsed_of(&template.value);
        let editable = editable_in(template.state);
        let fields: Vec<Value> = parsed
            .as_ref()
            .map(|parsed| pi_template::template_view(&parsed.params))
            .unwrap_or_default()
            .into_iter()
            .map(|field| field_json(field, editable))
            .collect();
        let promotion: Vec<Value> = match &parsed {
            Some(parsed) => {
                let stats = self.promotion_stats(template).await?;
                pi_template::promotion_lines(&parsed.params.promotion, &stats)
                    .into_iter()
                    .map(line_json)
                    .collect()
            }
            None => Vec::new(),
        };
        let approval = if template.state == TemplateState::PromotionPending {
            self.waiting_promotion(template.id)?
        } else {
            None
        };
        Ok(json!({
            "id": template.id.0,
            "style": parsed.as_ref().map(|parsed| serde_json::to_value(parsed.style).unwrap_or(Value::Null)),
            "state": state_word(template.state),
            "revision": template.revision,
            "fields": fields,
            "revisions": self.revisions_of(template.id)?,
            "promotion": promotion,
            "approval": approval,
        }))
    }

    /// 판 이력: 판마다 앞 판과의 차이(첫 판은 빈 목록).
    fn revisions_of(&self, id: TemplateId) -> Result<Vec<Value>, TradingError> {
        let mut before: Option<RiskParams> = None;
        let mut out = Vec::new();
        for line in self.store.template_history(id)? {
            let after = parsed_of(&line.value).map(|parsed| parsed.params);
            let changes: Vec<Value> = match (&before, &after) {
                (Some(before), Some(after)) => pi_template::diff_params(before, after)
                    .into_iter()
                    .map(change_json)
                    .collect(),
                _ => Vec::new(),
            };
            out.push(json!({
                "revision": line.revision,
                "at": line.at.0,
                "changes": changes,
                "proposal": line.proposal.map(|proposal| proposal.0),
            }));
            before = after;
        }
        Ok(out)
    }

    /// 그 템플릿의 기다리는 실전 승격 승인 번호.
    fn waiting_promotion(&self, id: TemplateId) -> Result<Option<String>, TradingError> {
        let requests = self.promotion_requests()?;
        Ok(requests
            .into_iter()
            .filter(|view| view.status == ApprovalStatus::Pending)
            .find(|view| matches!(view.subject, Subject::LivePromotion { template, .. } if template == id))
            .map(|view| view.number.as_str().to_string()))
    }

    /// 실전 템플릿 멈추기(F6-9a · E58): 실전중이면 멈춤으로, 그 밖은 그대로.
    pub(super) fn stop_live_template(&self, id: TemplateId) -> Result<Stopped, TradingError> {
        let template = match self.store.get_template(id) {
            Ok(template) => template,
            Err(StoreError::TemplateNotFound(..)) => return Ok(Stopped::Missing),
            Err(error) => return Err(error.into()),
        };
        if template.state != TemplateState::Live {
            return Ok(Stopped::NotLive(state_word(template.state)));
        }
        match self.store.stop_template(id, millis(self.clock.now())) {
            Ok(stopped) => Ok(Stopped::Done(stopped.id)),
            // 그 사이 다른 길(전체 정지 등)이 먼저 옮겼다: 지금 상태로 답한다.
            Err(StoreError::TransitionDenied { .. }) => {
                let now = self.store.get_template(id)?;
                Ok(Stopped::NotLive(state_word(now.state)))
            }
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 모의중과_실전중만_값을_넣을_수_있다() {
        assert!(editable_in(TemplateState::Simulating));
        assert!(editable_in(TemplateState::Live));
        assert!(!editable_in(TemplateState::PromotionPending));
        assert!(!editable_in(TemplateState::Stopped));
    }

    #[test]
    fn 상한은_글자로_나가고_잠긴_상태면_바꿀_수_없다() {
        let params = pi_template::Template::new(pi_template::Style::Swing).params;
        let first = pi_template::template_view(&params).remove(0);
        let shown = field_json(first, false);
        assert_eq!(shown["key"], json!("risk_per_trade_pct"));
        assert!(shown["cap"].is_string(), "{shown}");
        assert_eq!(shown["editable"], json!(false));
    }
}
