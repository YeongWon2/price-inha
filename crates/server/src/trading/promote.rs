//! (10차 10a 7번) 승격(결정 2026-11-21 6번, F6 5~8): 장 마감 뒤 모의 결과를 템플릿의 승격 기준에 잰다.
//!
//! - 기준을 재는 일은 `pi_template::PromotionCriteria::evaluate`가 한다. 여기서는 모의 결과를 모아 넘긴다:
//!   매매 건수 · 비용 뺀 손익은 그 템플릿을 만든 뒤 팔린 매매의 것, 거래일 수는 그 뒤 남긴 하루 마감의 날 수,
//!   최대 낙폭은 마감 때마다 잰 (최고 평가액 − 평가액) ÷ 최고 평가액의 가장 큰 값이다. 표본 밖 성과는 재는
//!   길이 없어 비워 둔다(기준에 있으면 미달이다).
//! - 넘으면 저장소에서 승격대기로 옮기고(`submit_template_for_promotion`) 실전 승격 승인 요청을 만든다.
//!   못 미치면 모의중 그대로다(E26). 잰 결과는 [`KIND_PROMOTION_CHECK`]로 남는다.
//! - 승인되면 `pi_approval::Approvals::promote_live`가 승인을 소비하며 실전중으로 올린다(E36). 거절이면
//!   모의중으로 돌린다. 실전 주문은 만들지 않는다: 실전중 템플릿의 사는 주문은 주문 실행기가 막는다(E25).
//! - (10a 보강 5번) 승격대기에 갇히지 않게 켤 때 · 하루 마감 때 · 다시 시작할 때 맞춰 본다
//!   ([`Trading::reconcile_promotions`]). 전체 정지 중의 승인 답은 승인을 닫기 전에 거절한다.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use pi_approval::{Answer, ApprovalNumber, ApprovalView, Ask, Decision, RejectReason, Subject};
use pi_store::{ApprovalStatus, StoreError, TeamState, TemplateId, TemplateState};
use pi_template::{PromotionStats, Verdict};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::answer::{Answered, rejected};
use super::base::{exec_text, millis};
use super::{KIND_DAY_CLOSE, Trading, reject_word};
use crate::TradingError;

/// 저장되는 이벤트 종류: 장 마감 뒤 승격 기준을 잰 결과(화면에는 보내지 않는다). 내용
/// `{"date", "template": 번호, "revision": 판, "stats": 모의 결과, "criteria": 기준, "passed": bool, "short": [모자란 기준]}`.
pub(crate) const KIND_PROMOTION_CHECK: &str = "promotion_check";

/// 이벤트를 한 번에 읽는 수.
const READ_PAGE: usize = 1_000;
/// 퍼센트로 바꾸는 값.
const HUNDRED: i64 = 100;

