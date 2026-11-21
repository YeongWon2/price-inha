//! 가리기 대상: [`SecretStore::redact`](crate::SecretStore::redact)가 가릴 값들을 칸마다 쥔다 (N8 · N1).
//!
//! 칸(묶음 + 칸 이름)마다 네 가지를 쥔다.
//! - 지금 값: 이 보관소가 마지막으로 저장했거나 키 파일에서 읽은 값. 언제나 가려진다.
//! - 앞 값: 지금 값이 되기 전의 값들. 새것부터 [`REDACT_PREVIOUS_KEPT`]개까지.
//! - 확인 중인 값: 확인기가 돌고 있거나 저장하고 있는 값. 끝날 때까지 언제나 가려진다.
//! - 실패한 값: 확인이나 저장에 실패한 값들. 새것부터 [`REDACT_FAILED_KEPT`]개까지.
//!
//! 그래서 가리기 대상은 칸마다 `1 + REDACT_PREVIOUS_KEPT + REDACT_FAILED_KEPT`개와
//! 그때 돌고 있는 등록의 수를 넘지 않는다. 오래 돌려도 끝없이 늘지 않는다.

use std::collections::{HashMap, VecDeque};

use crate::kind::Entry;
use crate::redact::{Pattern, redact_all};
use crate::{Credentials, REDACT_PREVIOUS_KEPT};

/// 칸마다 가리기 대상에 남겨 두는 **확인 · 저장에 실패한 값**의 개수.
///
/// 앞 값과 같은 개수로 둔다. 실패한 등록을 아무리 되풀이해도 칸마다 이 개수만 쥐고,
/// 가장 오래된 것부터 가리기 대상에서 빠진다.
pub(crate) const REDACT_FAILED_KEPT: usize = REDACT_PREVIOUS_KEPT;

/// 등록이 어떻게 끝났는가.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// 키 파일에 저장했다. 값은 그 칸의 지금 값이 된다.
    Saved,
    /// 확인이나 저장에 실패했다(등록이 도중에 버려진 것도 같다). 값은 실패한 값으로 남는다.
    Failed,
}

/// 칸 하나의 가리기 대상.
#[derive(Default)]
struct Slot {
    current: Option<Pattern>,
    /// 오래된 것이 앞, 새것이 뒤.
    previous: VecDeque<Pattern>,
    pending: Vec<Pattern>,
    /// 오래된 것이 앞, 새것이 뒤.
    failed: VecDeque<Pattern>,
}

/// `items`에서 `value`를 가리는 모양을 하나 뺀다.
fn take(items: &mut VecDeque<Pattern>, value: &str) -> Option<Pattern> {
    let at = items.iter().position(|item| item.is(value))?;
    items.remove(at)
}

/// `items`의 뒤에 넣고, `keep`개를 넘으면 오래된 것부터 버린다.
fn push_capped(items: &mut VecDeque<Pattern>, item: Pattern, keep: usize) {
    items.push_back(item);
    while items.len() > keep {
        items.pop_front();
    }
}

impl Slot {
    fn is_current(&self, value: &str) -> bool {
        self.current.as_ref().is_some_and(|item| item.is(value))
    }

    /// `value`를 지금 값으로 삼는다. 값이 바뀌었으면 앞의 지금 값은 앞 값으로 밀린다.
    fn set_current(&mut self, value: &str) {
        // 지금 값이 된 것을 실패한 값으로 따로 쥐지 않는다.
        let from_failed = take(&mut self.failed, value);
        if self.is_current(value) {
            return;
        }
        let Some(new) = take(&mut self.previous, value)
            .or(from_failed)
            .or_else(|| Pattern::new(value))
        else {
            return;
        };
        if let Some(old) = self.current.replace(new) {
            push_capped(&mut self.previous, old, REDACT_PREVIOUS_KEPT);
        }
    }

    /// 확인 중인 `value` 하나를 내려놓고, 지금 값이 아니면 실패한 값으로 쥔다.
    fn fail(&mut self, value: &str) {
        let Some(at) = self.pending.iter().position(|item| item.is(value)) else {
            return;
        };
        let item = self.pending.swap_remove(at);
        if self.is_current(value) {
            return;
        }
        // 이미 쥐고 있던 같은 값은 새것 자리로 옮긴다.
        let _older = take(&mut self.failed, value);
        push_capped(&mut self.failed, item, REDACT_FAILED_KEPT);
    }

