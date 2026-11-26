//! 복기 · 규칙 개선안 잇기(T15, V19~V21 · E17 · E28 · E35).
//!
//! 승인은 승인 모듈이 소비에 성공한 뒤에만 반영한다(`pi_approval::Approvals::with_proposal_approval`).

use chrono::{DateTime, Utc};
use pi_agent::Role;
use pi_approval::{Answer, ApprovalNumber, Ask, Decision, Subject};
use pi_journal::{Improvement, Mistake};
use pi_store::{
    ApprovalStatus, Proposal, ProposalId, ProposalSource, ProposalState, StoreError, Trade,
    UnixMillis,
};
use pi_template::Change;
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};

use super::demo_ai::REVIEW_HEAD;
use super::{Crew, KIND_PROPOSAL, Reviewed};
use crate::trading::{
    Answered as CardAnswer, PROPOSAL_UNAVAILABLE, change_of, reject_word, rejected,
};
use crate::{TeamError, TradingError};

/// 이벤트를 한 번에 읽는 개수.
const READ_PAGE: usize = 1_000;

fn millis(at: DateTime<Utc>) -> UnixMillis {
    UnixMillis(at.timestamp_millis())
}

/// 개선안 승인 길의 실패: 승인 모듈의 거부이거나 journal의 실패.
enum ApproveFail {
    Approval(pi_approval::Error),
    Journal(pi_journal::Error),
}

impl From<pi_approval::Error> for ApproveFail {
    fn from(error: pi_approval::Error) -> Self {
        ApproveFail::Approval(error)
    }
}

/// 복기 담당의 답.
struct ReviewReply {
    mistakes: Vec<Mistake>,
    note: String,
    changes: Vec<Change>,
}

/// 실수 글자.
pub(super) fn mistake_word(mistake: &Mistake) -> String {
    match mistake {
        Mistake::ChasedEntry => "chased_entry".into(),
        Mistake::LateStop => "late_stop".into(),
        Mistake::OversizedPosition => "oversized_position".into(),
        Mistake::NoRationale => "no_rationale".into(),
        Mistake::PanicExit => "panic_exit".into(),
        Mistake::Other(text) => text.clone(),
    }
}

fn mistake_of(word: &str) -> Mistake {
    match word.trim() {
        "chased_entry" => Mistake::ChasedEntry,
        "late_stop" => Mistake::LateStop,
        "oversized_position" => Mistake::OversizedPosition,
        "no_rationale" => Mistake::NoRationale,
        "panic_exit" => Mistake::PanicExit,
        other => Mistake::Other(other.to_string()),
    }
}

fn changes_of(change: Option<&Map<String, Value>>) -> Vec<Change> {
    change
        .into_iter()
        .flatten()
        .filter_map(|(field, value)| {
            let value: Decimal = value.as_str()?.trim().parse().ok()?;
            change_of(field, value)
        })
        .collect()
}

fn parse_review(text: &str) -> Option<ReviewReply> {
    let object = pi_agent::find_object_with(text, "mistakes")?;
    let mistakes = object
        .get("mistakes")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter(|word| !word.trim().is_empty())
        .map(mistake_of)
        .collect();
    Some(ReviewReply {
        mistakes,
        note: object.get("note")?.as_str()?.to_string(),
        changes: changes_of(object.get("change").and_then(Value::as_object)),
    })
}

/// 답 하나의 결과.
pub(super) struct Answered {
    pub(super) result: &'static str,
    pub(super) reason: Option<&'static str>,
}

/// 개선안 답의 거절 이유: 쓰는 템플릿의 값이 잠겨 있다(승격대기 · 실전중, V21).
const PROPOSAL_TEMPLATE_LOCKED: &str = "template_locked";

fn answered(result: &'static str, reason: Option<&'static str>) -> Answered {
    Answered { result, reason }
}

impl Crew {
    /// 아직 복기하지 못한 팔린 매매를 복기한다.
    pub(super) async fn review_pending(&self) -> Result<Reviewed, TeamError> {
        let journal_error = |error: pi_journal::Error| TeamError::Journal(error.to_string());
        let mut reviewed = Reviewed::default();
        for trade in self.journal.pending_reviews().map_err(journal_error)? {
            let Some(reply) = self.ask_review(&trade).await else {
                continue;
            };
            let now = self.clock.now();
            let record = self
                .journal
                .review_trade(trade.id, reply.mistakes, &reply.note, millis(now))
                .map_err(journal_error)?;
            self.index_review(&trade, &record);
            if !reply.changes.is_empty() {
                for mistake in &record.mistakes {
                    if let Some(id) = self.propose(mistake, &reply.changes).await? {
                        reviewed.proposals.push(id);
                    }
                }
            }
            reviewed.reviews.push(record);
        }
        // (기능 L) 하루 정리의 지침 승격 · 성적 · 회고.
        self.learn_guidelines().await;
        Ok(reviewed)
    }