fn date_of_payload(payload: &Value) -> Option<NaiveDate> {
    let text = payload["date"].as_str()?;
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

impl Trading {
    /// 장 마감 뒤 승격 기준을 잰다. 실패는 기록만 하고 다음 마감에 다시 본다.
    pub(super) async fn consider_promotion(&self, date: NaiveDate) {
        if let Err(error) = self.try_promotion(date).await {
            tracing::warn!(%error, "승격 기준을 재지 못했습니다");
        }
    }

    async fn try_promotion(&self, date: NaiveDate) -> Result<(), TradingError> {
        let Some(template) = self.template_in_use()? else {
            return Ok(());
        };
        if template.state != TemplateState::Simulating {
            return Ok(());
        }
        let Ok(parsed) = pi_template::Template::from_json(&template.value.to_string()) else {
            return Ok(());
        };
        let criteria = parsed.params.promotion;
        let stats = self.promotion_stats(&template).await?;
        let verdict = criteria.evaluate(&stats);
        let short = match &verdict {
            Verdict::Passed => Vec::new(),
            Verdict::Short(short) => short.clone(),
        };
        let record = json!({
            "date": date.format("%Y-%m-%d").to_string(),
            "template": template.id.0,
            "revision": template.revision,
            "stats": serde_json::to_value(&stats).unwrap_or(Value::Null),
            "criteria": serde_json::to_value(&criteria).unwrap_or(Value::Null),
            "passed": short.is_empty(),
            "short": serde_json::to_value(&short).unwrap_or(Value::Null),
        });
        // (백로그 Q) 통계를 재는 동안 개선안 반영 등으로 쓰는 템플릿이 바뀌었으면 이 마감에는 올리지 않는다.
        let _turn = self.template_turn();
        if !super::promote_turn::still_same(&template, self.template_in_use()?.as_ref()) {
            tracing::info!(
                template = template.id.0,
                "승격 기준을 재는 사이 템플릿이 바뀌어 다음 마감에 다시 잽니다"
            );
            return Ok(());
        }
        let at = millis(self.clock.now());
        self.store.append_at(KIND_PROMOTION_CHECK, &record, at)?;
        if verdict != Verdict::Passed {
            return Ok(());
        }
        let pending = self
            .store
            .submit_template_for_promotion(template.id, &record, at)?;
        let subject = Subject::LivePromotion {
            template: pending.id,
            revision: pending.revision,
        };
        let reason = "모의 결과가 이 템플릿의 승격 기준을 모두 넘었습니다".to_string();
        let summary = format!("템플릿 {} 실전 승격 · {reason}", pending.id.0);
        match self.approvals.request(Ask { subject, reason }) {
            Ok(requested) => {
                self.announce_approval(&requested.number, &summary);
                Ok(())
            }
            Err(error) => {
                // 물어볼 수 없으면 승격대기로 두지 않는다(다음 마감에 다시 잰다).
                self.back_to_simulating(pending.id)?;
                Err(exec_text(error))
            }
        }
    }

    /// 그 템플릿을 만든 뒤의 모의 결과(승격 판정과 템플릿 화면의 승격 기준 줄이 같이 쓴다).
    pub(super) async fn promotion_stats(
        &self,
        template: &pi_store::Template,
    ) -> Result<PromotionStats, TradingError> {
        let since = template.created_at;
        let sold = self.sold_records().await?;
        let mine: Vec<_> = sold
            .iter()
            .filter(|record| record.closed_at.0 >= since.0)
            .collect();
        let net_pnl = mine.iter().fold(Decimal::ZERO, |sum, record| {
            sum.saturating_add(record.pnl.unwrap_or(Decimal::ZERO))
        });
        let from = chrono::DateTime::from_timestamp_millis(since.0).map(|at| self.date_of(at));
        let (days, worst_before) = self.promotion_history(template.id, from)?;
        let currency = self.market_rules(self.market).currency.clone();
        let now_drawdown = self
            .last_books()
            .iter()
            .find(|book| book.currency == currency)
            .and_then(|book| {
                let fallen = book.peak_equity.checked_sub(book.equity)?;
                fallen
                    .checked_mul(Decimal::from(HUNDRED))?
                    .checked_div(book.peak_equity)
            })
            .unwrap_or(Decimal::ZERO)
            .max(Decimal::ZERO);
        Ok(PromotionStats {
            trades: u32::try_from(mine.len()).unwrap_or(u32::MAX),
            trading_days: days,
            net_pnl,
            max_drawdown_pct: now_drawdown.max(worst_before),
            // 표본 밖 성과를 재는 길이 아직 없다(기준에 있으면 미달로 남는다).
            out_of_sample_pnl: None,
        })
    }

    /// 저장된 이벤트에서 읽는다: `from`부터의 하루 마감 날 수와, 그 템플릿으로 앞서 잰 최대 낙폭의 가장 큰 값.
    fn promotion_history(
        &self,
        template: TemplateId,
        from: Option<NaiveDate>,
    ) -> Result<(u32, Decimal), TradingError> {
        let mut days = BTreeSet::new();
        let mut worst = Decimal::ZERO;
        let mut after = 0;
        loop {
            let events = self.store.read_after(after, READ_PAGE)?;
            let Some(last) = events.last() else {
                break;
            };
            after = last.seq;
            for event in &events {
                if event.kind == KIND_DAY_CLOSE {
                    let date = date_of_payload(&event.payload);
                    if let Some(date) = date.filter(|date| from.is_none_or(|from| *date >= from)) {
                        days.insert(date);
                    }
                } else if event.kind == KIND_PROMOTION_CHECK
                    && event.payload["template"] == json!(template.0)
                {
                    let seen = event.payload["stats"]["max_drawdown_pct"]
                        .as_str()
                        .and_then(|text| text.parse::<Decimal>().ok());
                    worst = worst.max(seen.unwrap_or(Decimal::ZERO));
                }
            }
        }
        Ok((u32::try_from(days.len()).unwrap_or(u32::MAX), worst))
    }

    /// 승격대기 → 모의중(거절됐거나 물어볼 수 없다). 이미 승격대기가 아니면 그대로 둔다.
    fn back_to_simulating(&self, template: TemplateId) -> Result<(), TradingError> {
        let at = millis(self.clock.now());
        match self.store.reject_template_promotion(template, at) {
            Ok(_) | Err(StoreError::TransitionDenied { .. }) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// (10a 보강 5번) 승격대기 템플릿을 맞춰 본다: 켤 때 · 하루 마감 때 · 전체 정지에서 다시 시작할 때 부른다.
    /// 실패는 기록만 하고 다음 번에 다시 본다.
    ///
    /// - 승인됨인데 소비되지 않은 실전 승격 승인이 있으면 `promote_live`를 다시 부른다(승인 저장 직후 꺼졌거나
    ///   전체 정지 중에 승인된 것). 전체 정지 중이면 기다린다.
    /// - 쓸 수 있는 승인 요청(기다림, 또는 승인됨 · 미소비)이 하나도 없으면 모의중으로 되돌린다(기준 통과 기록도
    ///   지워진다. 다음 마감에 다시 잰다).
    pub(super) fn reconcile_promotions(&self) {
        if let Err(error) = self.try_reconcile() {
            tracing::warn!(%error, "승격대기 템플릿을 맞춰 보지 못했습니다");
        }
    }

    fn try_reconcile(&self) -> Result<(), TradingError> {
        // (백로그 Q) 실전 승격의 답과 겹치지 않게 같은 차례로 돈다.
        let _turn = self.template_turn();
        let pending = self
            .store
            .templates_in_state(TemplateState::PromotionPending)?;
        if pending.is_empty() {
            return Ok(());
        }
        let stopped = self.store.team_state()? == TeamState::FullStop;
        let requests = self.promotion_requests()?;
        for template in pending {
            let mine = requests.iter().filter(|view| {
                matches!(view.subject, Subject::LivePromotion { template: id, .. } if id == template.id)
            });
            let mut usable = false;
            for view in mine {
                if view.status == ApprovalStatus::Pending || stopped {
                    usable = true;
                    continue;
                }
                match self.approvals.promote_live(&view.number, template.id) {
                    Ok(_) => {
                        tracing::info!(
                            template = template.id.0,
                            "받아 둔 승인으로 실전중으로 올렸습니다"
                        );
                        usable = true;
                        break;
                    }
                    // 그 사이 전체 정지가 됐다: 다시 시작한 뒤에 다시 본다.
                    Err(pi_approval::Error::Denied(RejectReason::FullStop)) => usable = true,
                    // 쓸 수 없는 승인이다(내용이 바뀜 · 이미 씀 등).
                    Err(pi_approval::Error::Denied(_)) => {}
                    Err(error) => return Err(exec_text(error)),
                }
            }
            if !usable {
                tracing::info!(
                    template = template.id.0,
                    "쓸 수 있는 승인 요청이 없어 승격대기를 모의중으로 되돌립니다"
                );
                self.back_to_simulating(template.id)?;
            }
        }
        Ok(())
    }

    /// 쓸 수 있는 실전 승격 승인 요청들: 기다림, 또는 승인됨인데 소비되지 않은 것.
    pub(super) fn promotion_requests(&self) -> Result<Vec<ApprovalView>, TradingError> {
        let mut found = Vec::new();
        for status in [ApprovalStatus::Pending, ApprovalStatus::Approved] {
            for row in self.store.approvals_in_status(status)? {
                let Some(number) = row.summary["number"].as_str() else {
                    continue;
                };
                let view = self.approvals.get(&ApprovalNumber::new(number));
                let Some(view) = view.map_err(exec_text)? else {
                    continue;
                };
                // 무효가 된 승인은 거절로 보인다(`ApprovalView::status`): 여기서 걸러진다.
                let open = view.status == ApprovalStatus::Pending
                    || (view.status == ApprovalStatus::Approved && !view.consumed);
                if open && matches!(view.subject, Subject::LivePromotion { .. }) {
                    found.push(view);
                }
            }
        }
        Ok(found)
    }

    /// 그 승인 번호가 실전 승격을 묻는 것이면 그 템플릿.
    pub(super) fn promotion_of(
        &self,
        number: &ApprovalNumber,
    ) -> Result<Option<TemplateId>, TradingError> {
        let view = self.approvals.get(number).map_err(exec_text)?;
        Ok(match view.map(|view| view.subject) {
            Some(Subject::LivePromotion { template, .. }) => Some(template),
            _ => None,
        })
    }

    /// 실전 승격의 답(V30 · V42 · E36): 승인이면 승인을 소비하며 실전중으로, 거절이면 모의중으로.
    /// 전체 정지로 거부된 것은 템플릿을 그대로 둔다(다시 시작한 뒤에 답한다).
    pub(super) fn answer_promotion(
        &self,
        number: &ApprovalNumber,
        template: TemplateId,
        shown: &str,
        decision: Decision,
    ) -> Result<Answered, TradingError> {
        // (백로그 Q) 맞춰 보기 · 승격 판정과 같은 차례로 돈다(겹쳐 실전중이 됐는데 답이 500이 되지 않게).
        let _turn = self.template_turn();
        // 전체 정지 중의 승인은 승인을 닫기 전에 거절한다: 승인 번호는 그대로 기다리고, 다시 시작한 뒤 같은
        // 번호로 답할 수 있다. 이 확인과 답 사이에 멈춘 것은 맞춰 보기([`Trading::reconcile_promotions`])가 잇는다.
        if decision == Decision::Approve && self.store.team_state()? == TeamState::FullStop {
            return Ok(rejected(reject_word(RejectReason::FullStop)));
        }
        let answered = self.approvals.answer(number, shown, decision);
        match answered.map_err(exec_text)? {
            Answer::Approved => match self.approvals.promote_live(number, template) {
                Ok(_) => Ok(Answered {
                    approved: true,
                    reason: None,
                }),
                Err(pi_approval::Error::Denied(reason)) => Ok(rejected(reject_word(reason))),
                Err(error) => Err(exec_text(error)),
            },
            Answer::Rejected(
                reason @ (RejectReason::NotFound
                | RejectReason::AlreadyResolved
                | RejectReason::FullStop),
            ) => Ok(rejected(reject_word(reason))),
            Answer::Rejected(reason) => {
                self.back_to_simulating(template)?;
                Ok(rejected(reject_word(reason)))
            }
        }
    }
}
