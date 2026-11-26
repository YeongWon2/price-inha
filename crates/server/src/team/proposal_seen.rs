//! (백로그 Q) 개선안마다 남긴 승인 요청 이벤트(`rule_proposal`)를 읽어 둔다.
//!
//! 개선안 목록을 그릴 때마다 저장소의 이벤트를 처음부터 다 읽던 것을, 읽은 이벤트 번호까지 기억해 두고 그 뒤에
//! 생긴 이벤트만 읽게 한다(이벤트는 지워지거나 바뀌지 않고 번호가 늘 커진다). 돌려주는 것은 처음부터 읽은 것과
//! 같다(난 차례대로).

use std::sync::{Mutex, PoisonError};

use pi_store::{Store, StoreError};
use serde_json::Value;

/// 한 번에 읽는 이벤트 수.
const READ_PAGE: usize = 1_000;

/// 읽어 둔 개선안 이벤트와 마지막으로 읽은 이벤트 번호.
#[derive(Debug, Default)]
pub(super) struct ProposalNotices(Mutex<Seen>);

#[derive(Debug, Default)]
struct Seen {
    after: u64,
    found: Vec<Value>,
}

impl ProposalNotices {
    /// 지금까지의 개선안 이벤트(`kind`) 내용들(난 차례대로).
    pub(super) fn read(&self, store: &Store, kind: &str) -> Result<Vec<Value>, StoreError> {
        Ok(self.read_counted(store, kind)?.0)
    }

    /// [`ProposalNotices::read`]와 같고, 이번에 저장소에서 읽은 이벤트 수를 함께 준다.
    fn read_counted(&self, store: &Store, kind: &str) -> Result<(Vec<Value>, usize), StoreError> {
        // 잡은 채로 읽어, 함께 읽는 쪽이 같은 이벤트를 두 번 싣지 않게 한다.
        let mut seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let mut read = 0;
        loop {
            let events = store.read_after(seen.after, READ_PAGE)?;
            let Some(last) = events.last() else {
                return Ok((seen.found.clone(), read));
            };
            let next = last.seq;
            read += events.len();
            seen.found.extend(
                events
                    .into_iter()
                    .filter(|event| event.kind == kind)
                    .map(|event| event.payload),
            );
            seen.after = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const KIND: &str = "rule_proposal";

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("pi.sqlite")).unwrap();
        (dir, store)
    }

    #[test]
    fn 읽은_뒤에는_새로_생긴_이벤트만_읽는다() {
        let (_dir, store) = store();
        store.append(KIND, &json!({"id": 1})).unwrap();
        store.append("other", &json!({"x": 1})).unwrap();
        let seen = ProposalNotices::default();
        let (first, read) = seen.read_counted(&store, KIND).unwrap();
        assert_eq!(first, vec![json!({"id": 1})]);
        assert!(read >= 2, "처음에는 다 읽는다");

        let (again, read) = seen.read_counted(&store, KIND).unwrap();
        assert_eq!(again, first, "같은 것을 돌려준다");
        assert_eq!(read, 0, "새 이벤트가 없으면 읽지 않는다");

        store.append("other", &json!({"x": 2})).unwrap();
        store.append(KIND, &json!({"id": 2})).unwrap();
        let (third, read) = seen.read_counted(&store, KIND).unwrap();
        assert_eq!(third, vec![json!({"id": 1}), json!({"id": 2})], "난 차례대로");
        assert_eq!(read, 2, "뒤에 생긴 것만 읽는다");
    }

    #[test]
    fn 이벤트가_없으면_빈_목록이고_뒤에_생기면_보인다() {
        let (_dir, store) = store();
        let seen = ProposalNotices::default();
        assert!(seen.read(&store, KIND).unwrap().is_empty());
        store.append(KIND, &json!({"id": 7})).unwrap();
        assert_eq!(seen.read(&store, KIND).unwrap(), vec![json!({"id": 7})]);
    }
}
