//! 하루 시작 · 마감(V5), 팀장의 오늘 판단(E19 · V7), 손실 · 멈춤 판단을 저장소 전이로(V8 · V9),
//! 다시 시작(C8 · E9), 팀 상태 화면 값.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::Quote;
use pi_rules::{Book, StopContext, StopTransition, TeamStatus, evaluate_stops};
use pi_store::{TeamCause, TeamState};
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::base::{Claim, exec_text, lock, millis};
use super::{
    DayPlan, KIND_DAY_CLOSE, KIND_DAY_PLAN, KIND_DAY_START, TEAM_ANOMALY_HALT,
    TEAM_DAILY_LIMIT_HALT, TEAM_FULL_STOP, TEAM_OPERATING, TEAM_RESTING, Trading,
};
use crate::TradingError;

/// 하루 시작 · 마감을 다시 보는 간격(시계의 쉬기로 잰다).
const TICK: Duration = Duration::from_secs(1);

/// 다시 시작(C8)의 결과.
pub(super) enum Restart {
    /// 쉬는 중이 됐다.
    Done(TeamState),
    /// 멈춰 있지 않다(운용 중 · 쉬는 중 · 하루 한도 멈춤).
    NotStopped,
    /// 필수 키(증권사 · AI)를 먼저 다시 등록해야 한다(E9).
    KeysRequired,
}

/// 저장소의 팀 상태 → 규칙 엔진의 팀 상태.
pub(super) fn status(state: TeamState) -> TeamStatus {
    match state {
        TeamState::Operating => TeamStatus::Operating,
        TeamState::Resting => TeamStatus::Resting,
        TeamState::DailyLimitHalt => TeamStatus::DailyLimitHalt,
        TeamState::AnomalyHalt => TeamStatus::AnomalyHalt,
        TeamState::FullStop => TeamStatus::FullStop,
    }
}

/// 팀 상태 글자.
pub(super) fn team_word(state: TeamState) -> &'static str {
    match state {
        TeamState::Operating => TEAM_OPERATING,
        TeamState::Resting => TEAM_RESTING,
        TeamState::DailyLimitHalt => TEAM_DAILY_LIMIT_HALT,
        TeamState::AnomalyHalt => TEAM_ANOMALY_HALT,
        TeamState::FullStop => TEAM_FULL_STOP,
    }
}

