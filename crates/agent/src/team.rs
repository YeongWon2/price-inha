//! 투자 팀: 팀 논의 방 → 점수 턴 → 합의 점수 → 팀장 판단 턴 → 주문 안(만드는 것 3 · 4 · 5).
//!
//! # 순서 ([`Team::discuss`])
//!
//! 1. 요청 확인: 참가 담당이 2명 이상 상한 이하이고 겹치지 않으며, 지명된 담당이 하나 이상이고
//!    모두 참가 담당이어야 한다. 아니면 [`Error::InvalidMembers`]이고 아무것도 쓰지 않는다.
//! 2. 자료 확인: [`DataItem::content`]가 하나라도 `None`이면 지켜보기
//!    ([`WatchReason::MissingData`], E32).
//! 3. 논의(원본 Hermes 계획기와 같은 규칙, 결정 14번):
//!    - 0라운드는 지명된 담당, 1라운드부터는 앞선 글에서 다른 담당이 `@<id>`로 부른 뒤
//!      아직 글을 내지 않은 담당만 답한다. 자기 멘션 · 명단 밖 멘션은 무시한다.
//!    - 라운드마다 응답자를 명단 순서로 놓고 라운드 번호만큼 왼쪽으로 돌린다.
//!    - 답이 비었거나 `(pass)`(대소문자 무시)면 넘김이다: 글을 쓰지 않는다.
//!      다만 0라운드에 지명된 담당의 넘김은 응답 실패다.
//!    - 글은 방 로그 `message.member`(쓴 쪽 `Member`, id는 담당 id)로 남는다. 내용은
//!      `{"text": <글>, "round": <라운드>, "handoff": [<부른 담당 id>, …]}`다.
//!      `handoff`는 그 글이 부른 다른 참가 담당(명단 순서, 겹침 없음)이다 — 넘기기 기록(V2).
//!    - 담당 턴이 실패하면 방 로그 `turn.failed`(쓴 쪽 `Gateway`, 내용에 `"member": <담당 id>`)를 남긴다.
//!    - 응답자가 없는 라운드(조용한 라운드)면 끝난다. 라운드 상한 · 글 수 상한에 닿으면
//!      지켜보기([`WatchReason::DiscussionBounded`]).
//! 4. 점수 턴(결정 15번): 팀장을 뺀 참가 담당마다 점수 JSON([`crate::parse_score`])을 받는다.
//!    점수 턴은 방 로그에 쓰지 않고 담당 세션에만 남는다. 담당 하나라도 실패하면 합의 불가로 지켜보기.
//! 5. 합의 점수([`crate::consensus`])가 기준을 넘지 못하면 지켜보기([`WatchReason::ConsensusBelow`], E27).
//! 6. 팀장 판단 턴. 답은 `decision` 칸이 있는 객체 하나다:
//!    - 사기 · 팔기: `{"decision": "buy"|"sell", "hypothesis": 글, "trigger": 글, "entry": 정수,
//!      "stop": 정수, "target": 정수, "expected_value": 정수}`.
//!      사기인데 근거 칸이 하나라도 비었으면 지켜보기([`WatchReason::RationaleIncomplete`], P9).
//!      아니면 `judge_trade`로 근거 · 자료(요청의 자료 이름과 알게 된 시각) · 담당 점수 · 합의 점수를
//!      남긴다(판단 시각은 시계). 매매는 판단됨이 된다(V3, M2).
//!    - 지켜보기: `{"decision": "watch", "reason": 글, "revisit": 글}` → [`WatchReason::LeaderWatch`].
//!    - 모양이 틀리면 팀장 응답 실패로 지켜보기.
//!    - 판단은 방 로그 `message.member`(쓴 쪽 팀장)로 남고 내용에 `"decision": "buy"|"sell"|"watch"`가 있다.
//! 7. 사기면 [`Team::draft_order`]와 같은 길로 매매 담당이 주문 안을 만든다(V4).
//!
//! 지켜보기로 끝나면 매매는 논의중 그대로이고(판단을 저장하지 않는다) 주문 안을 만들지 않는다.
//! 그때 방 로그에 `room.activity`(쓴 쪽 `Gateway`, 내용 `{"status": "watch", "reason_code": <코드>}`)를
//! 하나 남긴다. 실행기는 승인을 붙이거나 매매를 승인대기 · 주문됨으로 옮기지 않는다(P21).

use pi_store::TradeId;

#[cfg(doc)]
use crate::discussion::{DataItem, WatchReason};
use crate::discussion::{DiscussionOutcome, DiscussionRequest};
use crate::error::Error;
use crate::ports::OrderDraft;
use crate::role::{RoleConfig, TimeLimits};
use crate::turn::{Ports, TurnRunner};

/// 투자 팀. 담당 6명의 설정과 실행기를 가진다.
#[derive(Clone)]
pub struct Team {
    runner: TurnRunner,
    ports: Ports,
    roles: Vec<RoleConfig>,
    limits: TimeLimits,
}

impl Team {
    /// 만든다. `roles`는 6명을 한 번씩 담아야 한다(순서는 상관없다). 아니면 [`Error::InvalidRoles`].
    /// `limits`는 이 팀의 모든 턴에 쓴다.
    pub fn new(ports: Ports, roles: Vec<RoleConfig>, limits: TimeLimits) -> Result<Team, Error> {
        let _ = (ports, roles, limits);
        todo!()
    }

    /// 이 팀의 턴 실행기.
    pub fn runner(&self) -> &TurnRunner {
        let _ = (&self.ports, &self.roles, &self.limits);
        &self.runner
    }

    /// 논의 하나를 끝까지 돈다(모듈 문서의 순서).
    pub async fn discuss(&self, request: DiscussionRequest) -> Result<DiscussionOutcome, Error> {
        let _ = request;
        todo!()
    }

    /// 매매 담당이 주문 안을 만들어 도구 포트로 넘긴다(V4).
    ///
    /// - 매매가 판단됨이 아니면 [`Error::TradeNotJudged`]이고 AI를 부르지 않고 아무것도 넘기지 않는다(E20).
    /// - 매매 담당의 답은 `side` 칸이 있는 객체 하나다: `{"side": "buy", "note": 글}`.
    ///   다른 칸(수량 · 가격 등)은 버린다(P7). `side`가 `buy`가 아니거나 모양이 틀리면
    ///   매매 담당 응답 실패([`Error::RoleUnavailable`]).
    /// - 주문 안의 종목은 매매의 종목이다.
    pub async fn draft_order(&self, trade: TradeId) -> Result<OrderDraft, Error> {
        let _ = trade;
        todo!()
    }
}
