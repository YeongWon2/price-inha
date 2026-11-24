//! (10차 10a 보강 4 · 6번) "쓰는 템플릿"을 고르는 규칙을 두는 한 곳.
//!
//! - 쓰는 템플릿은 **가장 나중에 고른 템플릿 하나**다. 상태(모의중 · 승격대기 · 실전중 · 멈춤)를 가리지 않는다:
//!   전체 정지로 실전중 템플릿이 멈춤이 돼도 그것이 "마지막으로 쓰던 템플릿"이고, 더 오래된 다른 템플릿으로
//!   조용히 넘어가지 않는다.
//! - 파는 주문(손절 · 청산 · 익절 · 팀장 팔기), 손절 감시, 손실 · 멈춤 판단은 그 템플릿의 값으로 한다 — 멈춤이어도
//!   (결정 2026-11-21 12번 · F4 "멈춤 상태에서도 손절은 계속").
//! - 사는 주문은 그 템플릿이 쓸 수 있는 상태(모의중 · 승격대기 · 실전중)일 때만 그 값으로 낸다. 멈춤이면 고른
//!   템플릿이 없는 것과 같다(새로 골라야 한다).
//! - 값은 모의중일 때만 바꿀 수 있다(V21). 그 밖의 상태는 잠긴 것이고, 개선안 반영은 쓰는 템플릿에만 한다.

use pi_rules::Purpose;
use pi_store::{TemplateId, TemplateState};
use pi_template::{RiskParams, Template, TriggerKind};

use super::Trading;
use super::base::millis;
use crate::TradingError;

/// (10차 10b 0번 다) 매수가 막힌 까닭 안내: 쓰는 템플릿이 멈춤이거나 없다. 다시 시작 응답 · 팀 상태 화면 ·
/// `doctor`가 같은 글을 쓴다.
pub(crate) const CHOOSE_TEMPLATE_NOTICE: &str =
    "쓰는 템플릿이 없거나 멈춰 있어 사는 주문이 나가지 않습니다. 새 템플릿을 고르세요";

/// 상태를 가리지 않고 모두 읽는다.
pub(super) const STATES: [TemplateState; 4] = [
    TemplateState::Simulating,
    TemplateState::PromotionPending,
    TemplateState::Live,
    TemplateState::Stopped,
];

/// 그 템플릿의 값. 읽을 수 없으면 없음.
fn params_of(template: &pi_store::Template) -> Option<RiskParams> {
    Template::from_json(&template.value.to_string())
        .ok()
        .map(|parsed| parsed.params)
}

impl Trading {
    /// 쓰는 템플릿: 가장 나중에 고른 것(상태를 가리지 않는다). 하나도 없으면 없음.
    pub(super) fn template_in_use(&self) -> Result<Option<pi_store::Template>, TradingError> {
        let mut found = Vec::new();
        for state in STATES {
            found.extend(self.store.templates_in_state(state)?);
        }
        Ok(found.into_iter().max_by_key(|template| template.id))
    }

    /// (10b-2, 결정 19) 쓰는 템플릿의 구독(`RiskParams.triggers`): 무엇이 팀을 깨우는가. 쓰는 템플릿이 없거나
    /// 멈춤이면(사지 않으므로) 없음 — 아무 깨우기도 걸지 않는다. 손절 · 청산 감시는 이것과 상관없다.
    pub(crate) fn wake_subscriptions(&self) -> Result<Option<Vec<TriggerKind>>, TradingError> {
        let Some(template) = self.template_in_use()? else {
            return Ok(None);
        };
        if template.state == TemplateState::Stopped {
            return Ok(None);
        }
        Ok(params_of(&template).map(|params| params.triggers))
    }

    /// 쓰는 템플릿이 실전중인가. 실전 주문은 만들지 않으므로 그 템플릿의 사는 주문은 나가지 않는다(E25).
    pub(super) fn live_in_use(&self) -> Result<bool, TradingError> {
        let template = self.template_in_use()?;
        Ok(template.is_some_and(|template| template.state == TemplateState::Live))
    }

    /// (28차 1번) 쓰는 템플릿의 번호와 값(값을 읽을 수 없으면 값은 없음). 하나도 없으면 없음.
    pub(crate) fn in_use(&self) -> Result<Option<(TemplateId, Option<RiskParams>)>, TradingError> {
        Ok(self
            .template_in_use()?
            .map(|template| (template.id, params_of(&template))))
    }

    /// 쓰는 템플릿의 값(멈춤이어도 — 파는 주문 · 손절 감시 · 손실 판단 · 복기가 쓴다). 없거나 읽을 수 없으면 없음.
    pub(crate) fn params(&self) -> Result<Option<RiskParams>, TradingError> {
        Ok(self.template_in_use()?.as_ref().and_then(params_of))
    }

    /// 그 목적의 주문에 쓸 값: 파는 주문은 [`Trading::params`], 사는 주문은 쓰는 템플릿이 쓸 수 있는 상태일 때만.
    pub(super) fn params_for(&self, purpose: Purpose) -> Result<Option<RiskParams>, TradingError> {
        let Some(template) = self.template_in_use()? else {
            return Ok(None);
        };
        if purpose.is_buy() && template.state == TemplateState::Stopped {
            return Ok(None);
        }
        Ok(params_of(&template))
    }

    /// (10b 0번 다) 쓰는 템플릿이 멈춤이거나 없으면 그 안내([`CHOOSE_TEMPLATE_NOTICE`]). 읽지 못하면 없음.
    pub(super) fn template_notice(&self) -> Option<&'static str> {
        match self.template_in_use() {
            Ok(None) => Some(CHOOSE_TEMPLATE_NOTICE),
            Ok(Some(template)) if template.state == TemplateState::Stopped => {
                Some(CHOOSE_TEMPLATE_NOTICE)
            }
            Ok(Some(_)) => None,
            Err(error) => {
                tracing::warn!(%error, "쓰는 템플릿을 읽지 못했습니다");
                None
            }
        }
    }

    /// 쓰는 템플릿의 값이 잠겨 있는가(모의중이 아니다: 승격대기 · 실전중 · 멈춤). 템플릿이 없으면 거짓.
    pub(crate) fn template_locked(&self) -> Result<bool, TradingError> {
        let template = self.template_in_use()?;
        Ok(template.is_some_and(|template| template.state != TemplateState::Simulating))
    }

    /// (W5 2차) 쓰는 템플릿의 값을 `params`로 바꾼 새 판을 남긴다(승인된 규칙 개선안 반영, V21).
    /// 고른 템플릿이 없으면 [`TradingError::NoTemplate`], 잠겨 있으면 저장소가 거부한다(다른 템플릿에 반영하지 않는다).
    pub(crate) fn apply_params(&self, params: RiskParams) -> Result<(), TradingError> {
        let chosen = self.template_in_use()?.ok_or(TradingError::NoTemplate)?;
        let current =
            Template::from_json(&chosen.value.to_string()).map_err(|_| TradingError::NoTemplate)?;
        let next = Template { params, ..current };
        let value = serde_json::from_str(&next.to_json()).map_err(|_| TradingError::NoTemplate)?;
        self.store
            .revise_template(chosen.id, &value, millis(self.clock.now()))?;
        Ok(())
    }
}
