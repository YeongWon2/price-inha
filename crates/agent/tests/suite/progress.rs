//! 논의 진행 신호 · 호출 예산 · 정리 모드 · 팀장 예비분(10차, C21 · M13 · E51 · E52 · N14 · V53).
use std::sync::{Arc, Mutex};

use pi_agent::{
    CallBudget, Decision, DiscussionOptions, DiscussionRequest, ProgressSignal, ProgressSink, Role,
    SignalError, Stage, TURN_RETRY_MAX, WRAP_UP_LINE, WatchReason,
};
use pi_ai::AiError;
use serde_json::json;

use crate::common::{
    Env, Step, leader_buy, model, of_kind, request, say, score_reply, script_after_discussion,
    trader_buy,
};

/// 받은 신호를 모두 남기는 포트.
#[derive(Default)]
struct Recorder {
    seen: Mutex<Vec<ProgressSignal>>,
}

impl Recorder {
    fn all(&self) -> Vec<ProgressSignal> {
        self.seen.lock().unwrap().clone()
    }

    /// 그 담당의 논의 턴 단계 · 다시 시도 신호만, 낸 순서대로.
    fn of(&self, role: Role) -> Vec<ProgressSignal> {
        self.all()
            .into_iter()
            .filter(|signal| match signal {
                ProgressSignal::Member { role: r, .. } | ProgressSignal::Retry { role: r, .. } => {
                    *r == role
                }
                _ => false,
            })
            .collect()
    }
}

impl ProgressSink for Recorder {
    fn signal(&self, signal: ProgressSignal) -> Result<(), SignalError> {
        self.seen.lock().unwrap().push(signal);
        Ok(())
    }
}

/// 언제나 실패하는 포트(받은 횟수는 센다).
#[derive(Default)]
struct Broken {
    tries: Mutex<u32>,
}

impl ProgressSink for Broken {
    fn signal(&self, _signal: ProgressSignal) -> Result<(), SignalError> {
        *self.tries.lock().unwrap() += 1;
        Err(SignalError("화면이 닫혔다".to_string()))
    }
}

fn member(role: Role, stage: Stage) -> ProgressSignal {
    ProgressSignal::Member { role, stage }
}

/// 뉴스 공시 시세 · 전략 두 담당이 논의하는 요청(합의 기준 100).
fn two(env: &Env) -> DiscussionRequest {
    let mut req = request(env.trade());
    req.members = vec![Role::Scout, Role::Strategy];
    req.consensus_required = 100;
    req
}

/// 두 담당 논의 뒤의 대본: 점수 70 · 80, 사기, 주문 안.
fn script_two(env: &Env) {
    env.ai.script(
        Role::Scout,
        vec![say("공시가 좋다"), say(&score_reply(70, "공시"))],
    );
    env.ai
        .script(Role::Strategy, vec![say(&score_reply(80, "추세"))]);
    env.ai.script(Role::Leader, vec![say(&leader_buy())]);
    env.ai
        .script(Role::Trader, vec![say(&trader_buy("공시 뒤 사기"))]);
}

fn leader_watch() -> Step {
    say("{\"decision\": \"watch\", \"reason\": \"정리\", \"revisit\": \"내일\"}")
}

fn leader_saw_wrap_up(env: &Env) -> bool {
    env.ai
        .requests()
        .iter()
        .filter(|r| r.model == model(Role::Leader))
        .any(|r| r.messages.iter().any(|m| m.content.contains(WRAP_UP_LINE)))
}

fn wrap_up_logs(env: &Env) -> usize {
    of_kind(&env.room("room-1"), "room.activity")
        .iter()
        .filter(|e| e.payload == json!({"status": "wrap_up"}))
        .count()
}

/// 방 로그를 (종류, 쓴 담당 id, 내용)으로 줄인 것. 줄 id는 빼고 견준다.
fn log_shape(env: &Env) -> Vec<(String, String, String)> {
    env.room("room-1")
        .iter()
        .map(|e| (e.kind.clone(), e.actor_id.clone(), e.payload.to_string()))
        .collect()
}