    /// 복기 담당 턴. 답하지 못했거나 모양이 틀리면 없음(다음 마감에 다시 본다, E17).
    async fn ask_review(&self, trade: &Trade) -> Option<ReviewReply> {
        let input = format!(
            "{REVIEW_HEAD} 매매 {} · 종목 {}\n{}\n답은 JSON 객체 하나다: {{\"mistakes\": [\"chased_entry\" | \
             \"late_stop\" | \"oversized_position\" | \"no_rationale\" | \"panic_exit\" | <자유 글>], \
             \"note\": \"<복기 글>\", \"change\": {{\"<규칙 칸>\": \"<십진수>\"}} (없으면 빼기)}}",
            trade.id.0,
            trade.symbol,
            pi_journal::plain_summary(trade)
        );
        let title = format!("review-{}", trade.id.0);
        let limits = pi_agent::TimeLimits::MARKET;
        let config = self.role(Role::Review);
        match self
            .team
            .runner()
            .run_turn(&config, limits, &title, &input)
            .await
        {
            Ok(outcome) => parse_review(&outcome.text),
            Err(error) => {
                tracing::warn!(%error, "복기 담당이 답하지 못했다. 다음 마감에 다시 본다");
                None
            }
        }
    }

    /// 개선안을 내고 승인 요청을 만든다. 생기지 않았으면 없음.
    async fn propose(
        &self,
        mistake: &Mistake,
        changes: &[Change],
    ) -> Result<Option<ProposalId>, TeamError> {
        let Some(current) = self.trading.params()? else {
            return Ok(None);
        };
        let now = millis(self.clock.now());
        let proposal = match self.journal.propose(mistake, &current, changes, now) {
            Ok(Some(proposal)) => proposal,
            Ok(None) => return Ok(None),
            Err(error) => {
                tracing::warn!(%error, "개선안을 만들지 못했다");
                return Ok(None);
            }
        };
        let requested = self
            .approvals
            .request(Ask {
                subject: Subject::RuleImprovement {
                    proposal: proposal.id,
                },
                reason: format!(
                    "반복된 실수({})로 규칙을 바꾸자는 개선안",
                    mistake_word(mistake)
                ),
            })
            .map_err(|error| TeamError::Approval(error.to_string()))?;
        let payload = json!({
            "id": proposal.id.0,
            "number": requested.number.as_str(),
            "fingerprint": requested.fingerprint.as_str(),
            "mistake": mistake_word(mistake),
        });
        self.trading.record(KIND_PROPOSAL, payload).await?;
        self.trading.announce_proposal(&requested.number);
        Ok(Some(proposal.id))
    }

    /// 개선안마다 남긴 승인 요청 이벤트(개선안 번호 → 내용).
    pub(super) fn proposal_notices(&self) -> Result<Vec<Value>, TeamError> {
        let mut found = Vec::new();
        let mut after = 0;
        loop {
            let events = self.store.read_after(after, READ_PAGE)?;
            let Some(last) = events.last() else {
                return Ok(found);
            };
            after = last.seq;
            found.extend(
                events
                    .into_iter()
                    .filter(|event| event.kind == KIND_PROPOSAL)
                    .map(|event| event.payload),
            );
        }
    }

    /// 개선안 목록(번호 순서).
    pub(super) fn proposal_list(&self) -> Result<Vec<Value>, TeamError> {
        let notices = self.proposal_notices()?;
        let mut all: Vec<Proposal> = Vec::new();
        for state in [
            ProposalState::Proposed,
            ProposalState::Cooling,
            ProposalState::Applied,
            ProposalState::Rejected,
        ] {
            all.extend(self.store.proposals_in_state(state)?);
        }
        all.sort_by_key(|proposal| proposal.id);
        Ok(all
            .iter()
            .map(|proposal| view(proposal, &notices, self.proposal_changes(proposal)))
            .collect())
    }

