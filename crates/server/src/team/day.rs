//! 팀장의 하루 운용 판단(V5)과 냉각중 개선안 반영(E28).

use chrono::{DateTime, Utc};
use pi_agent::Role;
use pi_store::{NewRoomEvent, ProposalState, RoomActorKind, UnixMillis};
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
            .run_turn(config, limits, &title, &input)
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
        self.store.append_room_event(NewRoomEvent {
            room: DAY_ROOM.to_string(),
            event_id: format!("day-{date}-leader"),
            kind: "message.member".to_string(),
            actor_kind: RoomActorKind::Member,
            actor_id: Role::Leader.id().to_string(),
            payload: json!({"text": text, "plan": word}),
            at: millis(now),
        })?;
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
        for proposal in self.store.proposals_in_state(ProposalState::Cooling)? {
            if proposal.updated_at >= millis(start) {
                continue;
            }
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
}
