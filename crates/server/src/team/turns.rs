//! (54차, 기능 F) 하루 AI 상한을 AI 호출(턴) 단위로 센다(N14 · E52 · C22 · R1-5 · R5-12).
//!
//! - 팀의 "쓴 수"는 실제 AI 턴 수다: 논의 안의 담당 턴 · 다시 시도 · 점수 턴 · 팀장 판단(진행 신호마다 하나),
//!   논의 밖의 깨우기 턴 · 사무실 채팅 답 · 공지 확인 답 · 이어 말하기 · 교훈 턴이 하나씩이다. 셀 때마다 그 담당의
//!   사용에도 하나를 더하므로 담당별 사용의 합 = 팀의 쓴 수다. 한국 날짜로 저장해 다시 켜도 그날 것이 남는다.
//! - 논의를 열 때 남은 호출 · 팀장 예비분 · 정리 문턱([`CallBudget`])을 agent에 넘긴다. agent가 남은 호출이 정리
//!   문턱 이하면 새 라운드 없이 팀장 판단으로 넘어가고(정리 모드), 예비분만 남으면 담당 턴 없이 팀장만 부른다.
//!   남은 호출이 없으면(예비분도 없으면) 논의를 열지 않는다(상한 오류 · 그날 한 번 알림은 지금 그대로).
//! - 깨우기 · 공지 확인 답 · 이어 말하기 · 메신저 묻기 · 교훈 턴은 예비분을 쓰지 않는다(E52 — 팀장 예비분은 다른
//!   담당이 쓰지 않는다). 사람이 사무실에서 직접 물은 채팅 답은 막지 않고 세기만 한다(지금처럼 막지 않는다).
//! - 셈과 잔량 바의 모드(`mode`)는 같은 N14 값(예비분 · 문턱)으로 판정한다([`call_budget`]).
//! - 판단 뒤 매매 담당의 주문 안 턴 · 하루 판단(팀장) · 복기 턴은 agent가 신호를 내지 않는 턴이라 세지 않는다
//!   (지금 그대로 — 보고의 남은 문제).

use pi_agent::CallBudget;
use pi_office::Agent;

use super::Crew;
use super::notices::leader_reserve;
use crate::TeamError;

/// N14 예산: 상한 · 그날 쓴 수로 남은 호출 · 팀장 예비분 · 정리 문턱(예비분 + 상한의 20% 올림).
pub(super) fn call_budget(limit: u32, used: u32) -> CallBudget {
    let reserve = leader_reserve(limit);
    CallBudget {
        remaining: limit.saturating_sub(used),
        reserve,
        wrap_up_at: reserve + limit.div_ceil(5),
    }
}

/// 잔량 바의 모드(N14): 예비분 이하면 팀장 몫만, 정리 문턱 이하면 정리 모드.
pub(super) fn mode_name(budget: CallBudget) -> &'static str {
    if budget.remaining <= budget.reserve {
        "leader_only"
    } else if budget.remaining <= budget.wrap_up_at {
        "wrap_up"
    } else {
        "normal"
    }
}

impl Crew {
    /// 지금 하루 상한(켤 때 값, 없으면 설정 화면이 저장한 값).
    pub(super) fn day_limit(&self) -> Option<u32> {
        self.wiring
            .ai_daily_limit
            .or_else(|| self.desk.saved_ai_limit())
    }

    /// 논의를 열 때 agent에 넘기는 예산. 상한이 없으면 없음(막지 않는다).
    pub(super) fn discussion_budget(&self) -> Option<CallBudget> {
        let limit = self.day_limit()?;
        Some(call_budget(limit, self.ai_cap.peek(self.clock.now())))
    }

    /// 논의를 열어도 되는가: 남은 호출이 하나도 없으면(예비분도 없으면) 열지 않고 그날 한 번 알린다.
    /// 상한이 없으면 늘 연다. 세지는 않는다(논의 안의 턴은 진행 신호마다 센다).
    pub(super) fn open_discussion(&self) -> Result<(), TeamError> {
        match self.discussion_budget() {
            Some(budget) if budget.remaining == 0 => {
                self.tell_capped();
                Err(TeamError::AiLimit)
            }
            _ => Ok(()),
        }
    }

    /// 이미 쓴 AI 턴 하나를 센다: 팀의 쓴 수와 그 담당의 사용에 하나씩(막지 않는다). 잔량 이벤트를 낸다.
    pub(crate) fn count_turn(&self, agent: Agent) {
        let now = self.clock.now();
        self.ai_cap.take_within(now, u32::MAX);
        self.meter.add(now, agent);
        self.publish_budget();
    }

    /// 담당 턴 하나(깨우기 등)를 팀장 예비분 밖에서 잡는다. 잡았으면 그 담당의 호출로 센다. 못 잡았는데 상한을
    /// 다 썼으면 그날 한 번 알린다(예비분만 남은 것은 알리지 않는다 — 논의는 팀장 몫으로 아직 열린다).
    pub(super) fn take_member_turn(&self, agent: Agent) -> bool {
        if self.take_spare_ai_call_by(agent) {
            return true;
        }
        if self
            .discussion_budget()
            .is_some_and(|budget| budget.remaining == 0)
        {
            self.tell_capped();
        }
        false
    }

    /// 상한에 닿았다고 그날 한 번 알린다.
    fn tell_capped(&self) {
        let today = super::quota::kst_date(self.clock.now());
        let first_today = {
            let mut told = self
                .ai_cap_told
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let first = *told != Some(today);
            *told = Some(today);
            first
        };
        if first_today {
            tracing::warn!("오늘 AI 호출 상한에 닿아 논의를 쉽니다");
            if let Some(notify) = self.links.get().and_then(|links| links.ai_capped.as_ref()) {
                notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 상한_10이면_예비분_2_정리_문턱_4이고_쓴_수만큼_남는다() {
        let budget = call_budget(10, 3);
        assert_eq!(budget.remaining, 7);
        assert_eq!(budget.reserve, 2);
        assert_eq!(budget.wrap_up_at, 4);
        assert_eq!(call_budget(10, 12).remaining, 0, "넘게 써도 0에서 멈춘다");
        assert_eq!(call_budget(100, 0).wrap_up_at, 30);
    }

    #[test]
    fn 잔량_바의_모드는_논의에_넘기는_예산과_같은_판정이다() {
        for limit in [1, 2, 7, 10, 21, 100] {
            for used in 0..=limit + 2 {
                let budget = call_budget(limit, used);
                let shown = super::super::budget::budget_json(Some(limit), used, &[0; 6]);
                assert_eq!(
                    shown["mode"],
                    mode_name(budget),
                    "상한 {limit} · 쓴 수 {used}"
                );
                assert_eq!(shown["reserve"], budget.reserve);
                assert_eq!(shown["wrap_up_at"], budget.wrap_up_at);
            }
        }
    }
}