fn date_text(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

impl Trading {
    /// 하루 시작 · 마감을 뒤에서 돌린다. `stop`이 참이 되면(또는 알리는 쪽이 사라지면) 멈춘다.
    pub(crate) fn spawn_days(self: &Arc<Self>, mut stop: watch::Receiver<bool>) -> JoinHandle<()> {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop.wait_for(|stop| *stop) => return,
                    () = me.clock.sleep(TICK) => {}
                }
                me.tick().await;
            }
        })
    }

    /// 지금 시각으로 하루 시작 · 마감을 한 번 본다. 실패는 기록만 하고 다음 번에 다시 본다.
    pub(crate) async fn tick(&self) {
        if let Err(error) = self.try_tick().await {
            tracing::warn!(%error, "하루 시작 · 마감을 보지 못했습니다");
        }
        if let Err(error) = self.settle_due().await {
            tracing::warn!(%error, "답 없는 승인 요청을 처리하지 못했습니다");
        }
        // 실패한 파는 주문(손절 · 팀장 팔기 등)을 때가 되면 다시 낸다(E30).
        self.retry_sells().await;
    }

    async fn try_tick(&self) -> Result<(), TradingError> {
        let now = self.clock.now();
        let Some(calendar) = self.calendar(now) else {
            return Ok(());
        };
        self.executor.on_time().map_err(exec_text)?;
        let sessions = &calendar.today.sessions;
        let open = sessions.iter().map(|s| s.start).min();
        let close = sessions.iter().map(|s| s.end).max();
        let (Some(open), Some(close)) = (open, close) else {
            return Ok(());
        };
        let date = calendar.today.date;
        // 뒤에서 도는 tick과 하루 판단의 tick이 겹쳐도 그 날에 한 번만 남기게, 할 일을 잠금 안에서 먼저 잡는다(V5).
        let claimed = lock(&self.days).claim(date, now >= open && now < close, now >= close);
        let done = match claimed {
            Some(Claim::Start(_)) => self.day_start(date, now).await,
            Some(Claim::Close(_)) => self.day_close(date).await,
            None => return Ok(()),
        };
        if done.is_err()
            && let Some(claim) = claimed
        {
            // 남기지 못했으면 잡은 것을 되돌려 다음 번에 다시 본다.
            lock(&self.days).release(date, claim);
        }
        done
    }

    /// 하루 시작: 남기고, 그날 시작 평가액을 새로 잡고, 하루 한도 멈춤을 쉬는 중으로 푼다.
    async fn day_start(&self, date: NaiveDate, now: DateTime<Utc>) -> Result<(), TradingError> {
        self.books_for_day_start(now, date).await?;
        self.record(KIND_DAY_START, json!({ "date": date_text(date) }))
            .await?;
        // 세션이 시작되면 손절 · 청산의 물러서기를 비워 곧바로 다시 낸다(10b 보강 2 1번 다).
        self.forget_urgent_backoff(now);
        let _turn = lock(&self.team_turn);
        if self.store.team_state()? == TeamState::DailyLimitHalt {
            let cause = TeamCause::NextTradingDayStart;
            self.change_team(TeamState::Resting, cause, false, "다음 거래일 시작")?;
        }
        Ok(())
    }

    /// 하루 마감: 남기고, 운용 중이었으면 쉬는 중으로 바꾼다.
    async fn day_close(&self, date: NaiveDate) -> Result<(), TradingError> {
        self.record(KIND_DAY_CLOSE, json!({ "date": date_text(date) }))
            .await?;
        // 오늘 정규장의 마지막 시세가 다음 거래일의 기준가다(결정 2026-11-21 11번).
        self.save_references(date)?;
        {
            let _turn = lock(&self.team_turn);
            if self.store.team_state()? == TeamState::Operating {
                let cause = TeamCause::MarketClose;
                self.change_team(TeamState::Resting, cause, false, "장 마감")?;
            }
        }
        self.announce_close(date);
        // 승격대기에 갇힌 템플릿을 먼저 맞춰 본 뒤(되돌린 것은 이 마감에서 다시 잰다) 기준을 잰다.
        self.reconcile_promotions();
        self.consider_promotion(date).await;
        Ok(())
    }

    /// [`Trading::decide_day`]의 몸통.
    pub(super) fn decide(&self, plan: DayPlan) -> Result<TeamState, TradingError> {
        self.require_setup()?;
        let now = self.clock.now();
        let state = {
            let _turn = lock(&self.team_turn);
            let current = self.store.team_state()?;
            match &plan {
                DayPlan::Trade => self.start_trading(current)?,
                // 쉬는 결정에 맞는 사유가 따로 없어 "장 마감"(오늘 장을 닫는다)으로 남기고 까닭을 적는다.
                DayPlan::Rest { .. } if current == TeamState::Operating => {
                    let detail = "팀장이 오늘 쉬기로 함";
                    self.change_team(TeamState::Resting, TeamCause::MarketClose, false, detail)?
                }
                DayPlan::Rest { .. } => current,
            }
        };
        let date = self.date_of(now);
        let (word, reason) = match &plan {
            DayPlan::Trade => ("trade", Value::Null),
            DayPlan::Rest { reason } => ("rest", json!(reason)),
        };
        let payload = json!({ "date": date_text(date), "plan": word, "reason": reason });
        self.store.append_at(KIND_DAY_PLAN, &payload, millis(now))?;
        lock(&self.days).plan = Some((date, plan));
        Ok(state)
    }

    /// 오늘 매매한다: 쉬는 중이면 운용 중으로. 멈춰 있으면 [`TradingError::Halted`].
    ///
    /// 쉬는 중에는 저장소가 멈춤으로 가는 전이를 받지 않아(예: 전체 정지에서 다시 시작한 직후 손실이
    /// 아직 한도 밖) 손실 판단의 요구가 미뤄져 있을 수 있다. 운용 중이 된 이 자리에서 마지막 장부로
    /// 다시 판단해, 요구가 있으면 허용된 길(운용 중 → 멈춤)로 멈추고 [`TradingError::Halted`]를 돌려준다.
    pub(super) fn start_trading(&self, current: TeamState) -> Result<TeamState, TradingError> {
        match current {
            TeamState::Operating => Ok(current),
            TeamState::Resting => {
                let cause = TeamCause::TradingStart;
                let state = self.change_team(TeamState::Operating, cause, false, "오늘 매매")?;
                let after = self.enforce_stops(state, &self.last_books())?;
                if after == state {
                    Ok(state)
                } else {
                    Err(TradingError::Halted(after))
                }
            }
            halted => Err(TradingError::Halted(halted)),
        }
    }

    /// 손실 · 멈춤 판단(`pi_rules::evaluate_stops`)이 요구한 전이를 저장소에 한다. 팀 상태 줄을 쥐고 부른다.
    /// 저장소가 받지 않는 전이(쉬는 중 → 멈춤 등)는 하지 않고 지금 상태를 돌려준다.
    fn enforce_stops(&self, team: TeamState, books: &[Book]) -> Result<TeamState, TradingError> {
        let Some(params) = self.params()? else {
            return Ok(team);
        };
        let context = StopContext {
            team: status(team),
            params: &params,
            books,
        };
        match evaluate_stops(&context) {
            None => Ok(team),
            Some(StopTransition::DailyLimitHalt) => {
                let cause = TeamCause::DailyLossLimit;
                self.change_team(TeamState::DailyLimitHalt, cause, false, "하루 손실 한도")
            }
            Some(StopTransition::FullStop) => {
                let cause = TeamCause::MaxLoss;
                self.change_team(TeamState::FullStop, cause, false, "최대 손실")
            }
        }
    }

    /// [`Trading::on_quote`]의 몸통: 모의계좌 체결 → 매매 상태 → 손절 감시 → 장부 → 손실 · 멈춤 판단.
    pub(super) async fn take_quote(&self, quote: Quote) -> Result<(), TradingError> {
        self.remember(&quote);
        let now = self.clock.now();
        let calendar = self.calendar(now);
        self.observe(&quote, calendar.as_ref());
        self.executor.on_quote(&quote).map_err(exec_text)?;
        self.watch_stops(&quote).await;
        let books = self.books(now).await?;
        self.judge_losses(&books)
    }

    fn judge_losses(&self, books: &[Book]) -> Result<(), TradingError> {
        let _turn = lock(&self.team_turn);
        let team = self.store.team_state()?;
        self.enforce_stops(team, books).map(|_| ())
    }

    /// 실행기가 이상 감지 멈춤을 요구했다(E13).
    pub(super) fn anomaly(&self) -> Result<TeamState, TradingError> {
        // 비정상이 생겼다: 스스로 풀리는 정상 시간을 처음부터 다시 센다.
        self.unhealthy();
        let _turn = lock(&self.team_turn);
        let cause = TeamCause::AnomalyDetected;
        let detail = "증권사가 주문을 받지 않거나 응답하지 않음";
        self.change_team(TeamState::AnomalyHalt, cause, false, detail)
    }

    /// 다시 시작(C8): 전체 정지 · 이상 감지 멈춤이고 필수 키가 모두 있으면 쉬는 중으로.
    pub(super) fn restart(&self) -> Result<Restart, TradingError> {
        let state = {
            let _turn = lock(&self.team_turn);
            let current = self.store.team_state()?;
            if !matches!(current, TeamState::FullStop | TeamState::AnomalyHalt) {
                return Ok(Restart::NotStopped);
            }
            if !self.desk.keys_ready() {
                return Ok(Restart::KeysRequired);
            }
            let cause = TeamCause::OwnerRestart;
            self.change_team(TeamState::Resting, cause, true, "내가 다시 시작")?
        };
        // 전체 정지 중에 받아 둔 실전 승격 승인이 있으면 이제 올린다(10a 보강 5번).
        self.reconcile_promotions();
        Ok(Restart::Done(state))
    }

    /// 오늘 쉬는 날이면 그 이유.
    pub(super) fn rest_reason(&self, now: DateTime<Utc>) -> Option<String> {
        let today = self.date_of(now);
        match &lock(&self.days).plan {
            Some((date, DayPlan::Rest { reason })) if *date == today => Some(reason.clone()),
            _ => None,
        }
    }

    /// 팀 상태 화면 값([`super::TEAM_PATH`]).
    pub(super) fn view(&self) -> Result<Value, TradingError> {
        let state = self.store.team_state()?;
        let today = self.date_of(self.clock.now());
        let (plan, reason) = match &lock(&self.days).plan {
            Some((date, DayPlan::Trade)) if *date == today => (json!("trade"), Value::Null),
            Some((date, DayPlan::Rest { reason })) if *date == today => {
                (json!("rest"), json!(reason))
            }
            _ => (Value::Null, Value::Null),
        };
        let notice = self.template_notice();
        Ok(json!({ "state": team_word(state), "plan": plan, "reason": reason, "notice": notice }))
    }
}
