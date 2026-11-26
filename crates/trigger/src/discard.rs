//! (7차, server 10b-2 5번) 구독을 끈 종류의 자료 사건 버리기 — 뼈대(red).

use crate::{DataKind, Trigger};

impl Trigger {
    /// 묶음 · 장 밖 보관에 남은 그 종류들의 자료 사건을 버리고 버린 수를 준다(뼈대).
    pub fn discard_data(&self, kinds: &[DataKind]) -> usize {
        let _ = (self, kinds);
        0
    }
}
