//! (8차, 백로그 U 3번 "본 것 기억 기록 시점") 상한으로 버린 자료 사건을 부르는 쪽에 돌려준다.

use crate::{DataChange, Trigger};

impl Trigger {
    /// 지난번 뒤로 상한으로 버려 돌려줄 자료 사건들(이른 순). 한 번만 준다.
    pub fn take_returned_data(&self) -> Vec<DataChange> {
        todo!()
    }
}