    fn settle(&mut self, value: &str, outcome: Outcome) {
        match outcome {
            Outcome::Saved => {
                // 지금 값을 먼저 세운 뒤에 확인 중인 것을 내려놓는다(사이에 빈틈이 없다).
                self.set_current(value);
                if let Some(at) = self.pending.iter().position(|item| item.is(value)) {
                    self.pending.swap_remove(at);
                }
            }
            Outcome::Failed => self.fail(value),
        }
    }

    fn items(&self) -> impl Iterator<Item = &Pattern> {
        self.current
            .iter()
            .chain(&self.previous)
            .chain(&self.pending)
            .chain(&self.failed)
    }
}

/// 가리기 대상 칸 하나를 가리키는 열쇠.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    /// 줄 이름이 고정된 묶음의 칸.
    Field(Entry, &'static str),
    /// 공급자 키 줄(`PRICE_INHA_KEY__…`). 줄 이름으로 센다: 열 때 선언을 모르고 읽은 줄과
    /// 뒤에 선언으로 등록한 같은 줄이 한 칸이 되어, 다시 등록해도 개수에 끝이 있다.
    Line(String),
}

impl Key {
    /// 묶음 `entry`의 칸 `field`의 열쇠. 공급자 칸은 줄 이름으로 센다.
    fn of(entry: Entry, field: &'static str) -> Option<Self> {
        match entry {
            Entry::Provider(keys) => keys.env_name(field).map(Key::Line),
            other => Some(Key::Field(other, field)),
        }
    }
}

/// 보관소 하나의 가리기 대상 전부.
#[derive(Default)]
pub(crate) struct Targets {
    slots: HashMap<Key, Slot>,
}

impl Targets {
    /// `values`에 든 `entry`의 칸마다 `apply`를 부른다. 그 종류에 없는 칸은 건너뛴다.
    fn each(
        &mut self,
        entry: impl Into<Entry>,
        values: &Credentials,
        mut apply: impl FnMut(&mut Slot, &str),
    ) {
        let kind = entry.into();
        for &field in kind.fields() {
            if let (Some(value), Some(key)) = (values.get(field), Key::of(kind, field)) {
                apply(self.slots.entry(key).or_default(), value.expose());
            }
        }
    }

    /// 키 파일의 공급자 키 줄 하나(선언을 몰라도 된다)의 값을 그 줄의 지금 값으로 삼는다.
    /// [`MIN_SECRET_CHARS`](crate::MIN_SECRET_CHARS) 미만 값은 가릴 수 없어 대상이 아니다.
    pub(crate) fn set_current_line(&mut self, name: String, value: &str) {
        self.slots
            .entry(Key::Line(name))
            .or_default()
            .set_current(value);
    }

    /// 키 파일에 있는 값(열 때 읽었거나, 읽어서 내준 값)을 그 칸의 지금 값으로 삼는다.
    pub(crate) fn set_current(&mut self, kind: impl Into<Entry>, values: &Credentials) {
        self.each(kind, values, Slot::set_current);
    }

    /// 확인을 시작하는 값을 가리기 대상에 넣는다. 확인기를 부르기 **전에** 부른다.
    pub(crate) fn begin(&mut self, kind: impl Into<Entry>, values: &Credentials) {
        self.each(kind, values, |slot, value| {
            slot.pending.extend(Pattern::new(value));
        });
    }

    /// [`Targets::begin`]으로 넣은 값을 결과에 맞는 자리로 옮긴다.
    pub(crate) fn settle(
        &mut self,
        kind: impl Into<Entry>,
        values: &Credentials,
        outcome: Outcome,
    ) {
        self.each(kind, values, |slot, value| slot.settle(value, outcome));
    }

    /// 글에서 가리기 대상을 모두 [`REDACTED`](crate::REDACTED)로 바꾼다.
    pub(crate) fn redact(&self, text: &str) -> String {
        redact_all(self.slots.values().flat_map(Slot::items), text)
    }

