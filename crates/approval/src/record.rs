//! 저장소 승인 표의 한 줄과 이 크레이트의 승인 요청을 잇는다.
//!
//! store 9차의 승인 표에는 지문 · 요약(JSON) · 결과 · 시각만 있다. 무작위 승인 번호 · 대상 · 까닭 ·
//! 기다릴 시각은 요약(JSON)에 넣고, 합친 횟수 · 알림 결과 · 지문이 달라 무효가 된 번호처럼 나중에
//! 생기는 것은 이벤트 기록(`approval.*`)에 쌓아 다시 켜도 같게 읽는다.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use pi_store::{
    AccountKind, ApprovalId, ApprovalRequest, ApprovalStatus, ProposalId, Store, StoreError,
    TemplateId, UnixMillis,
};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::approvals::{ApprovalNumber, Notice};
use crate::subject::{
    Fingerprint, OrderSubject, OrderType, Side, SignalKind, Subject, account_code, order_type_code,
    side_code, signal_kind_code,
};

/// 만료 시각이 없는 대상(실전 승격 · 규칙 개선 · 다시 시작)의 저장소 만료 칸 값.
pub(crate) const NEVER: UnixMillis = UnixMillis(i64::MAX);

const EV_REPEAT: &str = "approval.repeat";
const EV_NOTICE: &str = "approval.notice";
const EV_VOIDED: &str = "approval.voided";
const EVENT_PAGE: usize = 500;

/// 저장소 한 줄을 풀어 낸 승인 요청.
#[derive(Debug, Clone)]
pub(crate) struct Record {
    pub(crate) id: ApprovalId,
    pub(crate) number: ApprovalNumber,
    pub(crate) subject: Subject,
    pub(crate) reason: String,
    pub(crate) status: ApprovalStatus,
    pub(crate) fingerprint: Fingerprint,
    pub(crate) consumed: bool,
    pub(crate) wait_until: Option<UnixMillis>,
    pub(crate) expires_at: Option<UnixMillis>,
}

impl Record {
    /// 이 시각에 만료인가(만료 시각이 없으면 아니다).
    pub(crate) fn expired_at(&self, now: UnixMillis) -> bool {
        self.expires_at.is_some_and(|end| now.0 >= end.0)
    }

    /// 이 시각에 기다릴 시간이 지났는가(지금 ≥ `wait_until`, 없으면 아니다).
    pub(crate) fn wait_passed(&self, now: UnixMillis) -> bool {
        self.wait_until.is_some_and(|t| now.0 >= t.0)
    }
}

/// 요약(JSON)에 넣을 내용.
pub(crate) fn summary(
    number: &ApprovalNumber,
    subject: &Subject,
    reason: &str,
    wait_until: Option<UnixMillis>,
    expires_at: Option<UnixMillis>,
) -> Value {
    json!({
        "number": number.as_str(),
        "subject": subject_json(subject),
        "reason": reason,
        "wait_until": wait_until.map(|t| t.0),
        "expires_at": expires_at.map(|t| t.0),
    })
}

/// 저장소 한 줄을 푼다. 이 크레이트가 만든 줄이 아니면 `None`.
pub(crate) fn decode(row: &ApprovalRequest) -> Option<Record> {
    let s = &row.summary;
    Some(Record {
        id: row.id,
        number: ApprovalNumber::new(s.get("number")?.as_str()?),
        subject: subject_from(s.get("subject")?)?,
        reason: s.get("reason")?.as_str()?.to_string(),
        status: row.status,
        fingerprint: Fingerprint::from_stored(row.fingerprint.clone()),
        consumed: row.consumed_at.is_some(),
        wait_until: s.get("wait_until")?.as_i64().map(UnixMillis),
        expires_at: s.get("expires_at")?.as_i64().map(UnixMillis),
    })
}