// 검증: R5-11 C21 V53 (논의가 열리면 참가 담당이 모두 대기로 시작한다)
#[tokio::test]
async fn 논의를_열면_참가_담당이_명단_순서로_대기가_된다() {
    let env = Env::new();
    let req = request(env.trade());
    env.ai.script(Role::Scout, vec![say("특별한 건 없다")]);
    script_after_discussion(&env.ai);
    let sink = Arc::new(Recorder::default());
    let options = DiscussionOptions::default().with_signals(sink.clone());
    env.team().discuss_with(req, options).await.unwrap();
    assert_eq!(
        sink.all().first(),
        Some(&ProgressSignal::Opened {
            members: vec![Role::Scout, Role::Strategy, Role::Risk]
        })
    );
}

// 검증: R5-11 C21 V53 (담당마다 대기 → 진행 → 완료, 점수 턴 · 팀장 판단 · 끝이 차례로)
#[tokio::test]
async fn 신호가_논의_순서대로_한_번씩_나온다() {
    let env = Env::new();
    let req = two(&env);
    script_two(&env);
    let sink = Arc::new(Recorder::default());
    let options = DiscussionOptions::default().with_signals(sink.clone());
    let out = env.team().discuss_with(req, options).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    assert_eq!(
        sink.all(),
        vec![
            ProgressSignal::Opened {
                members: vec![Role::Scout, Role::Strategy]
            },
            member(Role::Scout, Stage::Running),
            member(Role::Scout, Stage::Done),
            ProgressSignal::ScoreTurn { role: Role::Scout },
            ProgressSignal::ScoreTurn {
                role: Role::Strategy
            },
            ProgressSignal::LeaderTurn,
            ProgressSignal::Closed,
        ]
    );
}

// 검증: R5-11 C21 V53 (지켜보기로 끝나도 끝 신호는 마지막에 한 번)
#[tokio::test]
async fn 지켜보기로_끝나도_끝_신호가_마지막이다() {
    let env = Env::new();
    let req = two(&env);
    env.ai.script(
        Role::Scout,
        vec![say("공시가 좋다"), say(&score_reply(70, "공시"))],
    );
    env.ai
        .script(Role::Strategy, vec![say(&score_reply(80, "추세"))]);
    env.ai.script(Role::Leader, vec![leader_watch()]);
    let sink = Arc::new(Recorder::default());
    let options = DiscussionOptions::default().with_signals(sink.clone());
    let out = env.team().discuss_with(req, options).await.unwrap();
    assert!(matches!(
        out.decision,
        Decision::Watch(WatchReason::LeaderWatch { .. })
    ));
    let all = sink.all();
    assert_eq!(all.last(), Some(&ProgressSignal::Closed));
    assert_eq!(
        all.iter().filter(|s| **s == ProgressSignal::Closed).count(),
        1
    );
}

// 검증: E51 C21 E6 (턴이 실패하면 그 담당이 오류로 보이고, 다시 시도 없이 응답 실패로 간다)
#[tokio::test]
async fn 턴이_실패하면_오류_신호가_나오고_다시_시도는_없다() {
    let env = Env::new();
    let req = two(&env);
    env.ai
        .script(Role::Scout, vec![Step::Fail(AiError::InvalidResponse)]);
    let sink = Arc::new(Recorder::default());
    let options = DiscussionOptions::default().with_signals(sink.clone());
    let out = env.team().discuss_with(req, options).await.unwrap();
    assert_eq!(
        out.decision,
        Decision::Watch(WatchReason::RoleUnavailable(Role::Scout))
    );
    assert_eq!(
        sink.of(Role::Scout),
        vec![
            member(Role::Scout, Stage::Running),
            member(Role::Scout, Stage::Error)
        ]
    );
    assert_eq!(env.ai.calls(Role::Scout), 1);
    assert_eq!(sink.all().last(), Some(&ProgressSignal::Closed));
}

