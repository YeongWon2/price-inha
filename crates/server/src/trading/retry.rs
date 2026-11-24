//! (10차 10b 0번 가) 실패한 파는 주문 다시 내기(E30): 하루 tick마다 실행기가 다시 내기를 기다리는 파는 주문
//! (`Executor::pending_sells`)을 보고, 때가 된 것을 그 자리의 규칙 점검을 거쳐 `Executor::retry_sell`로 다시 낸다.
//!
//! - 다시 낼 주문 안은 실행기의 손절 주문 안(`stop_draft`: 그 매매의 근거 · 자료)에 처음 낸 주문의 목적 · 호가 유형 ·
//!   값 · 수량을 얹어 만든다. 점검 통과 표시는 규칙 엔진의 길(`pi_rules::issue_for_trade`)로만 받는다.
//! - 또 실패해 실행기가 이상 감지 멈춤을 요구하면 팀을 이상 감지 멈춤으로 옮긴다(첫 실패와 같은 길).
//! - (10b 보강 5번) 막힌 다시 내기(점검에서 막힘 · 점검 입력이 모자람 · 승인을 요구함 · 보유 자리나 시세가 없음 ·
//!   표시 내용이 처음 낸 것과 다름 `PermitMismatch`)는 **그 대기만** 물러선다: 다음에 볼 때까지의 간격을 1초에서
//!   막힐 때마다 두 배로(최대 10분) 늘린다. 같은 까닭의 기록은 처음 한 번만 남긴다. 손절 대기가 막힌 때부터
//!   10분 넘게 이어지면 사용자에게 한 번 알린다("손절을 다시 내지 못하고 있습니다" — 종목 · 수량만, 까닭 글은
//!   싣지 않는다). 다시 냈거나 대기가 사라지면 물러서기를 지운다(기억은 기다리는 대기만큼만 둔다).
//! - (10b 보강 2 1번) 손절 · 청산의 물러서기는 손절을 늦추지 않는다: (가) 장 밖(`MarketClosed`) · 달력 없음으로
//!   막히면 다음에 볼 때를 다음 세션 시작 시각 이하로 자른다(그 시각을 모르면 5초 뒤), (나) 그 밖의 까닭으로 막히면
//!   간격 상한이 5초다(익절 · 팀장 팔기는 10분 그대로), (다) 하루가 시작되면 손절 · 청산의 물러서기 간격을 비운다
//!   (같은 까닭 기록 한 번 · 10분 알림을 위한 까닭 · 처음 막힌 때는 둔다).
//! - (16차 1번) 팀 상태가 전체 정지면 팀장 팔기(`TeamExit`) 대기를 지운다(정지 전의 팀장 판단이 다시 시작 뒤에 저절로
//!   나가지 않게). 방 `trade-<번호>`에 매매마다 한 줄. 손절 · 청산 · 익절 대기는 남긴다. 상태 전이 한 곳이 아니라
//!   tick마다 보므로 저장소를 직접 바꾸는 길 · 백업 복원도 덮는다.
//! - (16차 4번) 손절 · 청산 대기가 낡았으면(`SellTooLarge` · `PermitMismatch`로 거부, 또는 매매가 보유중이 아니게 됨)
//!   지운다. 지금 남은 수량이 0보다 크면 그 수량으로 같은 목적의 파는 주문을 새 표시로 낸다. 방 기록에 한 줄.
//!   (17차 1 · 2번) 새로 내기는 낸 뒤에 지운다: 막히거나 실패하면 대기를 그대로 두고 물러선다. 청산은 처음 주문 모양을 지킨다.

use chrono::{DateTime, Duration, Utc};
use pi_exec::{SellPurpose, SellRetry};
use pi_rules::{Blocked, Outcome, Purpose, Reason, SymbolStatus, Violation};
use pi_store::{NewRoomEvent, RoomActorKind, TeamState, TradeId, TradeState};
use rust_decimal::Decimal;

use super::base::{lock, millis};
use super::{OrderRequest, Trading};
use crate::TradingError;

/// 막힌 다시 내기의 첫 물러서기 간격(초).
const BACKOFF_FIRST_SECONDS: i64 = 1;
/// 막힌 다시 내기의 물러서기 간격 상한(초, 10분).
const BACKOFF_MAX_SECONDS: i64 = 600;
/// 손절 · 청산 다시 내기의 물러서기 간격 상한(초, 10b 보강 2 1번 나).
const URGENT_BACKOFF_MAX_SECONDS: i64 = 5;
/// 손절 대기가 이만큼(초) 넘게 막혀 있으면 알린다(10분).
const STUCK_STOP_SECONDS: i64 = 600;