    /// 바이트 안에 가리기 대상 값이 하나라도 든가.
    pub(crate) fn found_in(&self, bytes: &[u8]) -> bool {
        self.slots
            .values()
            .flat_map(Slot::items)
            .any(|item| item.occurs_in(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KeyKind, REDACTED};

    fn data(value: &str) -> Credentials {
        Credentials::new().with("api_key", value)
    }

    fn value(label: &str, n: usize) -> String {
        format!("시험용-{label}-값-{n:04}")
    }

    fn hides(targets: &Targets, value: &str) -> bool {
        targets.redact(value) == REDACTED
    }

    fn count(targets: &Targets) -> usize {
        targets
            .slots
            .values()
            .map(|slot| slot.items().count())
            .sum()
    }

    #[test]
    fn 실패한_값은_칸마다_정한_개수만_쥐고_오래된_것부터_빠진다() {
        let mut targets = Targets::default();
        targets.set_current(KeyKind::Data, &data(&value("지금", 0)));
        let total = REDACT_FAILED_KEPT + 4;
        for n in 0..total {
            let attempt = data(&value("실패", n));
            targets.begin(KeyKind::Data, &attempt);
            assert!(hides(&targets, &value("실패", n)));
            targets.settle(KeyKind::Data, &attempt, Outcome::Failed);
        }
        for n in 0..total {
            let kept = n >= total - REDACT_FAILED_KEPT;
            assert_eq!(hides(&targets, &value("실패", n)), kept, "{n}번째");
        }
        assert!(hides(&targets, &value("지금", 0)));
        assert_eq!(count(&targets), 1 + REDACT_FAILED_KEPT);
    }

    #[test]
    fn 아무리_돌려도_칸의_가리기_대상은_상한을_넘지_않는다() {
        let mut targets = Targets::default();
        for n in 0..50 {
            let ok = data(&value("성공", n));
            targets.begin(KeyKind::Data, &ok);
            targets.settle(KeyKind::Data, &ok, Outcome::Saved);
            let bad = data(&value("실패", n));
            targets.begin(KeyKind::Data, &bad);
            targets.settle(KeyKind::Data, &bad, Outcome::Failed);
            assert!(count(&targets) <= 1 + REDACT_PREVIOUS_KEPT + REDACT_FAILED_KEPT);
        }
        assert!(hides(&targets, &value("성공", 49)));
        assert!(hides(&targets, &value("실패", 49)));
        assert!(!hides(&targets, &value("성공", 0)));
        assert!(!hides(&targets, &value("실패", 0)));
    }

    #[test]
    fn 같은_값으로_다시_등록하면_앞_값이_밀려나지_않는다() {
        let mut targets = Targets::default();
        targets.set_current(KeyKind::Data, &data(&value("옛", 0)));
        for _ in 0..REDACT_PREVIOUS_KEPT + 3 {
            let same = data(&value("지금", 0));
            targets.begin(KeyKind::Data, &same);
            targets.settle(KeyKind::Data, &same, Outcome::Saved);
        }
        assert!(hides(&targets, &value("옛", 0)));
        assert_eq!(count(&targets), 2);
    }

    #[test]
    fn 지금_값으로_한_다시_등록이_실패해도_지금_값은_한_번만_쥔다() {
        let mut targets = Targets::default();
        let now = data(&value("지금", 0));
        targets.set_current(KeyKind::Data, &now);
        targets.begin(KeyKind::Data, &now);
        assert_eq!(count(&targets), 2);
        targets.settle(KeyKind::Data, &now, Outcome::Failed);
        assert_eq!(count(&targets), 1);
        assert!(hides(&targets, &value("지금", 0)));
    }

    #[test]
    fn 실패했던_값이_나중에_등록되면_지금_값으로만_쥔다() {
        let mut targets = Targets::default();
        let later = data(&value("나중", 0));
        targets.begin(KeyKind::Data, &later);
        targets.settle(KeyKind::Data, &later, Outcome::Failed);
        targets.begin(KeyKind::Data, &later);
        targets.settle(KeyKind::Data, &later, Outcome::Saved);
        assert_eq!(count(&targets), 1);
        assert!(hides(&targets, &value("나중", 0)));
    }

    #[test]
    fn 함께_도는_등록은_저마다_끝날_때까지_가려진다() {
        let mut targets = Targets::default();
        let (a, b) = (data(&value("가", 0)), data(&value("나", 0)));
        targets.begin(KeyKind::Data, &a);
        targets.begin(KeyKind::Data, &b);
        targets.settle(KeyKind::Data, &a, Outcome::Saved);
        assert!(hides(&targets, &value("가", 0)));
        assert!(hides(&targets, &value("나", 0)));
        targets.settle(KeyKind::Data, &b, Outcome::Saved);
        assert!(hides(&targets, &value("가", 0)));
        assert!(hides(&targets, &value("나", 0)));
        assert_eq!(count(&targets), 2);
    }
}
