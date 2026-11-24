//! 논의 하나를 도는 동안의 옵션 상태(10차): 진행 신호 · 호출 예산 · 다시 시도 · 정리 모드.
//!
//! 옵션이 기본(신호 없음 · 예산 없음 · 다시 시도 0)이면 아무것도 하지 않고 언제나 호출을 허락한다.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::progress::{CallBudget, DiscussionOptions, ProgressSignal, ProgressSink};

/// 논의 하나의 옵션 상태.
pub(super) struct Run {
    signals: Option<Arc<dyn ProgressSink>>,
    budget: Option<CallBudget>,
    retries: u32,
    /// 이 논의에서 쓴 호출 수(담당 턴 · 다시 시도 · 점수 턴 · 팀장 턴).
    used: AtomicU32,
    /// 정리 모드에 들어갔는가.
    wrapping: AtomicBool,
    /// 호출이 모자라 점수 턴을 기권한 담당이 있는가.
    abstained: AtomicBool,
    /// 신호 포트가 실패한 횟수. 논의는 그대로 가고 여기에만 남긴다.
    signal_failures: AtomicU32,
}

impl Run {
    pub(super) fn new(options: DiscussionOptions) -> Run {
        Run {
            signals: options.signals,
            budget: options.budget,
            retries: options.turn_retries,
            used: AtomicU32::new(0),
            wrapping: AtomicBool::new(false),
            abstained: AtomicBool::new(false),
            signal_failures: AtomicU32::new(0),
        }
    }

    /// 신호 하나를 낸다. 포트가 없으면 아무것도 하지 않고, 실패하면 횟수만 센다.
    pub(super) fn signal(&self, signal: ProgressSignal) {
        if let Some(sink) = &self.signals
            && sink.signal(signal).is_err()
        {
            self.signal_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// 다시 시도 상한(옵션으로 받은 값).
    pub(super) fn retries(&self) -> u32 {
        self.retries
    }

    /// 남은 호출 수. 예산이 없으면 `None`.
    fn remaining(&self) -> Option<u32> {
        self.budget.map(|b| {
            b.remaining
                .saturating_sub(self.used.load(Ordering::Relaxed))
        })
    }

    /// 담당 턴(논의 · 점수 · 다시 시도) 하나를 쓸 수 있으면 하나를 쓰고 참이다.
    /// 예비분(`reserve`)은 쓰지 않는다. 예산이 없으면 언제나 참이다.
    pub(super) fn member_call(&self) -> bool {
        match (self.budget, self.remaining()) {
            (Some(budget), Some(left)) if left <= budget.reserve => false,
            _ => {
                self.used.fetch_add(1, Ordering::Relaxed);
                true
            }
        }
    }

    /// 팀장 판단 턴의 호출 하나를 쓴다(예비분에서 나간다).
    pub(super) fn leader_call(&self) {
        self.used.fetch_add(1, Ordering::Relaxed);
    }

    /// 새 라운드를 열기 전에: 남은 호출이 문턱 이하인가. 예산이 없으면 거짓.
    pub(super) fn wrap_up_due(&self) -> bool {
        match (self.budget, self.remaining()) {
            (Some(budget), Some(left)) => left <= budget.wrap_up_at,
            _ => false,
        }
    }

    /// 정리 모드에 들어간다.
    pub(super) fn enter_wrap_up(&self) {
        self.wrapping.store(true, Ordering::Relaxed);
    }

    /// 정리 모드인가(팀장 턴에 정리 한 줄을 넣는다).
    pub(super) fn wrapping(&self) -> bool {
        self.wrapping.load(Ordering::Relaxed)
    }

    /// 호출이 모자라 점수 턴을 기권한 담당이 있음을 남긴다.
    pub(super) fn abstain(&self) {
        self.abstained.store(true, Ordering::Relaxed);
    }

    /// 기권한 담당이 있는가.
    pub(super) fn abstained(&self) -> bool {
        self.abstained.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SignalError;

    struct Failing;

    impl ProgressSink for Failing {
        fn signal(&self, _signal: ProgressSignal) -> Result<(), SignalError> {
            Err(SignalError("닫힘".to_string()))
        }
    }

    fn budgeted(remaining: u32, reserve: u32, wrap_up_at: u32) -> Run {
        Run::new(DiscussionOptions::default().with_budget(CallBudget {
            remaining,
            reserve,
            wrap_up_at,
        }))
    }

    #[test]
    fn 예산이_없으면_언제나_허락하고_정리_모드가_없다() {
        let run = Run::new(DiscussionOptions::default());
        for _ in 0..1000 {
            assert!(run.member_call());
        }
        assert!(!run.wrap_up_due());
        assert_eq!(run.retries(), 0);
    }

    #[test]
    fn 담당_턴은_예비분을_쓰지_못하고_팀장은_쓴다() {
        let run = budgeted(4, 2, 0);
        assert!(run.member_call());
        assert!(run.member_call());
        assert!(!run.member_call());
        run.leader_call();
        assert_eq!(run.remaining(), Some(1));
    }

    #[test]
    fn 문턱_이하면_정리할_때다() {
        let run = budgeted(6, 2, 5);
        assert!(!run.wrap_up_due());
        assert!(run.member_call());
        assert!(run.wrap_up_due());
    }

    #[test]
    fn 신호_포트가_실패하면_횟수만_센다() {
        let run = Run::new(DiscussionOptions::default().with_signals(Arc::new(Failing)));
        run.signal(ProgressSignal::Closed);
        run.signal(ProgressSignal::LeaderTurn);
        assert_eq!(run.signal_failures.load(Ordering::Relaxed), 2);
    }
}