/// 막힌 다시 내기 하나의 물러서기.
pub(super) struct HeldBack {
    /// 다음에 다시 볼 시각.
    next_at: DateTime<Utc>,
    /// 지금 간격(초).
    interval: i64,
    /// 마지막으로 남긴 까닭(같은 까닭은 다시 남기지 않는다).
    reason: String,
    /// 처음 막힌 시각.
    since: DateTime<Utc>,
    /// 오래 막혔다고 알렸나.
    told: bool,
}

/// 다시 내 본 결과.
enum Tried {
    /// 다시 냈다(또는 아직 때가 아니라 실행기가 받지 않았다).
    Done,
    /// 막혔다.
    Blocked(Hold),
}

/// 막힌 까닭.
struct Hold {
    /// 사람이 읽는 까닭.
    reason: String,
    /// 장 밖(`MarketClosed`) · 달력 없음으로 막혔나.
    closed: bool,
}

impl Hold {
    fn other(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            closed: false,
        }
    }

    fn of(blocked: &Blocked) -> Self {
        let closed = matches!(&blocked.reason, Reason::Violations(list) if list.contains(&Violation::MarketClosed));
        Self {
            reason: blocked.detail(),
            closed,
        }
    }
}

/// 다음 물러서기 간격(초): 처음은 1초, 그 뒤로 두 배씩, 최대 `max`초.
fn next_interval(current: i64, max: i64) -> i64 {
    if current <= 0 {
        BACKOFF_FIRST_SECONDS.min(max)
    } else {
        current.saturating_mul(2).min(max)
    }
}

/// 손절 · 청산인가(물러서기가 손절을 늦추면 안 되는 파는 주문).
fn urgent(purpose: SellPurpose) -> bool {
    matches!(
        purpose_of(purpose),
        Purpose::StopLoss | Purpose::Liquidation
    )
}

/// 실행기의 파는 목적 → 규칙 엔진의 목적.
fn purpose_of(purpose: SellPurpose) -> Purpose {
    match purpose {
        SellPurpose::StopLoss => Purpose::StopLoss,
        SellPurpose::TakeProfit => Purpose::TakeProfit,
        SellPurpose::TeamExit => Purpose::TeamExit,
        _ => Purpose::Liquidation,
    }
}

impl Trading {
    /// 다시 내기를 기다리는 파는 주문 가운데 때가 된 것을 다시 낸다. 실패는 기록만 하고 다음 tick에 다시 본다.
    /// 막힌 대기는 물러선 간격이 지나야 다시 본다.
    pub(super) async fn retry_sells(&self) {
        let now = self.clock.now();
        self.withdraw_team_exits();
        self.drop_ended_urgent();
        let pending = self.executor.pending_sells();
        lock(&self.held_back).retain(|trade, _| pending.iter().any(|p| p.trade_id == *trade));
        for pending in pending {
            if pending.retry_at > now {
                continue;
            }
            let trade = pending.trade_id;
            let waiting = lock(&self.held_back)
                .get(&trade)
                .is_some_and(|held| held.next_at > now);
            if waiting {
                continue;
            }
            match self.retry_one(&pending, now).await {
                Ok(Tried::Done) => {
                    lock(&self.held_back).remove(&trade);
                }
                Ok(Tried::Blocked(hold)) => self.hold_back(&pending, hold, now),
                Err(error) => {
                    tracing::warn!(%error, trade = trade.0, "실패한 파는 주문을 다시 내지 못했습니다");
                }
            }
        }
    }