fn subject_json(subject: &Subject) -> Value {
    match subject {
        Subject::Order(o) => json!({
            "kind": "order",
            "account": account_code(o.account),
            "symbol": o.symbol,
            "side": side_code(o.side),
            "quantity": o.quantity,
            "price": o.price.normalize().to_string(),
            "order_type": order_type_code(o.order_type),
            "signal_id": o.signal_id,
            "signal_kind": signal_kind_code(o.signal_kind),
            "signal_valid_until": o.signal_valid_until.0,
        }),
        Subject::LivePromotion { template, revision } => {
            json!({"kind": "live_promotion", "template": template.0, "revision": revision})
        }
        Subject::RuleImprovement { proposal } => {
            json!({"kind": "rule_improvement", "proposal": proposal.0})
        }
        Subject::Restart => json!({"kind": "restart"}),
    }
}

fn subject_from(v: &Value) -> Option<Subject> {
    match v.get("kind")?.as_str()? {
        "order" => order_from(v).map(Subject::Order),
        "live_promotion" => Some(Subject::LivePromotion {
            template: TemplateId(v.get("template")?.as_i64()?),
            revision: u32::try_from(v.get("revision")?.as_u64()?).ok()?,
        }),
        "rule_improvement" => Some(Subject::RuleImprovement {
            proposal: ProposalId(v.get("proposal")?.as_i64()?),
        }),
        "restart" => Some(Subject::Restart),
        _ => None,
    }
}

fn order_from(v: &Value) -> Option<OrderSubject> {
    let text = |key: &str| v.get(key).and_then(Value::as_str);
    Some(OrderSubject {
        account: match text("account")? {
            "paper" => AccountKind::Paper,
            "live" => AccountKind::Live,
            _ => return None,
        },
        symbol: text("symbol")?.to_string(),
        side: match text("side")? {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            _ => return None,
        },
        quantity: v.get("quantity")?.as_u64()?,
        price: Decimal::from_str(text("price")?).ok()?,
        order_type: match text("order_type")? {
            "limit" => OrderType::Limit,
            "market" => OrderType::Market,
            _ => return None,
        },
        signal_id: text("signal_id")?.to_string(),
        signal_kind: match text("signal_kind")? {
            "stop_loss" => SignalKind::StopLoss,
            "take_profit" => SignalKind::TakeProfit,
            "buy" => SignalKind::Buy,
            _ => return None,
        },
        signal_valid_until: UnixMillis(v.get("signal_valid_until")?.as_i64()?),
    })
}

/// 이벤트 기록에서 읽어 둔 것. 이벤트 번호 `seen`까지 반영했다.
#[derive(Debug, Default)]
pub(crate) struct Memo {
    seen: u64,
    ids: HashMap<String, ApprovalId>,
    repeat: HashMap<String, u32>,
    notice: HashMap<String, Notice>,
    voided: HashSet<String>,
}

impl Memo {
    /// 아직 읽지 않은 이벤트를 읽어 반영한다.
    pub(crate) fn refresh(&mut self, store: &Store) -> Result<(), StoreError> {
        loop {
            let events = store.read_after(self.seen, EVENT_PAGE)?;
            let Some(last) = events.last() else {
                return Ok(());
            };
            self.seen = last.seq;
            for event in &events {
                self.apply(&event.kind, &event.payload);
            }
        }
    }

    fn apply(&mut self, kind: &str, payload: &Value) {
        let Some(number) = payload.get("number").and_then(Value::as_str) else {
            return;
        };
        let number = number.to_string();
        match kind {
            EV_REPEAT => {
                if let Some(n) = payload.get("repeat").and_then(Value::as_u64) {
                    self.repeat
                        .insert(number, u32::try_from(n).unwrap_or(u32::MAX));
                }
            }
            EV_NOTICE => {
                let notice = match payload.get("notice").and_then(Value::as_str) {
                    Some("sent") => Notice::Sent,
                    _ => Notice::Failed,
                };
                self.notice.insert(number, notice);
            }
            EV_VOIDED => {
                self.voided.insert(number);
            }
            _ => {}
        }
    }

    pub(crate) fn repeat(&self, number: &ApprovalNumber) -> u32 {
        self.repeat.get(number.as_str()).copied().unwrap_or(1)
    }

