//! 답 없음 · 만료 뒤 처리 기록(E22 "보이는 것", 4차).

use pi_store::{ApprovalStatus, UnixMillis};

use crate::approvals::{ApprovalNumber, Approvals, DueAction, DueOutcome, Notice};
use crate::error::Error;
use crate::record::{self, Record};
use crate::subject::{OrderSubject, Side, Subject};

/// 막힌 까닭 글([`DueHandling::Blocked`])을 처리 기록에 남기는 상한(글자 수). 넘으면 앞에서부터 이만큼만 남긴다.
pub const BLOCKED_REASON_MAX_CHARS: usize = 200;

/// 답 없음 · 만료로 닫힌 주문 요청을 호출하는 쪽이 어떻게 처리했는지(E22).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DueHandling {
    /// 금액 한도 안으로 줄여 냈다(FN2 3b3). 줄인 수량(0보다 크고 요청 수량 이하). 셈은 rules가 했다.
    Reduced {
        /// 줄인 수량.
        quantity: u64,
    },
    /// 사지 않았다(P20, 결정 5번).
    NotBought,
    /// 팀이 팔기로 판단했다(FN2 3b2).
    TeamSold,
    /// 팀이 들고 있기로 판단했다(FN2 3b2).
    TeamKept,
    /// 다시 점검에서 막혔다. 막힌 까닭 글(가리는 일은 부르는 쪽, E38).
    Blocked {
        /// 막힌 까닭(가린 것).
        reason: String,
    },
    /// 신호가 지나 아무것도 하지 않았다.
    Dropped,
}

impl Approvals {
    /// 답 없음 · 만료로 닫힌 주문 요청의 처리 기록을 한 번만 남긴다(E22).
    ///
    /// - 주문 대상이고 결과가 답 없음 또는 만료일 때만 받는다. 아니면(번호가 없어도) [`Error::NotDue`].
    /// - 두 번째 기록은 [`Error::AlreadyHandled`]이고 처음 기록이 그대로다.
    /// - 할 일([`expire_due`](Approvals::expire_due)와 같은 규칙으로 다시 셈)과 맞지 않으면 [`Error::NotDue`]:
    ///   `ReduceWithinLimit` ← `Reduced` · `Blocked` · `Dropped`, `DoNotBuy` ← `NotBought`,
    ///   `TeamDecides` ← `TeamSold` · `TeamKept` · `Blocked` · `Dropped`, `Dropped` ← `Dropped`.
    ///   신호 유효 시간이 지나 [`unhandled_due`](Approvals::unhandled_due)가 `Dropped`로 준 요청은
    ///   `Dropped`도 받는다(그 할 일을 마칠 수 있게).
    /// - `Reduced { quantity }`는 0보다 크고 요청 수량 이하여야 한다(아니면 [`Error::NotDue`]).
    /// - 저장소 이벤트로 남아 다시 켠 뒤에도 읽힌다.
    pub fn record_due_handling(
        &self,
        number: &ApprovalNumber,
        handling: DueHandling,
    ) -> Result<(), Error> {
        let now = self.clock.now();
        let handling = capped(handling);
        // 기억을 잡은 채로 보고 남겨, 같은 관리자에서 두 번 남지 않게 한다.
        let mut memo = self.memo()?;
        let Some(found) = memo.find(&self.store, number)? else {
            return Err(Error::NotDue);
        };
        let Some(order) = closed_order(&found) else {
            return Err(Error::NotDue);
        };
        if memo.handling(number).is_some() {
            return Err(Error::AlreadyHandled);
        }
        let action = due_action(found.status, order, memo.notice(number));
        let late_drop = found.expired_at(now) && handling == DueHandling::Dropped;
        if !(fits(action, &handling, order.quantity) || late_drop) {
            return Err(Error::NotDue);
        }
        record::log_handling(&self.store, number, &handling, now)?;
        memo.refresh(&self.store)?;
        Ok(())
    }

    /// 남긴 처리 기록. 없으면 `None`.
    pub fn due_handling(&self, number: &ApprovalNumber) -> Result<Option<DueHandling>, Error> {
        Ok(self.memo()?.handling(number))
    }

    /// 답 없음 · 만료로 닫힌 주문 요청 가운데 처리 기록이 없는 것을 닫힌 순서로 돌려준다.
    ///
    /// 할 일은 [`expire_due`](Approvals::expire_due)와 같은 규칙이되, 지금 시각(시계)이 신호 유효 시간의
    /// 끝 이후면 [`DueAction::Dropped`](crate::DueAction::Dropped)다. 읽기만 한다.
    ///
    /// (5차) 처리 기록이 남은 옛 요청은 다시 읽지 않는다: 관리자마다 "이 번호 아래는 모두 닫히고 처리됐다"는
    /// 바닥을 기억해 두고 그 위의 닫힌 요청만 읽는다([`crate::due_scan`]).
    pub fn unhandled_due(&self) -> Result<Vec<DueOutcome>, Error> {
        let now = self.clock.now();
        let mut memo = self.memo()?;
        let mut closed = crate::due_scan::scan(&self.store, &mut memo)?;
        closed.sort_by_key(|r| (r.resolved_at.unwrap_or(UnixMillis(i64::MIN)).0, r.id.0));
        Ok(closed
            .into_iter()
            .map(|found| {
                let action = if found.expired_at(now) {
                    DueAction::Dropped
                } else {
                    due_action(found.status, &found.order, memo.notice(&found.number))
                };
                DueOutcome {
                    number: found.number,
                    subject: Subject::Order(found.order),
                    status: found.status,
                    action,
                }
            })
            .collect())
    }
}

