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

use pi_store::{Judgment, RoleScore, SourceRef, TradeId, TradeState};
use serde_json::json;

use crate::discussion::{DataItem, Decision, DiscussionOutcome, DiscussionRequest, WatchReason};
use crate::error::{Error, TurnFailure};
use crate::floor::{Floor, is_pass, mentions};
use crate::ports::{OrderDraft, Side};
use crate::prompt;
use crate::role::{Role, RoleConfig, TimeLimits};
use crate::roomlog::RoomLog;
use crate::score::{consensus, exceeds_threshold, parse_score};
use crate::turn::{Ports, TurnOutcome, TurnRunner};
use crate::verdict::{Verdict, is_complete, parse_draft, parse_verdict};

/// 투자 팀. 담당 6명의 설정과 실행기를 가진다.
#[derive(Clone)]
pub struct Team {
    runner: TurnRunner,
    ports: Ports,
    roles: Vec<RoleConfig>,
    limits: TimeLimits,
}

/// 한 담당 턴의 끝.
enum Spoken {
    /// 답했다.
    Text(String),
    /// 답하지 못했다(E6). 까닭 글.
    Failed(String),
}

/// 논의 하나를 도는 동안 함께 들고 다니는 것.
struct Session<'a> {
    request: &'a DiscussionRequest,
    room: RoomLog<'a>,
    briefing: String,
}

impl Team {
    /// 만든다. `roles`는 6명을 한 번씩 담아야 한다(순서는 상관없다). 아니면 [`Error::InvalidRoles`].
    /// `limits`는 이 팀의 모든 턴에 쓴다.
    pub fn new(ports: Ports, roles: Vec<RoleConfig>, limits: TimeLimits) -> Result<Team, Error> {
        let complete = roles.len() == Role::ALL.len()
            && Role::ALL
                .iter()
                .all(|role| roles.iter().filter(|c| c.role == *role).count() == 1);
        if !complete {
            return Err(Error::InvalidRoles);
        }
        Ok(Team {
            runner: TurnRunner::new(ports.clone()),
            ports,
            roles,
            limits,
        })
    }

    /// 이 팀의 턴 실행기.
    pub fn runner(&self) -> &TurnRunner {
        &self.runner
    }

    /// 논의 하나를 끝까지 돈다(모듈 문서의 순서).
    pub async fn discuss(&self, request: DiscussionRequest) -> Result<DiscussionOutcome, Error> {
        check_members(&request)?;
        let trade = self.ports.store.get_trade(request.trade)?;
        let session = Session {
            request: &request,
            room: RoomLog::new(
                &self.ports.store,
                self.ports.clock.as_ref(),
                &request.room,
                request.trade,
            ),
            briefing: prompt::briefing(&request, &trade.symbol),
        };
        let missing: Vec<String> = request
            .data
            .iter()
            .filter(|item| item.content.is_none())
            .map(|item| item.name.clone())
            .collect();
        if !missing.is_empty() {
            return watched(
                &session,
                WatchReason::MissingData(missing),
                Vec::new(),
                None,
            );
        }
        let mut floor = Floor::default();
        if let Some(reason) = self.converse(&session, &mut floor).await? {
            return watched(&session, reason, Vec::new(), None);
        }
        let scores = match self.score_all(&session).await? {
            Ok(scores) => scores,
            Err(role) => {
                return watched(
                    &session,
                    WatchReason::RoleUnavailable(role),
                    Vec::new(),
                    None,
                );
            }
        };
        let Some(total) = consensus(&scores) else {
            let reason = WatchReason::ConsensusBelow {
                consensus: 0,
                required: request.consensus_required,
            };
            return watched(&session, reason, scores, None);
        };
        if !exceeds_threshold(total, request.consensus_required) {
            let reason = WatchReason::ConsensusBelow {
                consensus: total,
                required: request.consensus_required,
            };
            return watched(&session, reason, scores, Some(total));
        }
        self.judge(&session, floor.said(), scores, total).await
    }

