//! (10차 10b 8번) 주기 가져오기가 본 것의 기억: 종목 · 출처마다 끝이 있는 수를 저장소에 남겨 다시 켠 뒤에도
//! 같은 것으로 두 번 깨우지 않는다. 처음 보는 칸은 그때 있던 것을 "이미 본 것"으로만 적는다.

use std::collections::{HashMap, VecDeque};

use pi_store::Store;

/// 종목 · 출처마다 기억하는 본 것의 수.
const SEEN_MAX: usize = 512;
/// 본 것을 남기는 저장소 칸의 머리.
const SEEN_KEY: &str = "data_poll.seen";

/// 종목 · 출처마다 본 것(처음 본 순서). 저장소에서 처음 읽은 뒤 바뀔 때마다 남긴다.
#[derive(Default)]
pub(super) struct Seen {
    lists: HashMap<String, VecDeque<String>>,
}

impl Seen {
    pub(super) fn key(source: &str, symbol: &str) -> String {
        format!("{SEEN_KEY}.{source}.{symbol}")
    }

    /// 그 칸을 (처음이면 저장소에서 읽어) 준다. 저장소에 없으면 `None`(아직 한 번도 보지 않았다).
    fn load(&mut self, store: &Store, key: &str) -> Option<&mut VecDeque<String>> {
        if !self.lists.contains_key(key) {
            let saved = store.knowledge_meta(key).ok().flatten()?;
            let ids: Vec<String> = serde_json::from_str(&saved).unwrap_or_default();
            self.lists
                .insert(key.to_string(), ids.into_iter().collect());
        }
        self.lists.get_mut(key)
    }

    /// 받은 것들 가운데 처음 보는 것을 돌려주고 본 것으로 적는다. 처음 보는 칸이면 적기만 하고 빈 목록이다.
    /// `fresh`는 (번호, 값)이다.
    pub(super) fn take_new<T>(
        &mut self,
        store: &Store,
        key: &str,
        fresh: Vec<(String, T)>,
    ) -> Vec<(String, T)> {
        let first = self.load(store, key).is_none();
        let list = self.lists.entry(key.to_string()).or_default();
        let mut new = Vec::new();
        let mut changed = first;
        for (id, value) in fresh {
            if list.contains(&id) {
                continue;
            }
            list.push_back(id.clone());
            changed = true;
            if !first {
                new.push((id, value));
            }
        }
        while list.len() > SEEN_MAX {
            list.pop_front();
        }
        if changed {
            let ids: Vec<&String> = list.iter().collect();
            let text = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string());
            if let Err(error) = store.set_knowledge_meta(key, &text) {
                tracing::warn!(%error, "본 자료를 저장소에 남기지 못했다");
            }
        }
        new
    }

    /// 깨우기 엔진이 받지 않은 것(속도 제한)은 본 것에서 다시 뺀다(다음에 다시 넘긴다).
    pub(super) fn forget(&mut self, store: &Store, key: &str, id: &str) {
        if let Some(list) = self.lists.get_mut(key) {
            list.retain(|seen| seen != id);
            let ids: Vec<&String> = list.iter().collect();
            let text = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string());
            if let Err(error) = store.set_knowledge_meta(key, &text) {
                tracing::warn!(%error, "본 자료를 저장소에 남기지 못했다");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("poll.db")).unwrap();
        (dir, store)
    }

    fn ids(list: &[(String, ())]) -> Vec<&str> {
        list.iter().map(|(id, ())| id.as_str()).collect()
    }

    fn fresh(names: &[&str]) -> Vec<(String, ())> {
        names.iter().map(|name| ((*name).to_string(), ())).collect()
    }

    #[test]
    fn 처음_보는_칸은_적기만_하고_다음부터_새것만_준다() {
        let (_dir, store) = store();
        let mut seen = Seen::default();
        let key = Seen::key("dart", "005930");
        assert!(seen.take_new(&store, &key, fresh(&["a", "b"])).is_empty());
        let new = seen.take_new(&store, &key, fresh(&["a", "b", "c"]));
        assert_eq!(ids(&new), ["c"]);
        assert!(seen.take_new(&store, &key, fresh(&["c"])).is_empty());
    }

    #[test]
    fn 본_것은_다시_켜도_남는다() {
        let (_dir, store) = store();
        let key = Seen::key("naver", "000660");
        let mut first = Seen::default();
        assert!(
            first
                .take_new(&store, &key, Vec::<(String, ())>::new())
                .is_empty()
        );
        assert_eq!(ids(&first.take_new(&store, &key, fresh(&["x"]))), ["x"]);
        let mut again = Seen::default();
        assert!(again.take_new(&store, &key, fresh(&["x"])).is_empty());
        assert_eq!(ids(&again.take_new(&store, &key, fresh(&["y"]))), ["y"]);
    }

    #[test]
    fn 받지_않은_것은_잊어_다음에_다시_준다() {
        let (_dir, store) = store();
        let key = Seen::key("dart", "005930");
        let mut seen = Seen::default();
        let _ = seen.take_new(&store, &key, Vec::<(String, ())>::new());
        assert_eq!(ids(&seen.take_new(&store, &key, fresh(&["r"]))), ["r"]);
        seen.forget(&store, &key, "r");
        assert_eq!(ids(&seen.take_new(&store, &key, fresh(&["r"]))), ["r"]);
    }

    #[test]
    fn 기억하는_수에는_끝이_있다() {
        let (_dir, store) = store();
        let key = Seen::key("naver", "005930");
        let mut seen = Seen::default();
        let _ = seen.take_new(&store, &key, Vec::<(String, ())>::new());
        let many: Vec<String> = (0..SEEN_MAX + 10).map(|n| n.to_string()).collect();
        let names: Vec<&str> = many.iter().map(String::as_str).collect();
        let _ = seen.take_new(&store, &key, fresh(&names));
        assert_eq!(seen.lists.get(&key).map(VecDeque::len), Some(SEEN_MAX));
    }
}
