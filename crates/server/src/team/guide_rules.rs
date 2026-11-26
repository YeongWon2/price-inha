//! (백로그 T · 결정 30) 1층으로 가는 지침 후보를 지금의 개선안 길로 보낸다.
//!
//! 한도 · 금지 · 승인 길을 바꾸려는 되풀이는 지침이 되지 못한다(2층은 1층을 넘지 못한다). 그 가운데 어느 규칙 값을
//! 어떻게 바꾸자는 것인지 글에서 확실히 읽히는 것만(`pi_journal::rule_change_of`) 복기의 개선안과 같은 길로 낸다:
//! 제안됨 개선안 → 승인 요청(사람 승인) → [`super::KIND_PROPOSAL`] 이벤트 · 알림. 승인 · 냉각 · 상한은 지금 길
//! 그대로다. 확실히 읽히지 않으면 지금처럼 `to_rules` 이벤트만 남는다(이벤트의 `proposal`이 비어 있다).

use pi_approval::{Ask, Subject};
use pi_journal::{Candidate, Improvement};
use pi_store::{DecisionMemo, ProposalId, UnixMillis};
use serde_json::{Value, json};

use super::review::mistake_word;
use super::{Crew, KIND_PROPOSAL};

/// 1층 길로 보낸 후보 이벤트의 까닭.
const WHY_LAYER_ONE: &str = "layer_one";

/// 개선안 이벤트에 붙이는 출처 표시(복기의 개선안과 가른다).
const VIA_GUIDELINE: &str = "guideline";

/// 1층 길로 보낸 후보 하나의 이벤트 내용. 개선안이 생겼으면 그 번호.
pub(super) fn to_rules_payload(candidate: &Candidate, proposal: Option<ProposalId>) -> Value {
    json!({
        "action": "to_rules",
        "id": Value::Null,
        "kind": candidate.kind.word(),
        "why": WHY_LAYER_ONE,
        "into": Value::Null,
        "proposal": proposal.map(|id| id.0),
    })
}

impl Crew {
    /// 1층 후보에서 개선안을 내고 승인 요청을 만든다. 글이 확실히 읽히지 않거나 근거가 모자라거나 상한을 넘거나
    /// 쓰는 템플릿이 없으면 없음(경고만 남기고 하루 정리는 이어 간다).
    pub(super) async fn propose_layer_one(
        &self,
        candidate: &Candidate,
        memos: &[DecisionMemo],
    ) -> Option<ProposalId> {
        let current = match self.trading.params() {
            Ok(Some(current)) => current,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(%error, "규칙 값을 읽지 못해 1층 후보를 개선안으로 내지 않습니다");
                return None;
            }
        };
        let at = UnixMillis(self.clock.now().timestamp_millis());
        let proposal = match self
            .journal
            .propose_from_candidate(candidate, memos, &current, at)
        {
            Ok(Some(proposal)) => proposal,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(%error, "1층 후보로 개선안을 만들지 못했습니다");
                return None;
            }
        };
        let mistake = Improvement::from_proposal(&proposal)
            .map(|improvement| mistake_word(&improvement.mistake))
            .unwrap_or_default();
        let requested = match self.approvals.request(Ask {
            subject: Subject::RuleImprovement {
                proposal: proposal.id,
            },
            reason: format!("되풀이된 지침 후보({mistake})로 규칙을 바꾸자는 개선안"),
        }) {
            Ok(requested) => requested,
            Err(error) => {
                tracing::warn!(%error, "1층 후보 개선안의 승인 요청을 만들지 못했습니다");
                return Some(proposal.id);
            }
        };
        let payload = json!({
            "id": proposal.id.0,
            "number": requested.number.as_str(),
            "fingerprint": requested.fingerprint.as_str(),
            "mistake": mistake,
            "via": VIA_GUIDELINE,
        });
        if let Err(error) = self.trading.record(KIND_PROPOSAL, payload).await {
            tracing::warn!(%error, "1층 후보 개선안의 승인 요청 이벤트를 남기지 못했습니다");
        }
        self.trading.announce_proposal(&requested.number);
        Some(proposal.id)
    }
}