    /// 개선안에 답한다([`super::PROPOSAL_ANSWER_PATH`]).
    ///
    /// (21차 4번) 쓰는 템플릿 값을 읽을 수 없으면(없음 · 오류) 승인을 쓰지 않은 채
    /// [`TeamError::Unavailable`](503)로 답한다 — 승인 요청은 기다림 그대로다. 거절을 저장하지 못하면 실패(5xx)다.
    /// (21차 1번) 사용자 변경 개선안은 템플릿 잠금과 상관없이 승인하면 냉각중이 된다.
    pub(super) fn answer_proposal(&self, body: &Value) -> Result<Answered, TeamError> {
        let id = ProposalId(body["id"].as_i64().unwrap_or_default());
        let number = ApprovalNumber::new(body["number"].as_str().unwrap_or_default());
        let fingerprint = body["fingerprint"].as_str().unwrap_or_default();
        let decision = Decision::from_wire(body["answer"].as_str().unwrap_or_default());
        self.answer_improvement(id, &number, fingerprint, decision)
    }

    /// (28차 3번) 폰 · 사무실의 승인 카드 길로 개선안의 승인 번호에 답한다: [`Crew::answer_proposal`]와 같은
    /// 함수를 타고 결과를 그 길의 말로 바꾼다 — 반영 · 냉각중이면 승인됨, 그 밖은 거절과 같은 이유.
    /// 쓰는 템플릿 값을 읽을 수 없으면(503의 경우) 승인 번호를 건드리지 않은 채 거절(`unavailable`)이다.
    pub(crate) fn answer_by_card(
        &self,
        id: ProposalId,
        number: &ApprovalNumber,
        fingerprint: &str,
        decision: Decision,
    ) -> Result<CardAnswer, TradingError> {
        match self.answer_improvement(id, number, fingerprint, decision) {
            Ok(done) => Ok(match done.reason {
                Some(reason) => rejected(reason),
                None => CardAnswer {
                    approved: true,
                    reason: None,
                },
            }),
            Err(TeamError::Unavailable) => Ok(rejected(PROPOSAL_UNAVAILABLE)),
            Err(TeamError::Trading(error)) => Err(error),
            Err(error) => Err(TradingError::Exec(error.to_string())),
        }
    }

