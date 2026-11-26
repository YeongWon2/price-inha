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
//!    - 답이 비었거나 `(pass)`(대소문자 무시)면 넘김이다: 글을 쓰지 않는다. 턴 실행기가 빈 답을
//!      다 써서 실패한 것([`TurnFailure::EmptyReply`])도 넘김이다(원본).
//!      다만 0라운드에 지명된 담당의 넘김은 응답 실패다.
//!    - 글은 방 로그 `message.member`(쓴 쪽 `Member`, id는 담당 id)로 남는다. 내용은
//!      `{"text": <글>, "round": <라운드>, "handoff": [<부른 담당 id>, …]}`다.
//!      `handoff`는 그 글이 부른 다른 참가 담당(명단 순서, 겹침 없음)이다 — 넘기기 기록(V2).
//!    - 담당 턴이 실패하면 방 로그 `turn.failed`(쓴 쪽 `Gateway`, 내용에 `"member": <담당 id>`)를 남긴다.
//!    - 응답자가 없는 라운드(조용한 라운드)면 끝난다. 라운드 상한 · 글 수 상한에 닿으면
//!      지켜보기([`WatchReason::DiscussionBounded`]).
//! 4. 점수 턴(결정 15번): 팀장을 뺀 참가 담당마다 점수 JSON([`crate::parse_score`])을 받는다.
//!    점수 턴은 방 로그에 쓰지 않고 담당 세션에만 남는다. 담당 하나라도 실패하면 합의 불가로 지켜보기.
//!    답에 의견 칸(`stance` · `conviction` · `opinion_reason` · `horizon_days`)이 온전하면 그 담당의
//!    의견도 받는다(11차, `opinion.rs`). 없거나 틀리면 점수만 받는다(점수 턴 실패가 아니다).
//! 5. 합의 점수([`crate::consensus`])가 기준을 넘지 못하면 지켜보기([`WatchReason::ConsensusBelow`], E27).
//! 6. 팀장 판단 턴. 답은 `decision` 칸이 있는 객체 하나다:
//!    - 사기 · 팔기: `{"decision": "buy"|"sell", "hypothesis": 글, "trigger": 글, "entry": 정수,
//!      "stop": 정수, "target": 정수, "expected_value": 정수}`.
//!      사기 · 팔기 모두 근거 칸이 하나라도 비었거나 금액 칸이 0 · 음수면 지켜보기([`WatchReason::RationaleIncomplete`], P9).
//!      아니면 `judge_trade_with_side`로 근거 · 자료(요청의 자료 이름과 알게 된 시각) · 담당 점수 · 합의 점수와
//!      방향을 함께 남긴다(판단 시각은 시계). 매매는 판단됨이 된다(V3, M2).
//!    - 사기 · 팔기 답에 `summary` · `reversal`이 둘 다 공백 아닌 글이면(11차) `judge_trade_with_opinions`로
//!      판단 · 방향과 함께 담당 의견들과 팀장 요약을 한 쓰기로 남긴다. 아니면 위처럼 `judge_trade_with_side`.
//!    - 사용자 공지를 넘겼으면 자료 목록에 [`crate::NOTICE_SOURCE_NAME`] 한 줄(가장 늦은 보낸 시각)이 더 든다.
//!    - 지켜보기: `{"decision": "watch", "reason": 글, "revisit": 글}` → [`WatchReason::LeaderWatch`].
//!    - 모양이 틀리면 팀장 응답 실패로 지켜보기.
//!    - 판단은 방 로그 `message.member`(쓴 쪽 팀장)로 남고 내용에 `"decision": "buy"|"sell"|"watch"`가 있다.
//! 7. 사기면 [`Team::draft_order`]와 같은 길로 매매 담당이 주문 안을 만든다(V4).
//!    팔기면 [`Team::draft_sell`]과 같은 길로 같은 종목 · 계좌 종류의 보유 매매마다 파는 주문 안을 만든다(9차).
//!    팔 보유가 없으면 매매 담당을 부르지 않고 방 로그에 `room.activity`
//!    (`{"status": "nothing_held", "trade": <번호>}`)를 남긴다.
//!
//! 지켜보기로 끝나면 매매는 논의중 그대로이고(판단을 저장하지 않는다) 주문 안을 만들지 않는다.
//! 그때 방 로그에 `room.activity`(쓴 쪽 `Gateway`, 내용 `{"status": "watch", "reason_code": <코드>}`)를
//! 하나 남긴다. 방 로그 · 판단 · 주문 안에 남는 글은 모두 가린 것이다(E38).
//! 실행기는 승인을 붙이거나 매매를 승인대기 · 주문됨으로 옮기지 않는다(P21).

