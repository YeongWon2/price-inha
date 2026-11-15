//! 복기 기록과 실수 종류(V19 · E17).

use pi_store::{TradeId, UnixMillis};
use serde::{Deserialize, Serialize};

/// 실수 종류: 정한 목록 + 자유 글.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mistake {
    /// 추격 매수
    ChasedEntry,
    /// 손절이 늦음
    LateStop,
    /// 비중 과다
    OversizedPosition,
    /// 근거 없이 진입
    NoRationale,
    /// 성급한 청산
    PanicExit,
    /// 자유 글. 앞뒤 공백을 뗀 글자로 같은 실수인지 센다. 빈 글은 거부한다.
    Other(String),
}

/// 복기 기록 한 건.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRecord {
    /// 복기한 매매.
    pub trade: TradeId,
    /// 실수들. 없어도 된다(실수 없음). 자유 글은 앞뒤 공백을 뗀 것이다.
    pub mistakes: Vec<Mistake>,
    /// 복기 글.
    pub note: String,
    /// 복기한 시각.
    pub at: UnixMillis,
}