    /// 매매 담당이 주문 안을 만들어 도구 포트로 넘긴다(V4).
    ///
    /// - 매매가 판단됨이 아니면 [`Error::TradeNotJudged`]이고 AI를 부르지 않고 아무것도 넘기지 않는다(E20).
    /// - 매매 담당의 답은 `side` 칸이 있는 객체 하나다: `{"side": "buy", "note": 글}`.
    ///   다른 칸(수량 · 가격 등)은 버린다(P7). `side`가 `buy`가 아니거나 모양이 틀리면
    ///   매매 담당 응답 실패([`Error::RoleUnavailable`]).
    /// - 주문 안의 종목은 매매의 종목이다.
    pub async fn draft_order(&self, trade: TradeId) -> Result<OrderDraft, Error> {
        let found = self.ports.store.get_trade(trade)?;
        if found.state != TradeState::Judged {
            return Err(Error::TradeNotJudged {
                trade,
                state: found.state,
            });
        }
        let out = self
            .turn(Role::Trader, trade, &prompt::order_turn(&found))
            .await?;
        let note = parse_draft(&out.text).ok_or(Error::RoleUnavailable {
            role: Role::Trader,
            cause: TurnFailure::InvalidStructuredReply,
        })?;
        let draft = OrderDraft {
            trade,
            symbol: found.symbol,
            side: Side::Buy,
            note,
        };
        self.ports.tools.submit_order_draft(&draft).await?;
        Ok(draft)
    }

    /// 담당 한 명의 턴. 세션 제목은 매매마다 하나다.
    async fn turn(&self, role: Role, trade: TradeId, input: &str) -> Result<TurnOutcome, Error> {
        let config = self
            .roles
            .iter()
            .find(|config| config.role == role)
            .ok_or(Error::InvalidRoles)?;
        let title = format!("trade-{}", trade.0);
        self.runner
            .run_turn(config, self.limits, &title, input)
            .await
    }

    /// 담당 턴을 돌고, 답하지 못했으면 까닭 글로 바꾼다.
    async fn speak(&self, session: &Session<'_>, role: Role, input: &str) -> Result<Spoken, Error> {
        match self.turn(role, session.request.trade, input).await {
            Ok(out) => Ok(Spoken::Text(out.text)),
            Err(Error::RoleUnavailable { cause, .. }) => Ok(Spoken::Failed(cause.to_string())),
            Err(other) => Err(other),
        }
    }

    /// 논의(결정 14번). 지켜보기로 끝나면 그 까닭을 준다.
    async fn converse(
        &self,
        session: &Session<'_>,
        floor: &mut Floor,
    ) -> Result<Option<WatchReason>, Error> {
        let request = session.request;
        let limits = request.limits;
        let mut responders: Vec<Role> = request
            .members
            .iter()
            .copied()
            .filter(|role| request.nominated.contains(role))
            .collect();
        let mut round: u32 = 0;
        while !responders.is_empty() {
            if round >= limits.max_rounds {
                return Ok(Some(WatchReason::DiscussionBounded));
            }
            let shift = usize::try_from(round).unwrap_or(0) % responders.len();
            responders.rotate_left(shift);
            for role in responders {
                if floor.count() >= usize::try_from(limits.max_messages).unwrap_or(usize::MAX) {
                    return Ok(Some(WatchReason::DiscussionBounded));
                }
                let briefing = floor.is_new(role).then_some(session.briefing.as_str());
                let input = prompt::discussion_turn(briefing, round, &floor.unseen(role));
                let text = match self.speak(session, role, &input).await? {
                    Spoken::Text(text) => text,
                    Spoken::Failed(reason) => return failed(session, role, &reason),
                };
                if is_pass(&text) {
                    if round == 0 {
                        return failed(session, role, "지명된 담당이 넘겼습니다");
                    }
                    floor.pass(role);
                    continue;
                }
                let handoff = mentions(&text, role, &request.members);
                let ids: Vec<&str> = handoff.iter().map(|r| r.id()).collect();
                let payload = json!({"text": text, "round": round, "handoff": ids});
                session.room.member(role, payload)?;
                floor.say(role, text, &handoff);
            }
            round += 1;
            responders = floor.pending(&request.members);
        }
        Ok(None)
    }

    /// 점수 턴(결정 15번): 팀장을 뺀 참가 담당마다. 하나라도 실패하면 그 담당을 준다.
    async fn score_all(
        &self,
        session: &Session<'_>,
    ) -> Result<Result<Vec<RoleScore>, Role>, Error> {
        let mut scores = Vec::new();
        for role in session.request.members.iter().copied() {
            if role == Role::Leader {
                continue;
            }
            let parsed = match self.speak(session, role, &prompt::score_turn()).await? {
                Spoken::Text(text) => parse_score(&text)
                    .ok_or_else(|| TurnFailure::InvalidStructuredReply.to_string()),
                Spoken::Failed(reason) => Err(reason),
            };
            match parsed {
                Ok((score, reason)) => scores.push(RoleScore {
                    role: role.id().to_string(),
                    score,
                    reason,
                }),
                Err(reason) => {
                    session.room.failed(role, &reason)?;
                    return Ok(Err(role));
                }
            }
        }
        Ok(Ok(scores))
    }

