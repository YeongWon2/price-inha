//! 턴의 상한 · 시간 제한 · 오류 상한(만드는 것 2번, 결정 16번, E6). 시간은 tokio 멈춘 시계로 정한다.

use pi_agent::{
    Error, MAX_EMPTY_REPLIES, MAX_MALFORMED_REPLIES, MAX_UNKNOWN_TOOL_REPLIES, Role, RoleConfig,
    TimeLimits, TurnFailure,
};

use crate::common::{Env, Step, action, model, say};

fn config(role: Role) -> RoleConfig {
    RoleConfig::new(role, model(role))
}

async fn run(env: &Env, limits: TimeLimits) -> Result<pi_agent::TurnOutcome, Error> {
    env.runner()
        .run_turn(&config(Role::Scout), limits, "market", "계기 확인")
        .await
}

fn failure(result: Result<pi_agent::TurnOutcome, Error>) -> TurnFailure {
    match result {
        Err(Error::RoleUnavailable {
            role: Role::Scout,
            cause,
        }) => cause,
        Err(other) => panic!("다른 오류: {other}"),
        Ok(out) => panic!("끝나 버림: {}", out.text),
    }
}

// 검증: E6 결정 16 (반복 상한 20에 닿으면 담당 응답 실패)
#[tokio::test]
async fn 반복_상한에_닿으면_응답_실패다() {
    let env = Env::new();
    env.ai
        .script(Role::Scout, vec![Step::Forever(action("quote"))]);
    assert_eq!(
        failure(run(&env, TimeLimits::MARKET).await),
        TurnFailure::IterationLimit
    );
    assert_eq!(env.ai.calls(Role::Scout), 20);
    assert_eq!(env.tools.calls().len(), 20);
}

// 검증: E6 (상한은 담당 설정을 따른다)
#[tokio::test]
async fn 반복_상한은_담당_설정을_따른다() {
    let env = Env::new();
    env.ai
        .script(Role::Scout, vec![Step::Forever(action("quote"))]);
    let mut scout = config(Role::Scout);
    scout.max_iterations = 2;
    let result = env
        .runner()
        .run_turn(&scout, TimeLimits::MARKET, "market", "x")
        .await;
    assert_eq!(failure(result), TurnFailure::IterationLimit);
    assert_eq!(env.ai.calls(Role::Scout), 2);
}

// 검증: E6 (오류 상한을 표로: 모르는 도구 3 · 깨진 JSON 3 · 빈 답 3)
#[tokio::test]
async fn 오류가_연달아_상한에_닿으면_응답_실패다() {
    assert_eq!(
        (
            MAX_UNKNOWN_TOOL_REPLIES,
            MAX_MALFORMED_REPLIES,
            MAX_EMPTY_REPLIES
        ),
        (3, 3, 3)
    );
    let cases = [
        ("모르는 도구", action("teleport"), TurnFailure::UnknownTools),
        (
            "깨진 JSON",
            "{\"action\": \"quote\", \"args\": {".to_string(),
            TurnFailure::MalformedAction,
        ),
        ("빈 답", "   ".to_string(), TurnFailure::EmptyReply),
    ];
    for (why, reply, expected) in cases {
        let env = Env::new();
        env.ai.script(Role::Scout, vec![Step::Forever(reply)]);
        assert_eq!(
            failure(run(&env, TimeLimits::MARKET).await),
            expected,
            "{why}"
        );
        assert_eq!(env.ai.calls(Role::Scout), 3, "{why}");
        assert!(env.tools.calls().is_empty(), "{why}");
    }
}

// 검증: E6 (상한 아래에서 바로잡으면 턴은 끝까지 간다)
#[tokio::test]
async fn 상한_전에_바로잡으면_끝난다() {
    let cases = [
        ("모르는 도구", action("teleport")),
        ("깨진 JSON", "{\"action\": 5}".to_string()),
        ("빈 답", String::new()),
    ];
    for (why, reply) in cases {
        let env = Env::new();
        let steps = vec![
            Step::Say(reply.clone()),
            Step::Say(reply),
            say("바로잡은 답"),
        ];
        env.ai.script(Role::Scout, steps);
        let out = run(&env, TimeLimits::MARKET).await.unwrap();
        assert_eq!(out.text, "바로잡은 답", "{why}");
        assert_eq!(out.iterations, 3, "{why}");
    }
}

// 검증: E6 (세기는 연달아일 때만: 중간에 정상 행동이 오면 0으로)
#[tokio::test]
async fn 정상_행동이_끼면_세기를_되돌린다() {
    let env = Env::new();
    let bad = action("teleport");
    let steps = vec![
        Step::Say(bad.clone()),
        Step::Say(bad.clone()),
        Step::Say(action("quote")),
        Step::Say(bad.clone()),
        Step::Say(bad),
        say("끝"),
    ];
    env.ai.script(Role::Scout, steps);
    let out = run(&env, TimeLimits::MARKET).await.unwrap();
    assert_eq!(out.text, "끝");
    assert_eq!(out.iterations, 6);
}

// 검증: E6 결정 16 (장중 무응답 60초를 넘기면 실패)
#[tokio::test(start_paused = true)]
async fn 장중_무응답_60초를_넘기면_응답_실패다() {
    let env = Env::new();
    env.ai
        .script(Role::Scout, vec![Step::Slow(61, "늦은 답".into())]);
    assert_eq!(
        failure(run(&env, TimeLimits::MARKET).await),
        TurnFailure::IdleTimeout
    );

    let env = Env::new();
    env.ai
        .script(Role::Scout, vec![Step::Slow(59, "제때 답".into())]);
    assert_eq!(run(&env, TimeLimits::MARKET).await.unwrap().text, "제때 답");
}

// 검증: E6 결정 16 (장중 전체 180초를 넘기면 실패)
#[tokio::test(start_paused = true)]
async fn 장중_전체_180초를_넘기면_응답_실패다() {
    let env = Env::new();
    let slow = Step::Slow(50, action("quote"));
    env.ai.script(
        Role::Scout,
        vec![slow.clone(), slow.clone(), slow.clone(), slow, say("끝")],
    );
    assert_eq!(
        failure(run(&env, TimeLimits::MARKET).await),
        TurnFailure::TotalTimeout
    );
    assert!(env.ai.calls(Role::Scout) <= 4);
}

// 검증: E6 결정 16 (채팅은 무응답 180초 · 전체 600초)
#[tokio::test(start_paused = true)]
async fn 채팅은_시간_제한이_더_길다() {
    let env = Env::new();
    let slow = Step::Slow(170, action("quote"));
    env.ai.script(
        Role::Scout,
        vec![slow.clone(), slow.clone(), slow, say("끝")],
    );
    let out = run(&env, TimeLimits::CHAT).await.unwrap();
    assert_eq!(out.text, "끝");

    let env = Env::new();
    env.ai
        .script(Role::Scout, vec![Step::Slow(181, "늦음".into())]);
    assert_eq!(
        failure(run(&env, TimeLimits::CHAT).await),
        TurnFailure::IdleTimeout
    );

    let env = Env::new();
    let slow = Step::Slow(170, action("quote"));
    env.ai.script(
        Role::Scout,
        vec![slow.clone(), slow.clone(), slow.clone(), slow, say("끝")],
    );
    assert_eq!(
        failure(run(&env, TimeLimits::CHAT).await),
        TurnFailure::TotalTimeout
    );
}