// 검증: E51 C21 (다시 시도 1/3: 오류 → 다시 시도 → 진행 → 완료)
#[tokio::test]
async fn 다시_시도하면_일에서_삼_신호가_나오고_성공하면_완료다() {
    let env = Env::new();
    let req = two(&env);
    env.ai.script(
        Role::Scout,
        vec![
            Step::Fail(AiError::InvalidResponse),
            say("공시가 좋다"),
            say(&score_reply(70, "공시")),
        ],
    );
    env.ai
        .script(Role::Strategy, vec![say(&score_reply(80, "추세"))]);
    env.ai.script(Role::Leader, vec![say(&leader_buy())]);
    env.ai
        .script(Role::Trader, vec![say(&trader_buy("공시 뒤 사기"))]);
    let sink = Arc::new(Recorder::default());
    let options = DiscussionOptions::default()
        .with_signals(sink.clone())
        .with_turn_retries(TURN_RETRY_MAX);
    let out = env.team().discuss_with(req, options).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    assert_eq!(
        sink.of(Role::Scout),
        vec![
            member(Role::Scout, Stage::Running),
            member(Role::Scout, Stage::Error),
            ProgressSignal::Retry {
                role: Role::Scout,
                attempt: 1,
                max: 3
            },
            member(Role::Scout, Stage::Running),
            member(Role::Scout, Stage::Done),
        ]
    );
    assert_eq!(env.ai.calls(Role::Scout), 3);
}

// 검증: E51 E6 C21 (3번까지 다시 시도하고 그래도 안 되면 응답 실패)
#[tokio::test]
async fn 다시_시도를_다_써도_안_되면_응답_실패로_지켜본다() {
    let env = Env::new();
    let req = two(&env);
    env.ai.script(
        Role::Scout,
        vec![
            Step::Fail(AiError::InvalidResponse),
            Step::Fail(AiError::InvalidResponse),
            Step::Fail(AiError::InvalidResponse),
            Step::Fail(AiError::InvalidResponse),
        ],
    );
    let sink = Arc::new(Recorder::default());
    let options = DiscussionOptions::default()
        .with_signals(sink.clone())
        .with_turn_retries(TURN_RETRY_MAX);
    let out = env.team().discuss_with(req, options).await.unwrap();
    assert_eq!(
        out.decision,
        Decision::Watch(WatchReason::RoleUnavailable(Role::Scout))
    );
    let retries: Vec<ProgressSignal> = sink
        .of(Role::Scout)
        .into_iter()
        .filter(|s| matches!(s, ProgressSignal::Retry { .. }))
        .collect();
    let expected: Vec<ProgressSignal> = (1..=3)
        .map(|attempt| ProgressSignal::Retry {
            role: Role::Scout,
            attempt,
            max: 3,
        })
        .collect();
    assert_eq!(retries, expected);
    assert_eq!(env.ai.calls(Role::Scout), 4);
    assert_eq!(
        sink.of(Role::Scout).last(),
        Some(&member(Role::Scout, Stage::Error))
    );
}

// 검증: R5-11 C21 (신호 포트가 계속 실패해도 논의 결과 · 방 로그는 같다)
#[tokio::test]
async fn 신호_포트가_실패해도_논의_결과는_같다() {
    let plain = Env::new();
    let req = two(&plain);
    script_two(&plain);
    let base = plain.team().discuss(req).await.unwrap();

    let env = Env::new();
    let req = two(&env);
    script_two(&env);
    let sink = Arc::new(Broken::default());
    let options = DiscussionOptions::default().with_signals(sink.clone());
    let out = env.team().discuss_with(req, options).await.unwrap();
    assert!(*sink.tries.lock().unwrap() > 0);
    assert_eq!(out, base);
    assert_eq!(log_shape(&env), log_shape(&plain));
}