/// 막힌 까닭 글을 [`BLOCKED_REASON_MAX_CHARS`] 글자까지만 남긴다(글자 단위라 한글이 깨지지 않는다).
fn capped(handling: DueHandling) -> DueHandling {
    match handling {
        DueHandling::Blocked { reason } if reason.chars().count() > BLOCKED_REASON_MAX_CHARS => {
            DueHandling::Blocked {
                reason: reason.chars().take(BLOCKED_REASON_MAX_CHARS).collect(),
            }
        }
        other => other,
    }
}

/// 답 없음 · 만료로 닫힌 주문 요청이면 그 주문.
fn closed_order(found: &Record) -> Option<&OrderSubject> {
    let Subject::Order(order) = &found.subject else {
        return None;
    };
    matches!(
        found.status,
        ApprovalStatus::NoAnswer | ApprovalStatus::Expired
    )
    .then_some(order)
}

/// 닫힌 주문 요청의 할 일([`Approvals::expire_due`]와 같은 규칙).
pub(crate) fn due_action(
    status: ApprovalStatus,
    order: &OrderSubject,
    notice: Option<Notice>,
) -> DueAction {
    if status == ApprovalStatus::Expired {
        DueAction::Dropped
    } else if order.side == Side::Sell {
        DueAction::TeamDecides
    } else if notice == Some(Notice::Sent) {
        DueAction::ReduceWithinLimit
    } else {
        DueAction::DoNotBuy
    }
}

/// 처리 기록이 할 일 · 요청 수량과 맞는가.
fn fits(action: DueAction, handling: &DueHandling, requested: u64) -> bool {
    match (action, handling) {
        (DueAction::ReduceWithinLimit, DueHandling::Reduced { quantity }) => {
            *quantity > 0 && *quantity <= requested
        }
        (DueAction::DoNotBuy, DueHandling::NotBought)
        | (
            DueAction::ReduceWithinLimit | DueAction::TeamDecides,
            DueHandling::Blocked { .. } | DueHandling::Dropped,
        )
        | (DueAction::TeamDecides, DueHandling::TeamSold | DueHandling::TeamKept)
        | (DueAction::Dropped, DueHandling::Dropped) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subject::{OrderType, SignalKind};
    use pi_store::AccountKind;
    use rust_decimal::Decimal;

    fn order(side: Side) -> OrderSubject {
        OrderSubject {
            account: AccountKind::Paper,
            symbol: "005930".to_string(),
            side,
            quantity: 5,
            price: Decimal::new(100, 0),
            order_type: OrderType::Limit,
            signal_id: "s".to_string(),
            signal_kind: if side == Side::Buy {
                SignalKind::Buy
            } else {
                SignalKind::StopLoss
            },
            signal_valid_until: UnixMillis(10),
        }
    }

    #[test]
    fn action_follows_expire_due_rule() {
        let buy = order(Side::Buy);
        let sell = order(Side::Sell);
        let sent = Some(Notice::Sent);
        assert_eq!(
            due_action(ApprovalStatus::Expired, &sell, sent),
            DueAction::Dropped
        );
        assert_eq!(
            due_action(ApprovalStatus::NoAnswer, &sell, None),
            DueAction::TeamDecides
        );
        assert_eq!(
            due_action(ApprovalStatus::NoAnswer, &buy, sent),
            DueAction::ReduceWithinLimit
        );
        assert_eq!(
            due_action(ApprovalStatus::NoAnswer, &buy, Some(Notice::Failed)),
            DueAction::DoNotBuy
        );
        assert_eq!(
            due_action(ApprovalStatus::NoAnswer, &buy, None),
            DueAction::DoNotBuy
        );
    }

    #[test]
    fn reduced_quantity_bounds() {
        let a = DueAction::ReduceWithinLimit;
        assert!(!fits(a, &DueHandling::Reduced { quantity: 0 }, 5));
        assert!(fits(a, &DueHandling::Reduced { quantity: 1 }, 5));
        assert!(fits(a, &DueHandling::Reduced { quantity: 5 }, 5));
        assert!(!fits(a, &DueHandling::Reduced { quantity: 6 }, 5));
        assert!(!fits(
            DueAction::TeamDecides,
            &DueHandling::Reduced { quantity: 1 },
            5
        ));
    }
}
