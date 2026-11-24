//! 담당 부르기 단계(결정 17 · D12 · M12). 원본 DeskRPG: called → waiting → returning → idle.

/// 부르기 단계. 서버는 단계와 시각만 쥐고, 걷는 길 · 도착 판정은 화면이 한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SummonPhase {
    /// 자리에 있다. 이벤트 이름 `"idle"`.
    Idle,
    /// 불려서 오는 중. 이벤트 이름 `"called"`.
    Called,
    /// 곁에 와서 기다림. 이벤트 이름 `"waiting"`.
    Waiting,
    /// 자리로 돌아가는 중. 이벤트 이름 `"returning"`.
    Returning,
}

impl SummonPhase {
    /// 이벤트에 쓰는 이름.
    pub fn as_str(self) -> &'static str {
        match self {
            SummonPhase::Idle => "idle",
            SummonPhase::Called => "called",
            SummonPhase::Waiting => "waiting",
            SummonPhase::Returning => "returning",
        }
    }
}

/// 왜 왔는가(M12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SummonReason {
    /// 사용자가 불렀다. 이벤트 이름 `"call"`.
    Call,
    /// 팀 방에서 멘션됐다. 이벤트 이름 `"mention"`.
    Mention,
}

impl SummonReason {
    /// 이벤트에 쓰는 이름.
    pub fn as_str(self) -> &'static str {
        match self {
            SummonReason::Call => "call",
            SummonReason::Mention => "mention",
        }
    }
}

/// 곁에서 기다리는 담당이 대화 없이 이만큼(밀리초) 지나면 돌아간다(원본 10초).
pub const SUMMON_IDLE_MS: i64 = 10_000;

/// 부른 뒤 도착 알림 없이 이만큼(밀리초) 지나면 자리로 되돌린다(N10 · 결정 17).
pub const SUMMON_ARRIVE_MS: i64 = 30_000;

/// 담당의 일(`thinking` · `queued`)이 이만큼(밀리초) 바뀌지 않으면 한가로 푼다(N15).
pub const STALE_WORK_MS: i64 = 300_000;