    /// 막힌 대기를 물러서게 한다: 간격을 두 배로(최대 10분, 손절 · 청산은 5초), 새 까닭이면 한 번 남기고,
    /// 손절이 오래 막혔으면 알린다. 손절 · 청산이 장 밖 · 달력 없음으로 막혔으면 다음 세션 시작 시각보다 늦게 보지 않는다.
    fn hold_back(&self, pending: &SellRetry, hold: Hold, now: DateTime<Utc>) {
        let trade = pending.trade_id;
        let Hold { reason, closed } = hold;
        let urgent = urgent(pending.purpose);
        let (max, latest) = match (urgent, closed) {
            (false, _) => (BACKOFF_MAX_SECONDS, None),
            (true, false) => (URGENT_BACKOFF_MAX_SECONDS, None),
            (true, true) => {
                let soon = now + Duration::seconds(URGENT_BACKOFF_MAX_SECONDS);
                (
                    BACKOFF_MAX_SECONDS,
                    Some(self.next_session_start(now).unwrap_or(soon)),
                )
            }
        };
        let (new_reason, tell) = {
            let mut held = lock(&self.held_back);
            let entry = held.entry(trade).or_insert_with(|| HeldBack {
                next_at: now,
                interval: 0,
                reason: String::new(),
                since: now,
                told: false,
            });
            entry.interval = next_interval(entry.interval, max);
            let next_at = now + Duration::seconds(entry.interval);
            entry.next_at = latest.map_or(next_at, |latest| next_at.min(latest));
            let new_reason = entry.reason != reason;
            if new_reason {
                entry.reason = reason.clone();
            }
            let stuck = pending.purpose == SellPurpose::StopLoss
                && now - entry.since > Duration::seconds(STUCK_STOP_SECONDS);
            let tell = stuck && !entry.told;
            entry.told |= tell;
            (new_reason, tell)
        };
        if new_reason {
            tracing::warn!(trade = trade.0, detail = %reason, "다시 낼 파는 주문이 막혔습니다(물러서며 다시 봅니다)");
        }
        if tell {
            let order = &pending.order;
            tracing::warn!(trade = trade.0, "손절을 다시 내지 못하고 있습니다");
            self.announce_stuck_stop(
                order.symbol.as_str(),
                &order.quantity.normalize().to_string(),
            );
        }
    }

    /// 지금 뒤로 가장 먼저 열리는 세션의 시작 시각(오늘 · 다음 영업일). 달력을 모르면 없음.
    fn next_session_start(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let calendar = self.calendar(now)?;
        [&calendar.today, &calendar.next_business_day]
            .into_iter()
            .flat_map(|day| day.sessions.iter())
            .map(|session| session.start)
            .filter(|start| *start > now)
            .min()
    }

    /// (10b 보강 2 1번 다) 하루가 시작되면 손절 · 청산의 물러서기 간격을 비워 곧바로 다시 보게 한다.
    pub(super) fn forget_urgent_backoff(&self, now: DateTime<Utc>) {
        let urgent: Vec<_> = self
            .executor
            .pending_sells()
            .into_iter()
            .filter(|pending| urgent(pending.purpose))
            .map(|pending| pending.trade_id)
            .collect();
        let mut held = lock(&self.held_back);
        for trade in urgent {
            if let Some(entry) = held.get_mut(&trade) {
                entry.interval = 0;
                entry.next_at = now;
            }
        }
    }

    /// 그 자리의 시세 · 장부 · 팀 상태로 다시 점검하고, 지나면 다시 낸다.
    async fn retry_one(
        &self,
        pending: &SellRetry,
        now: DateTime<Utc>,
    ) -> Result<Tried, TradingError> {
        let purpose = purpose_of(pending.purpose);
        let Some(request) = self.retry_request(pending, purpose) else {
            return Ok(Tried::Blocked(Hold::other("보유 자리나 시세가 없습니다")));
        };
        let params = self.params_for(purpose)?.ok_or(TradingError::NoTemplate)?;
        let inputs = match self.inputs(&request, now).await? {
            Ok(inputs) => inputs,
            Err(blocked) => return Ok(Tried::Blocked(Hold::of(&blocked))),
        };
        let passed = match pi_rules::check(&request.draft, &inputs.context(self, &params, &request))
        {
            Outcome::Pass(passed) => passed,
            Outcome::Blocked(blocked) => return Ok(Tried::Blocked(Hold::of(&blocked))),
            Outcome::NeedsApproval(_) => {
                return Ok(Tried::Blocked(Hold::other(
                    "승인을 요구합니다(다시 내지 않습니다)",
                )));
            }
        };
        let trade = pending.trade_id;
        let permit = pi_rules::issue_for_trade(&self.authority, passed, trade.0);
        match self.executor.retry_sell(trade, permit).await {
            Ok(id) => {
                tracing::info!(trade = trade.0, "실패한 파는 주문을 다시 냈습니다");
                if purpose == Purpose::StopLoss {
                    self.note_stop(trade, &super::Placed::Ordered(id));
                }
                Ok(Tried::Done)
            }
            Err(pi_exec::Error::RetryNotDue { .. }) => Ok(Tried::Done),
            Err(pi_exec::Error::SellTooLarge { .. } | pi_exec::Error::PermitMismatch)
                if urgent(pending.purpose) =>
            {
                self.renew_stale(pending, purpose).await
            }
            Err(pi_exec::Error::PermitMismatch) => Ok(Tried::Blocked(Hold::other(
                "표시 내용이 처음 낸 주문과 다릅니다(PermitMismatch)",
            ))),
            Err(pi_exec::Error::AnomalyStop(error)) => {
                self.anomaly()?;
                Err(TradingError::Broker(error.to_string()))
            }
            Err(error) => Err(super::base::exec_text(error)),
        }
    }

