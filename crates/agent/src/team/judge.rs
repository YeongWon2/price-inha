//! 팀장 판단 턴과 그 뒤(판단 저장 · 주문 안). 모듈 문서 6 · 7번.

use pi_store::{Judgment, RoleOpinion, RoleScore, SourceRef, TradeSide, TradeSummary};
use serde_json::json;

use super::{Session, Spoken, Team, watched};
use crate::discussion::{DataItem, Decision, DiscussionOutcome, WatchReason};
use crate::error::{Error, TurnFailure};
use crate::opinion::parse_summary;
use crate::ports::{OrderDraft, Side};
use crate::progress::{NOTICE_SOURCE_NAME, NoticeNote, ProgressSignal, WRAP_UP_LINE};
use crate::prompt;
use crate::role::Role;
use crate::verdict::{Verdict, is_complete, parse_verdict};

impl Team {
    /// 팀장 판단 턴과 그 뒤(판단 저장 · 주문 안).
    pub(super) async fn judge(
        &self,
        session: &Session<'_>,
        said: &[(Role, String)],
        scores: Vec<RoleScore>,
        opinions: Vec<RoleOpinion>,
        total: Option<i32>,
    ) -> Result<DiscussionOutcome, Error> {
        let mut input = prompt::leader_turn(&session.briefing, said, &scores, total);
        if session.run.wrapping() {
            input.push('\n');
            input.push_str(WRAP_UP_LINE);
        }
        session.run.signal(ProgressSignal::LeaderTurn);
        session.run.leader_call();
        let (verdict, summary) = match self.speak(session, Role::Leader, &input).await? {
            Spoken::Text(text) => match parse_verdict(&text) {
                Some(verdict) => {
                    let summary = parse_summary(&text, self.ports.redactor.as_ref());
                    let payload = json!({"text": text, "decision": verdict.code()});
                    session.room.member(Role::Leader, payload)?;
                    (verdict, summary)
                }
                None => {
                    let reason = TurnFailure::InvalidStructuredReply.to_string();
                    return failed_scored(session, Role::Leader, &reason, scores, total);
                }
            },
            Spoken::Failed(cause) => {
                let reason = cause.to_string();
                return failed_scored(session, Role::Leader, &reason, scores, total);
            }
        };
        let (side, rationale) = match verdict {
            Verdict::Watch { reason, revisit } => {
                let reason = WatchReason::LeaderWatch { reason, revisit };
                return watched(session, reason, scores, total);
            }
            Verdict::Trade { side, rationale } => (side, rationale),
        };
        if let Some(reason) = partial_consensus(session, total) {
            return watched(session, reason, scores, total);
        }
        if !is_complete(&rationale) {
            return watched(session, WatchReason::RationaleIncomplete, scores, total);
        }
        let judgment = Judgment {
            decided_at: self.ports.clock.now(),
            rationale,
            sources: sources(session, self.ports.redactor.as_ref()),
            scores: scores.clone(),
            consensus_score: total.unwrap_or(0),
        };
        self.save(session, judgment, side, &opinions, summary)?;
        let (decision, order, sells) = match side {
            Side::Sell => (
                Decision::Sell,
                None,
                self.sells_after_judgment(session).await?,
            ),
            Side::Buy => (
                Decision::Buy,
                self.order_after_judgment(session).await?,
                Vec::new(),
            ),
        };
        Ok(DiscussionOutcome {
            decision,
            scores,
            consensus: total,
            order,
            sells,
        })
    }

    /// 판단을 방향과 함께 남긴다. 팀장 요약이 있으면 담당 의견들과 함께 한 쓰기로 남긴다(11차, M9).
    fn save(
        &self,
        session: &Session<'_>,
        judgment: Judgment,
        side: Side,
        opinions: &[RoleOpinion],
        summary: Option<TradeSummary>,
    ) -> Result<(), Error> {
        let side = match side {
            Side::Buy => TradeSide::Buy,
            Side::Sell => TradeSide::Sell,
        };
        let trade = session.request.trade;
        let store = &self.ports.store;
        match summary {
            Some(summary) => {
                store.judge_trade_with_opinions(trade, judgment, side, opinions, &summary)?;
            }
            None => {
                store.judge_trade_with_side(trade, judgment, side)?;
            }
        }
        Ok(())
    }

    /// 사기로 판단된 뒤 매매 담당의 주문 안. 매매 담당이 답하지 못하면 주문 안 없이 둔다.
    async fn order_after_judgment(
        &self,
        session: &Session<'_>,
    ) -> Result<Option<OrderDraft>, Error> {
        match self.draft_order(session.request.trade).await {
            Ok(draft) => Ok(Some(draft)),
            Err(Error::RoleUnavailable { role, cause }) => {
                session.room.failed(role, &cause.to_string())?;
                Ok(None)
            }
            Err(other) => Err(other),
        }
    }
}

/// (10차 검토) 호출이 모자라 점수를 내지 못한 담당이 있으면 합의가 온전하지 않다. 팀장이 사기 · 팔기로
/// 답해도 판단을 저장하지 않고 지켜보기로 끝낼 까닭을 준다(E27 — 적은 표나 표 없이 매매가 나가지 않게).
fn partial_consensus(session: &Session<'_>, total: Option<i32>) -> Option<WatchReason> {
    session
        .run
        .abstained()
        .then(|| WatchReason::ConsensusBelow {
            consensus: total.unwrap_or(0),
            required: session.request.consensus_required,
        })
}

/// 판단의 자료: 요청의 자료 이름(가린 것)과 알게 된 시각. 사용자 공지를 넘겼으면
/// [`NOTICE_SOURCE_NAME`] 한 줄을 가장 늦은 보낸 시각으로 더한다(11차, R17-2).
fn sources(session: &Session<'_>, redactor: &dyn pi_ai::Redactor) -> Vec<SourceRef> {
    let data: &[DataItem] = &session.request.data;
    let mut out: Vec<SourceRef> = data
        .iter()
        .map(|item| SourceRef {
            name: redactor.redact(&item.name),
            known_at: item.known_at,
        })
        .collect();
    let notices: &[NoticeNote] = session.run.notices();
    if let Some(latest) = notices.iter().map(|note| note.sent_at).max() {
        out.push(SourceRef {
            name: NOTICE_SOURCE_NAME.to_string(),
            known_at: latest,
        });
    }
    out
}

/// 점수 뒤(팀장 턴) 응답 실패로 지켜보기.
fn failed_scored(
    session: &Session<'_>,
    role: Role,
    reason: &str,
    scores: Vec<RoleScore>,
    total: Option<i32>,
) -> Result<DiscussionOutcome, Error> {
    session.room.failed(role, reason)?;
    watched(session, WatchReason::RoleUnavailable(role), scores, total)
}
