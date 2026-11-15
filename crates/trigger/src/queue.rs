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
        let items = self.items.entry(target.clone()).or_default();
        let outcome = push_into(items, item);
        if items.is_empty() {
            self.items.remove(target);
        }
        outcome
    }

    /// 대상의 항목을 넣은 순서대로 모두 꺼낸다(대기열에서 지운다).
    pub fn drain(&mut self, target: &Target) -> Vec<QueuedItem> {
        self.items.remove(target).unwrap_or_default()
    }

    /// 대상의 그 줄에 있는 항목 수.
    pub fn len(&self, target: &Target, lane: Lane) -> usize {
        self.items.get(target).map_or(0, |items| {
            items.iter().filter(|item| item.lane == lane).count()
        })
    }

    /// 대상의 대기열이 비었는가.
    pub fn is_empty(&self, target: &Target) -> bool {
        self.items.get(target).is_none_or(Vec::is_empty)
    }
}

/// 한 대상의 항목들에 넣는다. 넣지 않는 결과(`Duplicate` · `Full` · `Invalid`)면 항목들을 바꾸지 않는다.
fn push_into(items: &mut Vec<QueuedItem>, item: QueuedItem) -> PushOutcome {
    let same_key = |queued: &QueuedItem| queued.lane == item.lane && queued.key == item.key;
    if item.replace {
        if item.key.is_none() {
            return PushOutcome::Invalid;
        }
        let matching: Vec<&QueuedItem> = items.iter().filter(|queued| same_key(queued)).collect();
        if let [only] = matching.as_slice()
            && only.text == item.text
        {
            return PushOutcome::Duplicate;
        }
        let removed = matching.len();
        let remaining = count_lane(items, item.lane) - removed;
        if remaining >= LANE_CAPACITY {
            return PushOutcome::Full;
        }
        items.retain(|queued| !same_key(queued));
        items.push(item);
        return if removed > 0 {
            PushOutcome::Replaced
        } else {
            PushOutcome::Queued
        };
    }
    let duplicate = match &item.key {
        None => items
            .iter()
            .rev()
            .find(|queued| queued.lane == item.lane)
            .is_some_and(|last| last.key.is_none() && last.text == item.text),
        Some(_) => items
            .iter()
            .any(|queued| same_key(queued) && queued.text == item.text),
    };
    if duplicate {
        return PushOutcome::Duplicate;
    }
    if count_lane(items, item.lane) >= LANE_CAPACITY {
        return PushOutcome::Full;
    }
    items.push(item);
    PushOutcome::Queued
}

/// 그 줄의 항목 수.
fn count_lane(items: &[QueuedItem], lane: Lane) -> usize {
    items.iter().filter(|queued| queued.lane == lane).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(lane: Lane, text: &str) -> QueuedItem {
        QueuedItem {
            lane,
            key: None,
            text: text.to_string(),
            replace: false,
        }
    }

    #[test]
    fn 키_없는_합치기는_같은_줄의_맨_뒤만_본다() {
        let mut queue = WakeQueue::new();
        assert_eq!(
            queue.push(&Target::Team, item(Lane::Notice, "가")),
            PushOutcome::Queued
        );
        // 다른 줄의 같은 글은 합치지 않는다(알림이 깨우기를 삼키지 않게).
        assert_eq!(
            queue.push(&Target::Team, item(Lane::Wake, "가")),
            PushOutcome::Queued
        );
        assert_eq!(
            queue.push(&Target::Team, item(Lane::Notice, "가")),
            PushOutcome::Duplicate
        );
    }

    #[test]
    fn 가득_찬_줄의_교체는_지울_것이_없으면_넣지_않는다() {
        let mut queue = WakeQueue::new();
        for n in 0..LANE_CAPACITY {
            queue.push(&Target::Team, item(Lane::Wake, &n.to_string()));
        }
        let replace = QueuedItem {
            lane: Lane::Wake,
            key: Some("k".to_string()),
            text: "새것".to_string(),
            replace: true,
        };
        assert_eq!(queue.push(&Target::Team, replace), PushOutcome::Full);
        assert_eq!(queue.len(&Target::Team, Lane::Wake), LANE_CAPACITY);
    }
}