    /// (16차 1번) 전체 정지면 팀장 팔기 대기를 지우고 매매마다 방에 한 줄 남긴다. 손절 · 청산 · 익절은 남긴다.
    fn withdraw_team_exits(&self) {
        if !matches!(self.store.team_state(), Ok(TeamState::FullStop)) {
            return;
        }
        let exits: Vec<TradeId> = self
            .executor
            .pending_sells()
            .into_iter()
            .filter(|pending| pending.purpose == SellPurpose::TeamExit)
            .map(|pending| pending.trade_id)
            .collect();
        for trade in exits {
            if self.executor.forget_pending_sell(trade).is_some() {
                self.note_in_room(trade, "withdrawn", "전체 정지로 팀장 팔기 다시 내기를 거둠");
            }
        }
    }

    /// (16차 4번) 매매가 더는 보유중이 아닌 손절 · 청산 대기는 낸 것이 없어도 영영 남지 않게 지운다(남은 수량 0).
    fn drop_ended_urgent(&self) {
        let ended: Vec<TradeId> = self
            .executor
            .pending_sells()
            .into_iter()
            .filter(|pending| urgent(pending.purpose))
            .map(|pending| pending.trade_id)
            .filter(|trade| {
                self.store
                    .get_trade(*trade)
                    .is_ok_and(|found| found.state != TradeState::Holding)
            })
            .collect();
        for trade in ended {
            if self.executor.forget_pending_sell(trade).is_some() {
                self.note_in_room(
                    trade,
                    "stale-cleared",
                    "남은 수량이 0이라 낡은 손절 · 청산 다시 내기를 지움",
                );
            }
        }
    }

    /// (16차 4번 · 17차 1 · 2 · 3번) 낡은 손절 · 청산 대기: 지금 남은 수량으로 같은 목적의 파는 주문을 먼저 내고, 나간 뒤에 대기를 지운다.
    /// 실행기 장부로 남은 수량을 모르면(감시 자리나 시세가 없음) 지우지도 새로 내지도 않고 물러선다.
    async fn renew_stale(
        &self,
        pending: &SellRetry,
        purpose: Purpose,
    ) -> Result<Tried, TradingError> {
        let trade = pending.trade_id;
        let Some(mut request) = self.retry_request(pending, purpose) else {
            return Ok(Tried::Blocked(Hold::other("보유 자리나 시세가 없습니다")));
        };
        let guard = self
            .executor
            .guards()
            .into_iter()
            .find(|guard| guard.trade_id == trade);
        let quote = guard
            .as_ref()
            .and_then(|g| self.last_quote(g.symbol.as_str()));
        let remaining = match (&guard, &quote) {
            (Some(guard), Some(quote)) => self
                .executor
                .stop_draft(guard, quote)
                .ok()
                .and_then(|draft| draft.ai_quantity),
            _ => None,
        };
        let Some(remaining) = remaining else {
            return Ok(Tried::Blocked(Hold::other("남은 수량을 알 수 없습니다")));
        };
        if remaining <= Decimal::ZERO {
            self.executor.forget_pending_sell(trade);
            self.note_in_room(
                trade,
                "stale-cleared",
                "남은 수량이 0이라 낡은 손절 · 청산 다시 내기를 지움",
            );
            return Ok(Tried::Done);
        }
        // (17차 1 · 2번) 대기를 지우기 전에 새로 낸다: 손절은 시장가, 청산은 처음 주문 종류 · 가격 그대로.
        if purpose == Purpose::StopLoss {
            request.draft.order_type = pi_paper::OrderType::Market;
            request.draft.limit_price = None;
        }
        request.draft.ai_quantity = Some(remaining);
        match self.place_order(request).await {
            Ok(super::Placed::Ordered(_)) => {
                // 실행기는 나간 매매의 대기를 스스로 지운다. 남아 있으면 여기서 지운다.
                self.executor.forget_pending_sell(trade);
                self.note_in_room(
                    trade,
                    "stale-renewed",
                    &format!(
                        "낡은 다시 내기를 지우고 남은 수량 {}주로 새로 냈습니다",
                        remaining.normalize()
                    ),
                );
                Ok(Tried::Done)
            }
            Ok(other) => {
                tracing::warn!(
                    trade = trade.0,
                    ?other,
                    "남은 수량으로 새로 내지 못했습니다"
                );
                Ok(Tried::Blocked(Hold::other(
                    "남은 수량으로 새로 내지 못했습니다(대기는 그대로)",
                )))
            }
            Err(error) => {
                tracing::warn!(%error, trade = trade.0, "남은 수량으로 새로 내지 못했습니다");
                Ok(Tried::Blocked(Hold::other(
                    "남은 수량으로 새로 내지 못했습니다(대기는 그대로)",
                )))
            }
        }
    }