// 검증: R1-5 M13 N14 (예산 · 포트를 넘기지 않으면 지금과 똑같이 돈다)
#[tokio::test]
async fn 옵션이_기본이면_기존_논의와_똑같다() {
    let plain = Env::new();
    let req = request(plain.trade());
    plain.ai.script(Role::Scout, vec![say("@strategy 봐 줘")]);
    plain.ai.script(Role::Strategy, vec![say("살 만하다")]);
    script_after_discussion(&plain.ai);
    let base = plain.team().discuss(req).await.unwrap();

    let env = Env::new();
    let req = request(env.trade());
    env.ai.script(Role::Scout, vec![say("@strategy 봐 줘")]);
    env.ai.script(Role::Strategy, vec![say("살 만하다")]);
    script_after_discussion(&env.ai);
    let out = env
        .team()
        .discuss_with(req, DiscussionOptions::default())
        .await
        .unwrap();
    assert_eq!(out, base);
    assert_eq!(log_shape(&env), log_shape(&plain));
    assert_eq!(env.ai.requests().len(), plain.ai.requests().len());
    assert_eq!(wrap_up_logs(&env), 0);
    assert!(!leader_saw_wrap_up(&env));
}

// 검증: R1-5 M13 N14 V53 (예산이 넉넉하면 지금과 같은 글 수 · 정리 모드 없음)
#[tokio::test]
async fn 예산이_넉넉하면_정리_모드_없이_같은_글_수다() {
    let plain = Env::new();
    let req = request(plain.trade());
    plain.ai.script(Role::Scout, vec![say("@strategy 봐 줘")]);
    plain.ai.script(Role::Strategy, vec![say("살 만하다")]);
    script_after_discussion(&plain.ai);
    let base = plain.team().discuss(req).await.unwrap();

    let env = Env::new();
    let req = request(env.trade());
    env.ai.script(Role::Scout, vec![say("@strategy 봐 줘")]);
    env.ai.script(Role::Strategy, vec![say("살 만하다")]);
    script_after_discussion(&env.ai);
    let budget = CallBudget {
        remaining: 100,
        reserve: 10,
        wrap_up_at: 30,
    };
    let options = DiscussionOptions::default().with_budget(budget);
    let out = env.team().discuss_with(req, options).await.unwrap();
    assert_eq!(out, base);
    assert_eq!(
        of_kind(&env.room("room-1"), "message.member").len(),
        of_kind(&plain.room("room-1"), "message.member").len()
    );
    assert_eq!(wrap_up_logs(&env), 0);
    assert!(!leader_saw_wrap_up(&env));
}

// 검증: R1-5 M13 N14 E52 V53 (문턱 이하면 새 라운드 없이 점수 · 팀장 판단, 정리 한 줄, 방 로그 wrap_up)
#[tokio::test]
async fn 문턱_이하면_새_라운드를_열지_않고_팀장이_정리한다() {
    let env = Env::new();
    let req = request(env.trade());
    env.ai.script(
        Role::Scout,
        vec![say("@strategy 봐 줘"), say(&score_reply(70, "공시"))],
    );
    env.ai.script(
        Role::Strategy,
        vec![say("@risk 어때"), say(&score_reply(80, "추세"))],
    );
    env.ai
        .script(Role::Risk, vec![say(&score_reply(60, "비중 작게"))]);
    env.ai.script(Role::Leader, vec![leader_watch()]);
    // 남은 7 · 예비분 2 · 문턱 5: 글 둘(7 → 5) 뒤 2라운드를 열 때 문턱에 닿는다.
    let budget = CallBudget {
        remaining: 7,
        reserve: 2,
        wrap_up_at: 5,
    };
    let options = DiscussionOptions::default().with_budget(budget);
    let out = env.team().discuss_with(req, options).await.unwrap();
    assert!(matches!(
        out.decision,
        Decision::Watch(WatchReason::LeaderWatch { .. })
    ));
    // 불린 위험 담당은 논의 글을 쓰지 못하고 점수 턴만 했다.
    assert_eq!(env.ai.calls(Role::Scout), 2);
    assert_eq!(env.ai.calls(Role::Strategy), 2);
    assert_eq!(env.ai.calls(Role::Risk), 1);
    assert_eq!(env.ai.calls(Role::Leader), 1);
    let events = env.room("room-1");
    let said = of_kind(&events, "message.member");
    assert!(
        said.iter()
            .filter(|e| e.payload.get("round").is_some())
            .all(|e| e.actor_id != "risk")
    );
    assert_eq!(out.scores.len(), 3);
    assert_eq!(wrap_up_logs(&env), 1);
    assert!(leader_saw_wrap_up(&env));
}