    /// 팀장 판단 턴과 그 뒤(판단 저장 · 주문 안).
    async fn judge(
        &self,
        session: &Session<'_>,
        said: &[(Role, String)],
        scores: Vec<RoleScore>,
        total: i32,
    ) -> Result<DiscussionOutcome, Error> {
        let input = prompt::leader_turn(&session.briefing, said, &scores, total);
        let verdict = match self.speak(session, Role::Leader, &input).await? {
            Spoken::Text(text) => match parse_verdict(&text) {
                Some(verdict) => {
                    let payload = json!({"text": text, "decision": verdict.code()});
                    session.room.member(Role::Leader, payload)?;
                    verdict
                }
                None => {
                    let reason = TurnFailure::InvalidStructuredReply.to_string();
                    return failed_scored(session, Role::Leader, &reason, scores, total);
                }
            },
            Spoken::Failed(reason) => {
                return failed_scored(session, Role::Leader, &reason, scores, total);
            }
        };
        let (side, rationale) = match verdict {
            Verdict::Watch { reason, revisit } => {
                let reason = WatchReason::LeaderWatch { reason, revisit };
                return watched(session, reason, scores, Some(total));
            }
            Verdict::Trade { side, rationale } => (side, rationale),
        };
        if side == Side::Buy && !is_complete(&rationale) {
            return watched(
                session,
                WatchReason::RationaleIncomplete,
                scores,
                Some(total),
            );
        }
        let judgment = Judgment {
            decided_at: self.ports.clock.now(),
            rationale,
            sources: sources(&session.request.data),
            scores: scores.clone(),
            consensus_score: total,
        };
        self.ports
            .store
            .judge_trade(session.request.trade, judgment)?;
        let (decision, order) = match side {
            Side::Sell => (Decision::Sell, None),
            Side::Buy => (Decision::Buy, self.order_after_judgment(session).await?),
        };
        Ok(DiscussionOutcome {
            decision,
            scores,
            consensus: Some(total),
            order,
        })
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

/// 요청 확인(모듈 문서 1번).
fn check_members(request: &DiscussionRequest) -> Result<(), Error> {
    let members = &request.members;
    let distinct = members
        .iter()
        .enumerate()
        .all(|(index, role)| !members.iter().take(index).any(|other| other == role));
    let nominated_ok = !request.nominated.is_empty()
        && request.nominated.iter().all(|role| members.contains(role));
    if members.len() < 2 || members.len() > request.limits.max_members || !distinct || !nominated_ok
    {
        return Err(Error::InvalidMembers);
    }
    Ok(())
}

/// 판단의 자료: 요청의 자료 이름과 알게 된 시각.
fn sources(data: &[DataItem]) -> Vec<SourceRef> {
    data.iter()
        .map(|item| SourceRef {
            name: item.name.clone(),
            known_at: item.known_at,
        })
        .collect()
}

/// 지켜보기로 끝낸다: 방 로그에 까닭 코드를 남긴다. 매매는 그대로다.
fn watched(
    session: &Session<'_>,
    reason: WatchReason,
    scores: Vec<RoleScore>,
    consensus: Option<i32>,
) -> Result<DiscussionOutcome, Error> {
    session.room.watch(reason.code())?;
    Ok(DiscussionOutcome {
        decision: Decision::Watch(reason),
        scores,
        consensus,
        order: None,
    })
}

/// 논의 중 담당 응답 실패로 지켜보기.
fn failed(session: &Session<'_>, role: Role, reason: &str) -> Result<Option<WatchReason>, Error> {
    session.room.failed(role, reason)?;
    Ok(Some(WatchReason::RoleUnavailable(role)))
}

/// 점수 뒤(팀장 턴) 응답 실패로 지켜보기.
fn failed_scored(
    session: &Session<'_>,
    role: Role,
    reason: &str,
    scores: Vec<RoleScore>,
    total: i32,
) -> Result<DiscussionOutcome, Error> {
    session.room.failed(role, reason)?;
    watched(
        session,
        WatchReason::RoleUnavailable(role),
        scores,
        Some(total),
    )
}