    /// 그 매매의 방 `trade-<번호>`에 서버가 남기는 한 줄(같은 종류는 매매 · 시각마다 겹치지 않는 키).
    fn note_in_room(&self, trade: TradeId, kind: &str, text: &str) {
        let now = self.clock.now();
        let room = format!("{}{}", crate::TRADE_ROOM_PREFIX, trade.0);
        let event = NewRoomEvent {
            room: room.clone(),
            event_id: format!("server-{kind}-{}-{}", trade.0, now.timestamp_millis()),
            kind: "room.activity".to_string(),
            actor_kind: RoomActorKind::Gateway,
            actor_id: "server".to_string(),
            payload: serde_json::json!({ "text": text }),
            at: millis(now),
        };
        if let Err(error) = self.store.append_room_event(event) {
            tracing::warn!(%error, trade = trade.0, "방 기록을 남기지 못했습니다");
        }
    }

    /// 다시 낼 주문 안: 그 매매의 손절 주문 안(근거 · 자료)에 처음 낸 주문의 목적 · 호가 · 값 · 수량을 얹는다.
    /// 보유 자리나 시세가 없으면 없음(물러서며 다시 본다).
    fn retry_request(&self, pending: &SellRetry, purpose: Purpose) -> Option<OrderRequest> {
        let guard = self
            .executor
            .guards()
            .into_iter()
            .find(|guard| guard.trade_id == pending.trade_id)?;
        let quote = self.last_quote(guard.symbol.as_str())?;
        let mut draft = match self.executor.stop_draft(&guard, &quote) {
            Ok(draft) => draft,
            Err(error) => {
                tracing::warn!(%error, "다시 낼 파는 주문 안을 만들지 못했습니다");
                return None;
            }
        };
        let order = &pending.order;
        draft.purpose = purpose;
        draft.order_type = order.order_type;
        draft.limit_price = order.price;
        draft.ai_quantity = Some(order.quantity);
        Some(OrderRequest {
            trade_id: pending.trade_id,
            draft,
            reference_price: quote.price,
            // 파는 주문은 VI · 거래정지를 모른다고 막지 않는다.
            status: SymbolStatus {
                vi: None,
                halted: None,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 간격은_두_배씩_늘고_10분을_넘지_않는다() {
        let mut interval = 0;
        let mut seen = Vec::new();
        for _ in 0..12 {
            interval = next_interval(interval, BACKOFF_MAX_SECONDS);
            seen.push(interval);
        }
        assert_eq!(&seen[..4], &[1, 2, 4, 8]);
        assert_eq!(seen.last().copied(), Some(BACKOFF_MAX_SECONDS));
    }

    #[test]
    fn 손절_청산의_간격은_5초를_넘지_않는다() {
        let mut interval = 0;
        for _ in 0..12 {
            interval = next_interval(interval, URGENT_BACKOFF_MAX_SECONDS);
            assert!(interval <= URGENT_BACKOFF_MAX_SECONDS);
        }
        assert_eq!(interval, URGENT_BACKOFF_MAX_SECONDS);
        assert!(urgent(SellPurpose::StopLoss));
        assert!(!urgent(SellPurpose::TakeProfit));
        assert!(!urgent(SellPurpose::TeamExit));
    }
}