// 검증: M13 N14 E52 V53 (예비분은 담당 턴이 쓰지 못한다: 예산 = 예비분이면 담당 턴 0, 팀장 턴 1)
#[tokio::test]
async fn 예산이_예비분뿐이면_팀장만_부른다() {
    let env = Env::new();
    let req = request(env.trade());
    env.ai.script(Role::Leader, vec![leader_watch()]);
    let budget = CallBudget {
        remaining: 2,
        reserve: 2,
        wrap_up_at: 4,
    };
    let options = DiscussionOptions::default().with_budget(budget);
    let out = env.team().discuss_with(req, options).await.unwrap();
    for role in [Role::Scout, Role::Strategy, Role::Risk, Role::Trader] {
        assert_eq!(env.ai.calls(role), 0);
    }
    assert_eq!(env.ai.calls(Role::Leader), 1);
    assert!(matches!(
        out.decision,
        Decision::Watch(WatchReason::LeaderWatch { .. })
    ));
}

// 검증: M13 N14 E52 (문턱에 닿지 않아도 예비분을 넘겨 쓰지 못한다)
#[tokio::test]
async fn 담당_턴은_예비분까지_쓰지_못한다() {
    let env = Env::new();
    let req = request(env.trade());
    env.ai.script(Role::Scout, vec![say("@strategy 봐 줘")]);
    env.ai.script(Role::Leader, vec![leader_watch()]);
    // 남은 3 · 예비분 2 · 문턱 없음: 담당 턴은 하나뿐이다.
    let budget = CallBudget {
        remaining: 3,
        reserve: 2,
        wrap_up_at: 0,
    };
    let options = DiscussionOptions::default().with_budget(budget);
    let out = env.team().discuss_with(req, options).await.unwrap();
    assert_eq!(env.ai.calls(Role::Scout), 1);
    assert_eq!(env.ai.calls(Role::Strategy), 0);
    assert_eq!(env.ai.calls(Role::Risk), 0);
    assert_eq!(env.ai.calls(Role::Leader), 1);
    assert_eq!(env.ai.requests().len(), 2);
    assert!(matches!(
        out.decision,
        Decision::Watch(WatchReason::LeaderWatch { .. })
    ));
}

// 검증: E52 R5-11 (호출이 모자라 못 한 담당은 오류가 아니라 신호 없이 기권이다: 오류 신호가 없다)
#[tokio::test]
async fn 호출이_모자라_못_한_담당은_오류_신호가_없다() {
    let env = Env::new();
    let req = request(env.trade());
    env.ai.script(Role::Scout, vec![say("@strategy 봐 줘")]);
    env.ai.script(Role::Leader, vec![leader_watch()]);
    let budget = CallBudget {
        remaining: 3,
        reserve: 2,
        wrap_up_at: 0,
    };
    let sink = Arc::new(Recorder::default());
    let options = DiscussionOptions::default()
        .with_budget(budget)
        .with_signals(sink.clone());
    env.team().discuss_with(req, options).await.unwrap();
    assert!(sink.all().iter().all(|s| !matches!(
        s,
        ProgressSignal::Member {
            stage: Stage::Error,
            ..
        }
    )));
    assert!(sink.of(Role::Strategy).is_empty());
}