    pub(crate) fn notice(&self, number: &ApprovalNumber) -> Option<Notice> {
        self.notice.get(number.as_str()).copied()
    }

    pub(crate) fn is_voided(&self, number: &ApprovalNumber) -> bool {
        self.voided.contains(number.as_str())
    }

    /// 번호로 저장소 줄을 찾는다. 모르는 번호면 표를 다시 읽는다.
    pub(crate) fn find(
        &mut self,
        store: &Store,
        number: &ApprovalNumber,
    ) -> Result<Option<Record>, StoreError> {
        if !self.ids.contains_key(number.as_str()) {
            for record in all_records(store)? {
                self.ids
                    .insert(record.number.as_str().to_string(), record.id);
            }
        }
        let Some(id) = self.ids.get(number.as_str()).copied() else {
            return Ok(None);
        };
        Ok(decode(&store.get_approval(id)?))
    }

    /// 새로 만든 줄의 번호를 기억한다.
    pub(crate) fn remember(&mut self, number: &ApprovalNumber, id: ApprovalId) {
        self.ids.insert(number.as_str().to_string(), id);
    }
}

/// 기록 이벤트를 쌓는다.
pub(crate) fn log_repeat(
    store: &Store,
    number: &ApprovalNumber,
    repeat: u32,
    at: UnixMillis,
) -> Result<(), StoreError> {
    let payload = json!({"number": number.as_str(), "repeat": repeat});
    store.append_at(EV_REPEAT, &payload, at).map(|_| ())
}

pub(crate) fn log_notice(
    store: &Store,
    number: &ApprovalNumber,
    notice: Notice,
    at: UnixMillis,
) -> Result<(), StoreError> {
    let text = match notice {
        Notice::Sent => "sent",
        Notice::Failed => "failed",
    };
    let payload = json!({"number": number.as_str(), "notice": text});
    store.append_at(EV_NOTICE, &payload, at).map(|_| ())
}

pub(crate) fn log_voided(
    store: &Store,
    number: &ApprovalNumber,
    at: UnixMillis,
) -> Result<(), StoreError> {
    let payload = json!({"number": number.as_str()});
    store.append_at(EV_VOIDED, &payload, at).map(|_| ())
}

/// 이 크레이트가 만든 모든 승인 요청(결과별, 번호 순).
pub(crate) fn all_records(store: &Store) -> Result<Vec<Record>, StoreError> {
    let mut out = Vec::new();
    for status in ApprovalStatus::ALL {
        out.extend(store.approvals_in_status(status)?.iter().filter_map(decode));
    }
    Ok(out)
}

/// 기다림인 승인 요청들(번호 순).
pub(crate) fn pending_records(store: &Store) -> Result<Vec<Record>, StoreError> {
    Ok(store
        .approvals_in_status(ApprovalStatus::Pending)?
        .iter()
        .filter_map(decode)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_round_trips_subject() {
        let subject = Subject::Order(OrderSubject {
            account: AccountKind::Live,
            symbol: "A\"B".to_string(),
            side: Side::Sell,
            quantity: 3,
            price: Decimal::new(7_100_050, 2),
            order_type: OrderType::Limit,
            signal_id: "s".to_string(),
            signal_kind: SignalKind::TakeProfit,
            signal_valid_until: UnixMillis(9),
        });
        let number = ApprovalNumber::new("n");
        let row = ApprovalRequest {
            id: ApprovalId(1),
            fingerprint: "f".to_string(),
            summary: summary(&number, &subject, "왜", Some(UnixMillis(5)), None),
            status: ApprovalStatus::Pending,
            created_at: UnixMillis(0),
            expires_at: NEVER,
            resolved_at: None,
            consumed_at: None,
        };
        let back = decode(&row).unwrap();
        assert_eq!(back.subject, subject);
        assert_eq!(back.number, number);
        assert_eq!(back.wait_until, Some(UnixMillis(5)));
        assert_eq!(back.expires_at, None);
    }
}
