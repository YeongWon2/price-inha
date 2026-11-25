//! (33차 4번 · 3번) 사무실 크레이트에 끼우는 두 자리.
//!
//! - [`SummonSink`]: 사무실의 이벤트 포트를 감싼다. 사무실 크레이트는 부르기 단계를 `office.agent`
//!   (칸 `phase` · `reason` · `phaseAt`)에 싣는다. 그 단계가 담당마다 마지막으로 본 단계와 다르면
//!   [`KIND_OFFICE_SUMMON`] `{"agent", "phase", "reason", "at"}`를 하나 더 낸다. 어느 길(부르기 · 도착 · 멘션 ·
//!   방 바꿈 · 시계 굴리기)로 바뀌었든 같은 곳에서 내므로 빠지지 않는다.
//! - [`SpareBudget`]: 하루 AI 호출 상한 판정(공지 확인 답 · 이어 말하기). 팀은 사무실보다 늦게 조립되므로 팀이
//!   붙은 뒤에 잇는다. 팀이 없으면 늘 허용한다(AI를 부르지 않으므로).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use async_trait::async_trait;
use pi_office::{Agent, AiBudget, EVENT_AGENT, EventSink, SinkError};
use serde_json::{Value, json};

use super::KIND_OFFICE_SUMMON;
use crate::Crew;

/// 처음 단계(사무실 크레이트가 만들 때의 단계).
const FIRST_PHASE: &str = "idle";

/// 부르기 단계가 바뀔 때 [`KIND_OFFICE_SUMMON`]을 더 내는 이벤트 포트.
pub(crate) struct SummonSink {
    inner: Arc<dyn EventSink>,
    /// 담당 id → 마지막으로 본 단계.
    phases: Mutex<HashMap<String, String>>,
}

impl SummonSink {
    pub(crate) fn new(inner: Arc<dyn EventSink>) -> SummonSink {
        SummonSink {
            inner,
            phases: Mutex::new(HashMap::new()),
        }
    }

    /// `office.agent` 내용에서 단계가 바뀌었으면 부르기 단계 이벤트 내용을 만든다(마지막 단계를 새로 적는다).
    fn changed(&self, payload: &Value) -> Option<Value> {
        let agent = payload["agent"].as_str()?;
        let phase = payload["phase"].as_str()?;
        let mut phases = self.phases.lock().unwrap_or_else(PoisonError::into_inner);
        let last = phases
            .get(agent)
            .map_or(FIRST_PHASE, String::as_str)
            .to_owned();
        if last == phase {
            return None;
        }
        phases.insert(agent.to_owned(), phase.to_owned());
        Some(json!({
            "agent": agent,
            "phase": phase,
            "reason": payload["reason"].clone(),
            "at": payload["phaseAt"].clone(),
        }))
    }
}

#[async_trait]
impl EventSink for SummonSink {
    async fn publish(&self, kind: &str, payload: Value) -> Result<u64, SinkError> {
        let summon = (kind == EVENT_AGENT)
            .then(|| self.changed(&payload))
            .flatten();
        let seq = self.inner.publish(kind, payload).await?;
        if let Some(summon) = summon
            && let Err(error) = self.inner.publish(KIND_OFFICE_SUMMON, summon).await
        {
            tracing::warn!(%error, "부르기 단계 이벤트를 내지 못했습니다");
        }
        Ok(seq)
    }
}

/// 하루 AI 호출 상한 판정: 팀이 붙으면 팀의 남은 몫(팀장 예비분 제외)으로 판정한다.
#[derive(Default)]
pub(crate) struct SpareBudget(OnceLock<Weak<Crew>>);

impl SpareBudget {
    pub(crate) fn attach(&self, crew: &Arc<Crew>) {
        let _ = self.0.set(Arc::downgrade(crew));
    }
}

impl AiBudget for SpareBudget {
    fn try_take(&self, agent: Agent) -> bool {
        match self.0.get().and_then(Weak::upgrade) {
            Some(crew) => crew.take_spare_ai_call_by(agent),
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Nowhere;

    #[async_trait]
    impl EventSink for Nowhere {
        async fn publish(&self, _kind: &str, _payload: Value) -> Result<u64, SinkError> {
            Ok(1)
        }
    }

    #[test]
    fn 단계가_바뀔_때만_부르기_이벤트를_만든다() {
        let sink = SummonSink::new(Arc::new(Nowhere));
        let view =
            |phase: &str| json!({"agent": "risk", "phase": phase, "reason": "call", "phaseAt": 7});
        assert!(sink.changed(&view("idle")).is_none(), "처음은 자리");
        let called = sink.changed(&view("called")).expect("오는 중");
        assert_eq!(
            called,
            json!({"agent": "risk", "phase": "called", "reason": "call", "at": 7})
        );
        assert!(
            sink.changed(&view("called")).is_none(),
            "같은 단계는 다시 내지 않는다"
        );
        assert!(
            sink.changed(&json!({"agent": "risk"})).is_none(),
            "단계 칸이 없으면 없다"
        );
    }
}