    /// 개선안 하나에 답한다(두 길이 함께 쓰는 한 함수).
    fn answer_improvement(
        &self,
        id: ProposalId,
        number: &ApprovalNumber,
        fingerprint: &str,
        decision: Decision,
    ) -> Result<Answered, TeamError> {
        let source = match self.store.proposal_source(id) {
            Ok(source) => source,
            // 없는 개선안은 복기 길로 보낸다(승인 모듈 · 복기가 지금처럼 거절한다).
            Err(StoreError::ProposalNotFound(..)) => ProposalSource::Review,
            Err(error) => return Err(error.into()),
        };
        if let ProposalSource::User { .. } = source {
            return self.answer_user_change(id, number, fingerprint, decision);
        }
        // 복기 개선안의 반영 판정 · 거절은 쓰는 템플릿 값으로 한다. 읽을 수 없으면 아무것도 쓰기 전에 멈춘다.
        let current = match self.trading.params() {
            Ok(Some(current)) => current,
            Ok(None) => return Err(TeamError::Unavailable),
            Err(error) => {
                tracing::warn!(%error, "쓰는 템플릿 값을 읽지 못해 개선안 답을 미룬다");
                return Err(TeamError::Unavailable);
            }
        };
        let approval_error = |error: pi_approval::Error| TeamError::Approval(error.to_string());
        if decision == Decision::Reject {
            self.approvals
                .answer(number, fingerprint, Decision::Reject)
                .map_err(approval_error)?;
            let now = millis(self.clock.now());
            return match self.journal.reject(id, &current, now) {
                Ok(_) => Ok(answered("rejected", Some("user_rejected"))),
                Err(pi_journal::Error::Store(
                    StoreError::TransitionDenied { .. } | StoreError::ProposalNotFound(..),
                )) => Ok(answered("rejected", Some("already_resolved"))),
                Err(error) => Err(TeamError::Journal(error.to_string())),
            };
        }
        // 쓰는 템플릿의 값이 잠겨 있으면(승격대기 · 실전중) 승인을 닫거나 소비하기 전에 거절한다(V21 · E28):
        // 승인 번호는 그대로 기다리고 개선안 · 템플릿도 그대로다. 잠김이 풀린 뒤 같은 번호로 다시 답할 수 있다.
        let waiting = self.approvals.get(number).map_err(approval_error)?;
        let waiting = waiting.is_some_and(|view| view.status == ApprovalStatus::Pending);
        if waiting && self.trading.template_locked()? {
            return Ok(answered("rejected", Some(PROPOSAL_TEMPLATE_LOCKED)));
        }
        let answer = self
            .approvals
            .answer(number, fingerprint, Decision::Approve)
            .map_err(approval_error)?;
        if let Answer::Rejected(reason) = answer {
            return Ok(answered("rejected", Some(reject_word(reason))));
        }
        // 승인 소비는 승인 모듈이 하고, 반영 판정(지금 규칙으로 위험을 다시 본다)은 journal이 한다.
        let outcome = self
            .approvals
            .with_proposal_approval(number, id, |using, at| {
                self.journal
                    .approve(id, Some(using), &current, at)
                    .map_err(ApproveFail::Journal)
            });
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(ApproveFail::Approval(pi_approval::Error::Denied(reason))) => {
                return Ok(answered("rejected", Some(reject_word(reason))));
            }
            Err(ApproveFail::Approval(other)) => return Err(approval_error(other)),
            Err(ApproveFail::Journal(error)) => return Err(TeamError::Journal(error.to_string())),
        };
        if outcome.proposal.state == ProposalState::Applied {
            self.trading.apply_params(outcome.rules)?;
            return Ok(answered("applied", None));
        }
        Ok(answered("cooling", None))
    }

    /// (21차 1번) 사용자 변경 개선안(실전중 템플릿의 위험을 키우는 값 바꾸기)에 답한다. 템플릿 잠금으로
    /// 거절하지 않는다: 승인하면 승인을 소비하고 냉각중(늘 위험을 키운다), 반영은 다음 거래일 시작이다.
    fn answer_user_change(
        &self,
        id: ProposalId,
        number: &ApprovalNumber,
        fingerprint: &str,
        decision: Decision,
    ) -> Result<Answered, TeamError> {
        let approval_error = |error: pi_approval::Error| TeamError::Approval(error.to_string());
        if decision == Decision::Reject {
            self.approvals
                .answer(number, fingerprint, Decision::Reject)
                .map_err(approval_error)?;
            let now = millis(self.clock.now());
            return match self.store.reject_proposal(id, now) {
                Ok(_) => Ok(answered("rejected", Some("user_rejected"))),
                Err(StoreError::TransitionDenied { .. }) => {
                    Ok(answered("rejected", Some("already_resolved")))
                }
                Err(error) => Err(error.into()),
            };
        }
        let answer = self
            .approvals
            .answer(number, fingerprint, Decision::Approve)
            .map_err(approval_error)?;
        if let Answer::Rejected(reason) = answer {
            return Ok(answered("rejected", Some(reject_word(reason))));
        }
        match self.approvals.approve_proposal(number, id) {
            Ok(proposal) if proposal.state == ProposalState::Applied => {
                Ok(answered("applied", None))
            }
            Ok(_) => Ok(answered("cooling", None)),
            Err(pi_approval::Error::Denied(reason)) => {
                Ok(answered("rejected", Some(reject_word(reason))))
            }
            Err(error) => Err(approval_error(error)),
        }
    }
}

fn state_word(state: ProposalState) -> &'static str {
    match state {
        ProposalState::Proposed => "proposed",
        ProposalState::Cooling => "cooling",
        ProposalState::Applied => "applied",
        ProposalState::Rejected => "rejected",
    }
}

fn view(proposal: &Proposal, notices: &[Value], changes: Vec<Value>) -> Value {
    let notice = notices
        .iter()
        .find(|notice| notice["id"] == json!(proposal.id.0));
    let field = |name: &str| notice.map_or(Value::Null, |n| n[name].clone());
    let mistake = Improvement::from_proposal(proposal)
        .map(|improvement| mistake_word(&improvement.mistake))
        .unwrap_or_default();
    json!({
        "id": proposal.id.0,
        "state": state_word(proposal.state),
        "mistake": mistake,
        "increases_risk": proposal.increases_risk,
        "evidence": proposal.evidence.iter().map(|trade| trade.0).collect::<Vec<_>>(),
        "number": field("number"),
        "fingerprint": field("fingerprint"),
        "changes": changes,
    })
}
