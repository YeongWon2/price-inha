//! (백로그 Q) 쓰는 템플릿의 값 · 상태를 바꾸는 일을 한 줄로 세우는 차례 잠금.
//!
//! 개선안 답(잠김 확인 → 승인 소비 → 값 반영), 하루 마감의 승격 판정(모의중 → 승격대기), 실전 승격의 답과
//! 맞춰 보기(승격대기 → 실전중)가 겹치면, 개선안이 "반영됨"인데 값은 그대로이거나(잠긴 템플릿에 반영이 거부돼
//! 500) 답이 500이 될 수 있었다. 이 일들은 [`TemplateTurn`] 하나를 잡고 돈다. 잡은 동안에는 기다리지(await) 않는다.
//!
//! 승격 판정은 통계를 재는 동안(기다림이 있다) 잠금을 잡지 않으므로, 잡은 뒤에 쓰는 템플릿이 그대로인지
//! ([`still_same`]) 다시 보고 바뀌었으면 그 마감에는 올리지 않는다(다음 마감에 다시 잰다).

use std::sync::{Mutex, MutexGuard, PoisonError};

use pi_store::{Template, TemplateState};

/// 쓰는 템플릿을 바꾸는 일의 차례 잠금.
#[derive(Debug, Default)]
pub(crate) struct TemplateTurn(Mutex<()>);

impl TemplateTurn {
    /// 차례를 잡는다(앞 사람이 놓을 때까지 기다린다 — 잡은 쪽은 기다리지 않으니 짧다).
    pub(crate) fn hold(&self) -> MutexGuard<'_, ()> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// 통계를 잴 때의 템플릿(`before`)이 지금 쓰는 템플릿(`now`)과 같은 판이고 아직 모의중인가.
pub(super) fn still_same(before: &Template, now: Option<&Template>) -> bool {
    now.is_some_and(|now| {
        now.id == before.id
            && now.revision == before.revision
            && now.state == TemplateState::Simulating
    })
}

impl super::Trading {
    /// 쓰는 템플릿을 바꾸는 일의 차례를 잡는다([`TemplateTurn`]). 잡은 동안 기다리지 않는다.
    pub(crate) fn template_turn(&self) -> MutexGuard<'_, ()> {
        self.template_turn.hold()
    }
}

#[cfg(test)]
mod tests {
    use pi_store::{TemplateId, UnixMillis};
    use serde_json::json;

    use super::*;

    fn template(id: i64, revision: u32, state: TemplateState) -> Template {
        Template {
            id: TemplateId(id),
            revision,
            value: json!({}),
            state,
            created_at: UnixMillis(0),
            updated_at: UnixMillis(0),
            criteria_pass: None,
            approval: None,
        }
    }

    #[test]
    fn 같은_판의_모의중이면_그대로다() {
        let before = template(1, 3, TemplateState::Simulating);
        assert!(still_same(&before, Some(&before.clone())));
    }

    #[test]
    fn 판이_바뀌었거나_상태가_바뀌었거나_다른_템플릿이면_그대로가_아니다() {
        let before = template(1, 3, TemplateState::Simulating);
        assert!(!still_same(
            &before,
            Some(&template(1, 4, TemplateState::Simulating))
        ));
        assert!(!still_same(
            &before,
            Some(&template(1, 3, TemplateState::PromotionPending))
        ));
        assert!(!still_same(
            &before,
            Some(&template(2, 3, TemplateState::Simulating))
        ));
        assert!(!still_same(&before, None));
    }

    #[test]
    fn 차례는_한_번에_하나만_잡힌다() {
        let turn = TemplateTurn::default();
        let held = turn.hold();
        assert!(turn.0.try_lock().is_err(), "잡은 동안 다른 쪽은 못 잡는다");
        drop(held);
        assert!(turn.0.try_lock().is_ok());
    }
}
