//! 승인 관리자의 안쪽 도우미: 대상 검사 · 합치기 · 새로 만들기 · 소비 거부 까닭.

use std::sync::MutexGuard;

use pi_store::{DenyReason, NewApprovalRequest, StoreError, TeamState, UnixMillis};

use crate::approvals::{ApprovalNumber, Approvals, Ask, Notice, Requested, lock};
use crate::error::{Error, RejectReason};
use crate::record::{self, Memo, NEVER, Record};
use crate::subject::{Fingerprint, OrderType, Side, SignalKind, Subject};

impl Approvals {
    /// 이벤트 기록을 따라잡은 기억.
    pub(crate) fn memo(&self) -> Result<MutexGuard<'_, Memo>, Error> {
        let mut memo = lock(&self.memo);
        memo.refresh(&self.store)?;
        Ok(memo)
    }

    pub(crate) fn notice_of(&self, number: &ApprovalNumber) -> Result<Option<Notice>, Error> {
        Ok(self.memo()?.notice(number))
    }

    /// 요청을 만들 수 있는 대상인지 본다.
    pub(crate) fn check_subject(&self, subject: &Subject, now: UnixMillis) -> Result<(), Error> {
        match subject {
            Subject::Order(order) => {
                if order.order_type != OrderType::Limit {
                    return Err(Error::LimitOrderRequired);
                }
                let paired = matches!(
                    (order.side, order.signal_kind),
                    (Side::Buy, SignalKind::Buy)
                        | (Side::Sell, SignalKind::StopLoss | SignalKind::TakeProfit)
                );
                if !paired {
                    return Err(Error::InvalidSubject(
                        "주문 방향과 신호 종류의 짝이 맞지 않는다".to_string(),
                    ));
                }
                if now.0 >= order.signal_valid_until.0 {
                    return Err(Error::SignalAlreadyExpired);
                }
                Ok(())
            }
            Subject::LivePromotion { .. } if self.store.team_state()? == TeamState::FullStop => {
                Err(Error::FullStop)
            }
            _ => Ok(()),
        }
    }

    /// 기다리는 중인 같은 요청에 합친다.
    pub(crate) fn merge(&self, found: Record, now: UnixMillis) -> Result<Requested, Error> {
        let mut memo = self.memo()?;
        let repeat = memo.repeat(&found.number).saturating_add(1);
        record::log_repeat(&self.store, &found.number, repeat, now)?;
        memo.refresh(&self.store)?;
        Ok(Requested {
            short_code: found.fingerprint.short_code().to_string(),
            number: found.number,
            fingerprint: found.fingerprint,
            wait_until: found.wait_until,
            expires_at: found.expires_at,
            repeat,
        })
    }

    /// 새 요청을 만든다.
    pub(crate) fn create(
        &self,
        ask: Ask,
        fp: Fingerprint,
        now: UnixMillis,
    ) -> Result<Requested, Error> {
        let mut memo = self.memo()?;
        let mut number = ApprovalNumber::random();
        while memo.find(&self.store, &number)?.is_some() {
            number = ApprovalNumber::random();
        }
        let (wait_until, expires_at) = match &ask.subject {
            Subject::Order(order) => {
                let wait = match order.signal_kind {
                    SignalKind::StopLoss => self.waits.stop_loss,
                    SignalKind::TakeProfit => self.waits.take_profit,
                    SignalKind::Buy => self.waits.buy,
                };
                let wait_ms = i64::try_from(wait.as_millis()).unwrap_or(i64::MAX);
                (
                    Some(UnixMillis(now.0.saturating_add(wait_ms))),
                    Some(order.signal_valid_until),
                )
            }
            _ => (None, None),
        };
        let created = self.store.create_approval(NewApprovalRequest {
            fingerprint: fp.as_str().to_string(),
            summary: record::summary(&number, &ask.subject, &ask.reason, wait_until, expires_at),
            created_at: now,
            expires_at: expires_at.unwrap_or(NEVER),
        })?;
        memo.remember(&number, created.id);
        Ok(Requested {
            short_code: fp.short_code().to_string(),
            number,
            fingerprint: fp,
            wait_until,
            expires_at,
            repeat: 1,
        })
    }

    /// 소비가 막힌 까닭. 지문이 달랐으면 그 번호를 영영 못 쓰게 기록한다(E7).
    pub(crate) fn consume_denied(
        &self,
        err: StoreError,
        number: &ApprovalNumber,
        now: UnixMillis,
    ) -> Error {
        let reason = match err {
            StoreError::ApprovalNotFound(_) => RejectReason::NotFound,
            StoreError::ApprovalNotUsable { .. } => RejectReason::NotApproved,
            StoreError::ApprovalAlreadyConsumed { .. } => RejectReason::AlreadyConsumed,
            StoreError::ApprovalExpired { .. } => RejectReason::Expired,
            // 승인한 뒤 템플릿 판이 바뀐 것은 승인한 지문과 달라진 것이다(무효).
            StoreError::ApprovalFingerprintMismatch { .. }
            | StoreError::TemplateRevisionMismatch { .. } => {
                if let Err(log_err) = record::log_voided(&self.store, number, now) {
                    return log_err.into();
                }
                RejectReason::FingerprintMismatch
            }
            // 승인을 본 뒤 승격 사이에 팀이 전체 정지가 됐다(승인은 그대로).
            StoreError::TransitionDenied {
                reason: DenyReason::TeamFullStop,
                ..
            } => RejectReason::FullStop,
            other => return other.into(),
        };
        Error::Denied(reason)
    }
}
