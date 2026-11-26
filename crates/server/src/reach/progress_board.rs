//! (51차, 차수 표 21번) 사무실에 걸리는 팀의 두 표시판: 지금 도는 논의의 진행 단계(C21)와 하루 AI 잔량(C22).
//!
//! - 진행 단계는 마지막 내용을 들고 있다가 접속 스냅샷([`super::OFFICE_PATH`]의 `progress` 칸)에 싣는다. 논의가
//!   끝나면 마지막 상태를 한 번 내고 비운다(스냅샷은 null). 이벤트를 내다 실패해도 부르는 쪽은 그대로 간다(E51).
//! - 잔량 이벤트([`crate::KIND_TEAM_AI_USED`])는 줄로 받아 차례대로 낸다(뒤에 센 것이 마지막 이벤트가 되게).
//! - 팀은 사무실보다 늦게 조립되므로 팀이 붙을 때 이 판을 받는다([`crate::Crew::attach_board`]).

use std::sync::{Arc, Mutex, PoisonError};

use pi_office::EventSink;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{KIND_OFFICE_PROGRESS, KIND_TEAM_AI_USED};

/// 진행 단계 · 잔량 표시판.
pub(crate) struct ProgressBoard {
    sink: Arc<dyn EventSink>,
    /// 지금 도는 논의의 마지막 진행 내용(없으면 도는 논의가 없다).
    current: Mutex<Option<Value>>,
    /// 잔량 이벤트 줄(처음 쓸 때 내보내는 작업을 띄운다).
    budget: Mutex<Option<mpsc::UnboundedSender<Value>>>,
}

impl ProgressBoard {
    pub(crate) fn new(sink: Arc<dyn EventSink>) -> ProgressBoard {
        ProgressBoard {
            sink,
            current: Mutex::new(None),
            budget: Mutex::new(None),
        }
    }

    /// 지금 도는 논의의 진행 내용(없으면 null). 접속 스냅샷에 싣는다.
    pub(crate) fn current(&self) -> Value {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or(Value::Null)
    }

    /// 진행 내용을 낸다. `last`면 낸 뒤 비운다(논의가 끝났다).
    pub(crate) async fn show(&self, payload: Value, last: bool) {
        {
            let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
            *current = (!last).then(|| payload.clone());
        }
        if let Err(error) = self.sink.publish(KIND_OFFICE_PROGRESS, payload).await {
            tracing::warn!(%error, "논의 진행 단계를 내지 못했습니다(논의는 그대로 갑니다)");
        }
    }

    /// 진행 내용을 내지 않고 비운다(논의가 취소 · panic으로 끝났을 때).
    pub(crate) fn clear(&self) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// 잔량 내용을 줄에 넣는다(차례대로 낸다). 런타임 밖이면 내지 않는다.
    pub(crate) fn budget(self: &Arc<Self>, payload: Value) {
        let mut line = self.budget.lock().unwrap_or_else(PoisonError::into_inner);
        if line.is_none() {
            let Ok(runtime) = tokio::runtime::Handle::try_current() else {
                return;
            };
            let (sender, mut receiver) = mpsc::unbounded_channel::<Value>();
            let board = Arc::downgrade(self);
            runtime.spawn(async move {
                while let Some(payload) = receiver.recv().await {
                    let Some(board) = board.upgrade() else {
                        return;
                    };
                    if let Err(error) = board.sink.publish(KIND_TEAM_AI_USED, payload).await {
                        tracing::warn!(%error, "하루 AI 잔량 이벤트를 내지 못했습니다");
                    }
                }
            });
            *line = Some(sender);
        }
        if let Some(sender) = line.as_ref()
            && sender.send(payload).is_err()
        {
            *line = None;
        }
    }
}
