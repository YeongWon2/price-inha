//! (28차 1 · 2번, C29 · V82 · F5-8) 규칙 읽기와 개선안의 앞 → 뒤 값.
//!
//! - 칸은 `pi_template::template_view`, 앞 → 뒤 차이는 `pi_template::diff_params`가 만든다. 여기서는 모아서 낸다.
//! - 모양은 [`super::RULES_VIEW_PATH`] · [`super::PROPOSALS_PATH`]의 문서에 있다.
//! - 사용자 변경 개선안(실전중 템플릿의 위험을 키우는 값 바꾸기, 21차)도 복기 개선안과 같이 규칙을 바꾸므로
//!   `cooling` · `recent`에 든다. 그 앞 → 뒤는 반영됐으면 그 개선안으로 생긴 판과 앞 판, 아니면 그 템플릿의
//!   마지막 판과 개선안의 값이다.

use std::cmp::Reverse;

use pi_journal::Improvement;
use pi_store::{Proposal, ProposalSource, ProposalState, TemplateId};
use pi_template::{RiskParams, Style, Template, template_view};
use serde_json::{Map, Value, json};

use super::{Crew, RULES_RECENT_LIMIT};
use crate::TeamError;
use crate::trading::change_json;

/// 냉각중 개선안이 반영되는 때(E28): 다음 거래일 시작.
const APPLIES_ON: &str = "next_trading_day";

/// 템플릿 값(JSON)의 RiskParams. 읽을 수 없으면 없음.
fn params_in(value: &Value) -> Option<RiskParams> {
    Template::from_json(&value.to_string())
        .ok()
        .map(|parsed| parsed.params)
}

/// 앞 → 뒤 값 차이(템플릿 판 이력의 `changes`와 같은 모양).
fn diff(before: &RiskParams, after: &RiskParams) -> Vec<Value> {
    pi_template::diff_params(before, after)
        .into_iter()
        .map(change_json)
        .collect()
}

/// 원칙의 절대 상한: 상한이 있는 칸의 키 → 상한(십진수 글자). 칸의 상한은 값과 상관없이 늘 같다.
fn caps() -> Map<String, Value> {
    template_view(&Template::new(Style::Swing).params)
        .into_iter()
        .filter_map(|field| {
            let cap = field.cap?;
            Some((field.key.to_string(), json!(cap.to_string())))
        })
        .collect()
}

/// 지금 규칙의 칸(키 · 값 · 상한). 값이 없으면 빈 목록.
fn fields(params: Option<&RiskParams>) -> Vec<Value> {
    params
        .map(template_view)
        .unwrap_or_default()
        .into_iter()
        .map(|field| {
            json!({
                "key": field.key,
                "value": field.value,
                "cap": field.cap.map(|cap| cap.to_string()),
            })
        })
        .collect()
}

impl Crew {
    /// 규칙 읽기([`super::RULES_VIEW_PATH`]). 아무것도 바꾸지 않는다.
    pub(super) fn rules_view(&self) -> Result<Value, TeamError> {
        let in_use = self.trading.in_use()?;
        let template = in_use.as_ref().map(|(id, _)| id.0);
        let params = in_use.as_ref().and_then(|(_, params)| params.as_ref());

        let mut cooling = self.store.proposals_in_state(ProposalState::Cooling)?;
        cooling.sort_by_key(|proposal| proposal.id);
        let cooling: Vec<Value> = cooling
            .iter()
            .map(|proposal| {
                json!({
                    "proposal": proposal.id.0,
                    "changes": self.proposal_changes(proposal),
                    "applies_on": APPLIES_ON,
                })
            })
            .collect();

        let mut applied = self.store.proposals_in_state(ProposalState::Applied)?;
        applied.sort_by_key(|proposal| Reverse((proposal.applied_at, proposal.id)));
        let recent: Vec<Value> = applied
            .iter()
            .take(RULES_RECENT_LIMIT)
            .map(|proposal| {
                json!({
                    "proposal": proposal.id.0,
                    "applied_at": proposal.applied_at.map(|at| at.0),
                    "changes": self.proposal_changes(proposal),
                })
            })
            .collect();

        Ok(json!({
            "template": template,
            "fields": fields(params),
            "caps": caps(),
            "cooling": cooling,
            "recent": recent,
        }))
    }

    /// 개선안의 앞 → 뒤 값 차이. 읽을 수 없으면 빈 목록.
    pub(super) fn proposal_changes(&self, proposal: &Proposal) -> Vec<Value> {
        match self.store.proposal_source(proposal.id) {
            Ok(ProposalSource::Review) => Improvement::from_proposal(proposal)
                .map(|improvement| diff(&improvement.before, &improvement.after))
                .unwrap_or_default(),
            Ok(ProposalSource::User { template }) => {
                self.user_changes(proposal, template).unwrap_or_default()
            }
            Err(error) => {
                tracing::warn!(%error, "개선안의 출처를 읽지 못했습니다");
                Vec::new()
            }
        }
    }

    /// 사용자 변경 개선안의 앞 → 뒤: 반영됐으면 그 개선안으로 생긴 판과 앞 판, 아니면 그 템플릿의 마지막 판과
    /// 개선안의 값(바꾼 뒤의 템플릿 값).
    fn user_changes(&self, proposal: &Proposal, template: TemplateId) -> Option<Vec<Value>> {
        let history = self.store.template_history(template).ok()?;
        let made = history
            .iter()
            .position(|line| line.proposal == Some(proposal.id));
        let (before, after) = match made {
            Some(at) => (
                params_in(&history.get(at.checked_sub(1)?)?.value)?,
                params_in(&history[at].value)?,
            ),
            None => (
                params_in(&history.last()?.value)?,
                params_in(&proposal.content)?,
            ),
        };
        Some(diff(&before, &after))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 절대_상한은_여섯_칸이고_글자로_나간다() {
        let caps = caps();
        assert_eq!(caps.len(), 6, "{caps:?}");
        assert!(caps.values().all(Value::is_string), "{caps:?}");
    }

    #[test]
    fn 값이_없으면_칸도_없고_있으면_편집_여부_없이_낸다() {
        assert!(fields(None).is_empty());
        let params = Template::new(Style::Swing).params;
        let shown = fields(Some(&params));
        assert_eq!(shown.len(), template_view(&params).len());
        assert!(shown[0].get("editable").is_none(), "{}", shown[0]);
    }

    #[test]
    fn 같은_값이면_바뀐_칸이_없다() {
        let params = Template::new(Style::Swing).params;
        assert!(
            diff(&params, &params)
                .iter()
                .all(|change| change["before"] == change["after"])
        );
    }
}