use pi_store::{RoleOpinion, RoleScore, TradeId, TradeSide, TradeState};
use serde_json::json;

use crate::discussion::{Decision, DiscussionOutcome, DiscussionRequest, WatchReason};
use crate::error::{Error, TurnFailure};
use crate::floor::{Floor, is_pass, mentions};
use crate::opinion::parse_opinion;
use crate::ports::{OrderDraft, Side};
use crate::progress::DiscussionOptions;
use crate::prompt;
use crate::role::{Role, RoleConfig, TimeLimits};
use crate::roomlog::{RoomLog, Writer};
use crate::score::{consensus, exceeds_threshold, parse_score};
use crate::turn::{Ports, TurnOutcome, TurnRunner};

mod judge;
mod run;
mod sell;
use crate::progress::{ProgressSignal, Stage};
use crate::verdict::parse_draft;
use run::Run;

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
    /// 답하지 못했다(E6). 그 까닭.
    Failed(TurnFailure),
}

/// 점수 턴의 결과: 담당 점수와 새 칸이 온전한 담당의 의견(11차).
type Scored = (Vec<RoleScore>, Vec<RoleOpinion>);

/// 논의 하나를 도는 동안 함께 들고 다니는 것.
struct Session<'a> {
    request: &'a DiscussionRequest,
    room: RoomLog<'a>,
    briefing: String,
    run: &'a Run,
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
    /// [`Team::discuss_with`]에 기본 옵션을 넘긴 것과 같다.
    pub async fn discuss(&self, request: DiscussionRequest) -> Result<DiscussionOutcome, Error> {
        self.discuss_with(request, DiscussionOptions::default())
            .await
    }

    /// 논의 하나를 옵션(진행 신호 포트 · 호출 예산 · 다시 시도)과 함께 돈다(10차).
    ///
    /// - 옵션이 기본이면 [`Team::discuss`]와 똑같다.
    /// - 신호는 [`crate::ProgressSignal`] 순서로 낸다. 포트가 실패해도 논의는 그대로 간다.
    /// - 예산이 있으면 담당 턴은 `remaining - reserve`까지만 쓴다(예비분은 팀장 몫).
    ///   남은 호출이 `wrap_up_at` 이하이면 새 라운드를 열지 않고 점수 턴 · 팀장 판단으로 넘어간다.
    ///   그때 방 로그에 `room.activity`(`{"status": "wrap_up"}`)를 남기고 팀장 턴 글에
    ///   [`crate::WRAP_UP_LINE`]을 넣는다. 담당 턴을 쓸 호출이 없으면 그 담당은 기권이다
    ///   (점수 턴을 하지 않고, 합의 점수는 점수를 낸 담당만으로 따진다. 아무도 못 냈으면
    ///   합의 기준을 건너뛰고 팀장 판단 턴으로 간다).
    ///
    /// 정해 둔 것(시험이 덮지 않는 곳):
    /// - 논의 중 담당 턴을 쓸 호출이 떨어지면 그 자리에서 논의를 끝내고 점수 턴으로 간다
    ///   (기권한 담당은 신호를 내지 않는다). 문턱에 닿은 것이 아니므로 정리 모드는 아니다.
    /// - 다시 시도는 논의 턴에만 한다(점수 턴 · 팀장 턴은 지금처럼 한 번). 다시 시도도 호출 하나를
    ///   쓰고, 쓸 호출이 없으면 그만둔다. `Retry`의 `max`는 옵션으로 받은 다시 시도 수다.
    /// - 빈 답을 다 써서 넘김이 된 턴([`TurnFailure::EmptyReply`])은 실패가 아니라 완료다.
    /// - 팀장 턴은 예비분에서 나가므로 막지 않는다. 판단 뒤 매매 담당의 주문 안 턴은 세지 않는다.
    /// - 요청이 틀리면([`Error::InvalidMembers`]) 신호를 내지 않는다. 그 밖에는 오류로 끝나도
    ///   마지막에 `Closed`를 한 번 낸다.
    pub async fn discuss_with(
        &self,
        request: DiscussionRequest,
        options: DiscussionOptions,
    ) -> Result<DiscussionOutcome, Error> {
        check_members(&request)?;
        let run = Run::new(options);
        run.signal(ProgressSignal::Opened {
            members: request.members.clone(),
        });
        let result = self.discuss_run(&request, &run).await;
        run.signal(ProgressSignal::Closed);
        result
    }

    /// 확인된 요청으로 논의를 끝까지 돈다(모듈 문서 2번부터).
    async fn discuss_run(
        &self,
        request: &DiscussionRequest,
        run: &Run,
    ) -> Result<DiscussionOutcome, Error> {
        let trade = self.ports.store.get_trade(request.trade)?;
        let session = Session {
            request,
            run,
            room: RoomLog::new(
                Writer {
                    store: &self.ports.store,
                    clock: self.ports.clock.as_ref(),
                    redactor: self.ports.redactor.as_ref(),
                },
                &request.room,
                request.trade,
            )?,
            briefing: prompt::briefing(request, &trade.symbol)
                + &prompt::notices(run.notices(), self.ports.redactor.as_ref())
                // (기능 L) 팀 지침 블록: 자료 · 공지 뒤 정해진 자리.
                + &crate::guideline::block(run.guidelines(), self.ports.redactor.as_ref()),
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
        let (scores, opinions) = match self.score_all(&session, &floor).await? {
            Ok(scored) => scored,
            Err(role) => {
                return watched(
                    &session,
                    WatchReason::RoleUnavailable(role),
                    Vec::new(),
                    None,
                );
            }
        };
        let total = match consensus(&scores) {
            // 호출이 모자라 아무도 점수를 내지 못했으면 합의 기준을 건너뛰고 팀장이 정리만 한다(10차).
            // 이때 팀장의 사기 · 팔기 답은 판단으로 받지 않는다(`judge.rs` — 기권이 있으면 지켜보기).
            None if run.abstained() => None,
            None => {
                let reason = WatchReason::ConsensusBelow {
                    consensus: 0,
                    required: request.consensus_required,
                };
                return watched(&session, reason, scores, None);
            }
            Some(total) if !exceeds_threshold(total, request.consensus_required) => {
                let reason = WatchReason::ConsensusBelow {
                    consensus: total,
                    required: request.consensus_required,
                };
                return watched(&session, reason, scores, Some(total));
            }
            Some(total) => Some(total),
        };
        self.judge(&session, floor.said(), scores, opinions, total)
            .await
    }

    /// 매매 담당이 주문 안을 만들어 도구 포트로 넘긴다(V4).
    ///
    /// - 매매가 판단됨이 아니면 [`Error::TradeNotJudged`]이고 AI를 부르지 않고 아무것도 넘기지 않는다(E20).
    /// - 매매 담당의 답은 `side` 칸이 있는 객체 하나다: `{"side": "buy", "note": 글}`.
    ///   다른 칸(수량 · 가격 등)은 버린다(P7). `side`가 `buy`가 아니거나 모양이 틀리면
    ///   매매 담당 응답 실패([`Error::RoleUnavailable`]).
    /// - 저장소에 남은 방향이 팔기면 [`Error::TradeNotBuy`]다(다시 띄운 팀도 같다). AI를 부르지 않는다.
    /// - 주문 안의 종목은 매매의 종목이다.
    pub async fn draft_order(&self, trade: TradeId) -> Result<OrderDraft, Error> {
        let found = self.ports.store.get_trade(trade)?;
        if found.state != TradeState::Judged {
            return Err(Error::TradeNotJudged {
                trade,
                state: found.state,
            });
        }
        if self.ports.store.trade_side(trade)? == Some(TradeSide::Sell) {
            return Err(Error::TradeNotBuy { trade });
        }
        let out = self
            .turn(Role::Trader, trade, &prompt::order_turn(&found))
            .await?;
        let reply = parse_draft(&out.text).ok_or(Error::RoleUnavailable {
            role: Role::Trader,
            cause: TurnFailure::InvalidStructuredReply,
        })?;
        let draft = OrderDraft {
            trade,
            symbol: found.symbol,
            side: Side::Buy,
            note: reply.note,
            ai_quantity: reply.ai_quantity,
            ai_price: reply.ai_price,
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
            Err(Error::RoleUnavailable { cause, .. }) => Ok(Spoken::Failed(cause)),
            Err(other) => Err(other),
        }
    }

    /// 논의 턴 하나: 진행 신호를 내고, 실패하면 옵션의 수만큼 다시 시도한다(10차, E51).
    /// 첫 호출은 부르는 쪽이 이미 셌다. 다시 시도는 호출 하나씩을 더 쓴다.
    async fn discussion_turn(
        &self,
        session: &Session<'_>,
        role: Role,
        input: &str,
    ) -> Result<Spoken, Error> {
        let run = session.run;
        let mut attempt = 0;
        loop {
            run.signal(ProgressSignal::Member {
                role,
                stage: Stage::Running,
            });
            let spoken = self.speak(session, role, input).await;
            let failed = match &spoken {
                Ok(Spoken::Failed(cause)) => *cause != TurnFailure::EmptyReply,
                Ok(Spoken::Text(_)) => false,
                Err(_) => true,
            };
            let stage = if failed { Stage::Error } else { Stage::Done };
            run.signal(ProgressSignal::Member { role, stage });
            let retry = matches!(spoken, Ok(Spoken::Failed(_)))
                && failed
                && attempt < run.retries()
                && run.member_call();
            if !retry {
                return spoken;
            }
            attempt += 1;
            run.signal(ProgressSignal::Retry {
                role,
                attempt,
                max: run.retries(),
            });
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
            if session.run.wrap_up_due() {
                session.run.enter_wrap_up();
                session.room.wrap_up()?;
                return Ok(None);
            }
            let shift = usize::try_from(round).unwrap_or(0) % responders.len();
            responders.rotate_left(shift);
            for role in responders {
                if floor.count() >= usize::try_from(limits.max_messages).unwrap_or(usize::MAX) {
                    return Ok(Some(WatchReason::DiscussionBounded));
                }
                if !session.run.member_call() {
                    // 담당 턴을 쓸 호출이 없다: 기권하고 논의를 끝낸다(예비분은 팀장 몫).
                    return Ok(None);
                }
                let briefing = floor.is_new(role).then_some(session.briefing.as_str());
                let input = prompt::discussion_turn(briefing, round, &floor.unseen(role));
                let text = match self.discussion_turn(session, role, &input).await? {
                    Spoken::Text(text) => text,
                    Spoken::Failed(TurnFailure::EmptyReply) => String::new(),
                    Spoken::Failed(cause) => return failed(session, role, &cause.to_string()),
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
    /// 점수와 함께 새 칸이 온전한 담당의 의견(11차, D13)을 준다.
    /// 논의 턴이 없어 안내를 받지 못한 담당은 점수 턴 글 앞에 안내(자료 · 사용자 공지)를 받는다(11차).
    async fn score_all(
        &self,
        session: &Session<'_>,
        floor: &Floor,
    ) -> Result<Result<Scored, Role>, Error> {
        let redactor = self.ports.redactor.as_ref();
        let mut scores = Vec::new();
        let mut opinions = Vec::new();
        for role in session.request.members.iter().copied() {
            if role == Role::Leader {
                continue;
            }
            if !session.run.member_call() {
                // 호출이 모자라 점수 턴을 기권한다(10차). 신호도 내지 않는다.
                session.run.abstain();
                continue;
            }
            session.run.signal(ProgressSignal::ScoreTurn { role });
            let briefing = floor.is_new(role).then_some(session.briefing.as_str());
            let score_input = prompt::score_turn(briefing);
            let parsed = match self.speak(session, role, &score_input).await? {
                Spoken::Text(text) => parse_score(&text)
                    .map(|scored| (scored, parse_opinion(&text, role, redactor)))
                    .ok_or_else(|| TurnFailure::InvalidStructuredReply.to_string()),
                Spoken::Failed(cause) => Err(cause.to_string()),
            };
            match parsed {
                Ok(((score, reason), opinion)) => {
                    scores.push(RoleScore {
                        role: role.id().to_string(),
                        score,
                        reason,
                    });
                    opinions.extend(opinion);
                }
                Err(reason) => {
                    session.room.failed(role, &reason)?;
                    return Ok(Err(role));
                }
            }
        }
        Ok(Ok((scores, opinions)))
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
        sells: Vec::new(),
    })
}

/// 논의 중 담당 응답 실패로 지켜보기.
fn failed(session: &Session<'_>, role: Role, reason: &str) -> Result<Option<WatchReason>, Error> {
    session.room.failed(role, reason)?;
    Ok(Some(WatchReason::RoleUnavailable(role)))
}
