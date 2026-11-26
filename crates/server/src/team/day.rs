//! 팀장의 하루 운용 판단(V5)과 냉각중 개선안 반영(E28).

use chrono::{DateTime, Utc};
use pi_agent::Role;
use pi_store::{
    NewRoomEvent, Proposal, ProposalSource, ProposalState, RoomActorKind, StoreError, TemplateId,
    UnixMillis,
};
use serde_json::{Value, json};

use super::demo_ai::DAY_PLAN_HEAD;
use super::{Crew, DAY_ROOM, KIND_DAY_DISCUSSION};
use crate::{DayPlan, TeamError, TradingError};

fn millis(at: DateTime<Utc>) -> UnixMillis {
    UnixMillis(at.timestamp_millis())
}

/// 팀장의 답을 읽는다. 모양이 틀리면 없음.
fn parse_plan(text: &str) -> Option<DayPlan> {
    let value = Value::Object(pi_agent::find_object_with(text, "plan")?);
    match value["plan"].as_str()? {
        "trade" => Some(DayPlan::Trade),
        "rest" => Some(DayPlan::Rest {
            reason: value["reason"]
                .as_str()
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or("팀장이 쉬기로 정했다")
                .to_string(),
        }),
        _ => None,
    }
}

/// 답하지 못했을 때의 판단(보수적으로 쉬기).
fn unanswered() -> DayPlan {
    DayPlan::Rest {
        reason: "팀장이 하루 판단에 답하지 못했다".to_string(),
    }
}

fn plan_fields(plan: &DayPlan) -> (&'static str, Value) {
    match plan {
        DayPlan::Trade => ("trade", Value::Null),
        DayPlan::Rest { reason } => ("rest", json!(reason)),
    }
}

impl Crew {
    /// 오늘 판단이 이미 있으면 그것, 없으면 팀장에게 묻고 남긴다.
    pub(super) async fn decide_today(&self) -> Result<DayPlan, TeamError> {
        if let Some(plan) = self.trading.plan_today() {
            return Ok(plan);
        }
        let now = self.clock.now();
        let date = self.trading.date_of(now).format("%Y-%m-%d").to_string();
        let input = format!(
            "{DAY_PLAN_HEAD} {date} 오늘 매매할지 쉴지 정한다. 팀 상태와 시장을 보고 답은 JSON 객체 하나다: \
             {{\"plan\": \"trade\"}} 또는 {{\"plan\": \"rest\", \"reason\": \"<쉬는 이유>\"}}"
        );
        let config = self.role(Role::Leader);
        let title = format!("day-{date}");
        let limits = pi_agent::TimeLimits::MARKET;
        let (plan, text) = match self
            .team
            .runner()
            .run_turn(&config, limits, &title, &input)
            .await
        {
            Ok(outcome) => (
                parse_plan(&outcome.text).unwrap_or_else(unanswered),
                outcome.text,
            ),
            Err(error) => (unanswered(), error.to_string()),
        };
        let text = pi_ai::Redactor::redact(self.redactor.as_ref(), &text);
        self.record_day(&date, &plan, &text, now).await?;
        match self.trading.decide_day(plan.clone()) {
            Ok(_) | Err(TradingError::Halted(_)) => Ok(plan),
            Err(other) => Err(other.into()),
        }
    }

    /// 하루 판단 논의를 방 로그와 이벤트로 남긴다.
    async fn record_day(
        &self,
        date: &str,
        plan: &DayPlan,
        text: &str,
        now: DateTime<Utc>,
    ) -> Result<(), TeamError> {
        let (word, reason) = plan_fields(plan);
        let appended = self.store.append_room_event(NewRoomEvent {
            room: DAY_ROOM.to_string(),
            event_id: format!("day-{date}-leader"),
            kind: "message.member".to_string(),
            actor_kind: RoomActorKind::Member,
            actor_id: Role::Leader.id().to_string(),
            payload: json!({"text": text, "plan": word}),
            at: millis(now),
        })?;
        // (22차 1번) 쓴 자리에서 곧바로 사무실 팀 대화로 낸다(같은 줄이 이미 있었으면 다시 내지 않는다).
        if let (pi_store::RoomAppend::Appended(event), Some(talk)) = (
            &appended,
            self.links.get().and_then(|links| links.talk.as_ref()),
        ) {
            talk.publish_event(event, "day", self.trading.market())
                .await;
        }
        let payload = json!({"date": date, "plan": word, "reason": reason, "text": text});
        self.trading.record(KIND_DAY_DISCUSSION, payload).await?;
        Ok(())
    }

    /// 냉각중 개선안 가운데 냉각에 든 뒤로 거래일 시작이 지난 것을 반영한다(E28).
    pub(super) fn apply_cooled(&self) -> Result<(), TeamError> {
        let now = self.clock.now();
        let Some(calendar) = self.trading.calendar(now) else {
            return Ok(());
        };
        let Some(start) = calendar.today.sessions.iter().map(|s| s.start).min() else {
            return Ok(());
        };
        if now < start {
            return Ok(());
        }
        let mut cooled = Vec::new();
        for proposal in self
            .store
            .proposals_in_state(ProposalState::Cooling)?
            .into_iter()
            .filter(|proposal| proposal.updated_at < millis(start))
        {
            match self.store.proposal_source(proposal.id)? {
                ProposalSource::User { template } => {
                    self.apply_cooled_user(&proposal, template, millis(now))?;
                }
                ProposalSource::Review => cooled.push(proposal),
            }
        }
        // (백로그 Q) 잠김 확인부터 값 반영까지 승격 판정 · 실전 승격과 같은 차례로 돈다.
        let _turn = self.trading.template_turn();
        // 쓰는 템플릿의 값이 잠겨 있으면(승격대기 · 실전중) 반영을 건너뛰고 냉각중으로 둔다(V21). 하루 판단은
        // 그대로 간다. 잠김이 풀린 뒤의 거래일 시작에 반영한다.
        if !cooled.is_empty() && self.trading.template_locked()? {
            tracing::info!(
                waiting = cooled.len(),
                "쓰는 템플릿이 잠겨 있어 냉각이 끝난 개선안을 반영하지 않았습니다"
            );
            return Ok(());
        }
        for proposal in cooled {
            let Some(current) = self.trading.params()? else {
                return Ok(());
            };
            let outcome = self
                .journal
                .apply_cooled(proposal.id, &current, millis(start), millis(now))
                .map_err(|error| TeamError::Journal(error.to_string()))?;
            self.trading.apply_params(outcome.rules)?;
        }
        Ok(())
    }

    /// (21차 1번) 냉각이 끝난 사용자 변경 개선안을 그 템플릿에 반영한다(실전중이어도 — 저장소가 한 쓰기로
    /// 개선안 반영됨과 새 리비전을 남긴다). 템플릿이 멈춤이라 거부되면 냉각중으로 두고 기록만 한다.
    fn apply_cooled_user(
        &self,
        proposal: &Proposal,
        template: TemplateId,
        at: UnixMillis,
    ) -> Result<(), TeamError> {
        match self
            .store
            .apply_cooled_user_proposal(proposal.id, &proposal.content, at)
        {
            Ok(_) => Ok(()),
            Err(StoreError::TemplateLocked { .. }) => {
                tracing::info!(
                    proposal = proposal.id.0,
                    template = template.0,
                    "템플릿이 멈춤이라 냉각이 끝난 사용자 변경을 반영하지 않았습니다"
                );
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
}
