//! 깨우기 대기열(원본: OpenClaw 시스템 이벤트 대기열). 대상마다 메모리 안의 줄이다.
//!
//! 대상마다 "다음 턴이 가져갈 알림"([`Lane::Notice`]) 20개와 "스스로 턴을 시작하는 깨우기"
//! ([`Lane::Wake`]) 20개를 **따로** 센다(대상마다 최대 40개).

use std::collections::HashMap;

use crate::Target;

/// 한 줄(lane)의 상한(20).
pub const LANE_CAPACITY: usize = 20;

/// 줄의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane {
    /// 다음 턴이 가져갈 알림.
    Notice,
    /// 스스로 턴을 시작하는 깨우기.
    Wake,
}

/// 대기열의 항목 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedItem {
    /// 줄.
    pub lane: Lane,
    /// 중복 · 교체 키(예: `price:005930:above`). 없을 수 있다.
    pub key: Option<String>,
    /// 글.
    pub text: String,
    /// 참이면 같은 키의 대기 항목을 지우고 이것으로 바꾼다(키가 있어야 한다).
    pub replace: bool,
}

/// 넣은 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    /// 넣었다.
    Queued,
    /// 같은 것이 이미 있어 넣지 않았다(합쳐짐).
    ///
    /// 키가 없으면 그 대상의 **맨 뒤 항목**과 글이 같을 때, 키가 있으면 같은 키 · 같은 글이
    /// 줄 **어디에든** 있을 때다. `replace`인데 같은 키 항목이 같은 글 하나뿐일 때도 이것이다.
    Duplicate,
    /// `replace`로 같은 키의 대기 항목을 지우고 새것을 넣었다.
    Replaced,
    /// 그 줄이 가득 차 넣지 않았다. 대기열은 바뀌지 않는다.
    Full,
    /// `replace`인데 키가 없다. 대기열은 바뀌지 않는다.
    Invalid,
}

/// 대상마다의 대기열.
#[derive(Debug, Default)]
pub struct WakeQueue {
    items: HashMap<Target, Vec<QueuedItem>>,
}

impl WakeQueue {
    /// 빈 대기열.
    pub fn new() -> Self {
        Self::default()
    }

    /// 대상의 줄에 넣는다. 규칙은 [`PushOutcome`]의 각 값에 적었다.
    /// 상한은 줄마다 [`LANE_CAPACITY`]이고, `replace`면 같은 키 항목을 지운 뒤에 센다.
    pub fn push(&mut self, target: &Target, item: QueuedItem) -> PushOutcome {
        let _ = (&mut self.items, target, item);
        todo!()
    }

    /// 대상의 항목을 넣은 순서대로 모두 꺼낸다(대기열에서 지운다).
    pub fn drain(&mut self, target: &Target) -> Vec<QueuedItem> {
        let _ = target;
        todo!()
    }

    /// 대상의 그 줄에 있는 항목 수.
    pub fn len(&self, target: &Target, lane: Lane) -> usize {
        let _ = (target, lane);
        todo!()
    }

    /// 대상의 대기열이 비었는가.
    pub fn is_empty(&self, target: &Target) -> bool {
        let _ = target;
        todo!()
    }
}
