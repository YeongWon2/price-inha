//! (7차, server 10b-2 5번) 구독을 끈 종류의 자료 사건 버리기.
//!
//! 쓰는 템플릿이 어떤 자료 구독을 끄면, 그 전에 받아 묶음(조용한 시간을 기다리는 중)이나 장 밖 보관에 든 그 종류의
//! 사건이 뒤늦게 팀을 깨우면 안 된다(해제 직전 대기열). 부르는 쪽이 끈 종류로 이것을 부른다.
//! 원본에 없음(원본 Hermes webhook은 경로를 끄면 그 경로의 요청을 받지 않을 뿐 — 묶음은 경로마다라 함께 사라진다).
//! 이미 깨운(실행 중인) 깨우기는 건드리지 않는다.

use crate::engine::lock;
use crate::{DataKind, Trigger};

impl Trigger {
    /// 묶음 · 장 밖 보관에 남은 그 종류들의 자료 사건을 버리고 버린 수를 준다. 중복 표는 그대로다.
    pub fn discard_data(&self, kinds: &[DataKind]) -> usize {
        if kinds.is_empty() {
            return 0;
        }
        let removed = lock(&self.data).discard(kinds);
        if removed > 0 {
            tracing::info!(removed, ?kinds, "구독을 끈 종류의 자료 사건을 버렸다");
        }
        removed
    }
}
