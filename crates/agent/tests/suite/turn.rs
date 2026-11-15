//! 턴 실행기: 행동 → 도구, 저장 먼저, 가리기(만드는 것 2 · 6번).

use std::sync::Arc;

use pi_agent::{Error, Role, RoleConfig, TimeLimits, TurnFailure};
use pi_ai::{AiError, Role as AiRole};
use pi_store::{MessageRole, UnixMillis};

use crate::common::{Env, MASK, SECRET_MARK, Step, action, model, say};

fn config(role: Role) -> RoleConfig {
    RoleConfig::new(role, model(role))
}

// 검증: M2 (행동 없는 답은 그대로 끝)
#[tokio::test]
async fn 행동이_없는_답이면_한_번에_끝난다() {
    let env = Env::new();
    env.ai.script(Role::Scout, vec![say("특이사항 없습니다.")]);
    let out = env
        .runner()
        .run_turn(
            &config(Role::Scout),
            TimeLimits::MARKET,
            "chat",
            "오늘 시장 어때?",
        )
        .await
        .unwrap();
    assert_eq!(out.text, "특이사항 없습니다.");
    assert_eq!(out.iterations, 1);
    let messages = env.store.session_messages(out.session).unwrap();
    let roles: Vec<MessageRole> = messages.iter().map(|m| m.role).collect();
    assert_eq!(roles, [MessageRole::User, MessageRole::Assistant]);
    assert_eq!(messages[0].content.as_deref(), Some("오늘 시장 어때?"));
    assert_eq!(messages[1].content.as_deref(), Some("특이사항 없습니다."));
    let session = env.store.get_session(out.session).unwrap();
    assert_eq!(session.agent, "scout");
    assert_eq!(session.title, "chat");
    assert!(env.tools.calls().is_empty());
    let request = &env.ai.requests()[0];
    assert_eq!(request.model, model(Role::Scout));
    assert_eq!(request.messages.last().unwrap().role, AiRole::User);
}

// 검증: M2 (행동을 Tools 포트로 보내고 결과를 붙여 다시 묻는다)
#[tokio::test]
async fn 행동을_도구로_보내고_결과를_붙여_다시_묻는다() {
    let env = Env::new();
    env.ai.script(
        Role::Scout,
        vec![Step::Say(action("quote")), say("시세 확인 끝")],
    );
    let out = env
        .runner()
        .run_turn(
            &config(Role::Scout),
            TimeLimits::MARKET,
            "chat",
            "시세 봐 줘",
        )
        .await
        .unwrap();
    assert_eq!(out.text, "시세 확인 끝");
    assert_eq!(out.iterations, 2);
    let calls = env.tools.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, Role::Scout);
    assert_eq!(calls[0].1.name, "quote");
    assert_eq!(calls[0].1.args["symbol"], "005930");
    let second = &env.ai.requests()[1];
    assert!(
        second
            .messages
            .iter()
            .any(|m| m.content.contains("결과:quote"))
    );
    let session = env.store.get_session(out.session).unwrap();
    assert_eq!(session.tool_call_count, 1);
}

// 검증: E20 (도구를 부르기 전에 메시지를 먼저 저장한다)
#[tokio::test]
async fn 도구를_부르기_전에_메시지를_먼저_저장한다() {
    let env = Env::new();
    let reply = format!("{}\n{{\"action\": \"filing\"}}", action("quote"));
    env.ai
        .script(Role::Strategy, vec![Step::Say(reply), say("끝")]);
    env.runner()
        .run_turn(&config(Role::Strategy), TimeLimits::MARKET, "chat", "봐 줘")
        .await
        .unwrap();
    assert_eq!(env.tools.calls().len(), 2);
    assert_eq!(env.tools.persisted_first(), [true, true]);
}

// 검증: E20 (저장하지 못하면 도구를 부르지 않는다)
#[tokio::test]
async fn 저장하지_못하면_도구를_부르지_않는다() {
    let env = Env::new();
    let store = env.store.clone();
    let end_open = Arc::new(move || {
        for session in store.sessions_of("scout").unwrap() {
            if session.ended_at.is_none() {
                store
                    .end_session(session.id, "test", UnixMillis(1))
                    .unwrap();
            }
        }
    });
    env.ai
        .script(Role::Scout, vec![Step::Hook(end_open, action("quote"))]);
    let result = env
        .runner()
        .run_turn(&config(Role::Scout), TimeLimits::MARKET, "chat", "봐 줘")
        .await;
    assert!(matches!(
        result,
        Err(Error::RoleUnavailable {
            role: Role::Scout,
            cause: TurnFailure::Persistence
        })
    ));
    assert!(env.tools.calls().is_empty());
}

// 검증: M2 (아는 도구와 모르는 도구가 섞이면 아는 것만 부른다)
#[tokio::test]
async fn 모르는_도구에는_오류_결과를_주고_아는_것은_부른다() {
    let env = Env::new();
    let reply = format!("{}\n{}", action("teleport"), action("quote"));
    env.ai.script(Role::Risk, vec![Step::Say(reply), say("끝")]);
    let out = env
        .runner()
        .run_turn(&config(Role::Risk), TimeLimits::MARKET, "chat", "봐 줘")
        .await
        .unwrap();
    assert_eq!(out.text, "끝");
    let names: Vec<String> = env.tools.calls().into_iter().map(|(_, c)| c.name).collect();
    assert_eq!(names, ["quote"]);
    let second = &env.ai.requests()[1];
    assert!(
        second
            .messages
            .iter()
            .any(|m| m.content.contains("teleport"))
    );
}

// 검증: M2 (도구 오류는 결과로 돌려주고 턴은 이어진다)
#[tokio::test]
async fn 도구_오류는_결과로_돌려주고_이어간다() {
    let env = Env::new();
    env.ai.script(
        Role::Review,
        vec![Step::Say(action("boom")), say("다른 길로 봤다")],
    );
    let out = env
        .runner()
        .run_turn(&config(Role::Review), TimeLimits::MARKET, "chat", "복기")
        .await
        .unwrap();
    assert_eq!(out.text, "다른 길로 봤다");
    let second = &env.ai.requests()[1];
    assert!(
        second
            .messages
            .iter()
            .any(|m| m.content.contains("도구가 터졌다"))
    );
}

// 검증: E6 (공급자 오류는 담당 응답 실패)
#[tokio::test]
async fn 공급자_오류는_담당_응답_실패다() {
    let env = Env::new();
    env.ai
        .script(Role::Strategy, vec![Step::Fail(AiError::Timeout)]);
    let result = env
        .runner()
        .run_turn(&config(Role::Strategy), TimeLimits::MARKET, "chat", "의견?")
        .await;
    assert!(matches!(
        result,
        Err(Error::RoleUnavailable {
            role: Role::Strategy,
            cause: TurnFailure::Provider(AiError::Timeout)
        })
    ));
}

// 검증: M2 (같은 제목이면 같은 세션을 이어 쓴다)
#[tokio::test]
async fn 같은_제목이면_같은_세션을_이어_쓴다() {
    let env = Env::new();
    env.ai
        .script(Role::Leader, vec![say("첫 답"), say("둘째 답")]);
    let runner = env.runner();
    let leader = config(Role::Leader);
    let first = runner
        .run_turn(&leader, TimeLimits::CHAT, "chat", "첫 질문")
        .await
        .unwrap();
    let second = runner
        .run_turn(&leader, TimeLimits::CHAT, "chat", "둘째 질문")
        .await
        .unwrap();
    assert_eq!(first.session, second.session);
    let request = &env.ai.requests()[1];
    let texts: Vec<&str> = request
        .messages
        .iter()
        .map(|m| m.content.as_str())
        .collect();
    assert!(texts.contains(&"첫 질문"));
    assert!(texts.contains(&"첫 답"));
    assert_eq!(texts.last(), Some(&"둘째 질문"));
    let other = runner
        .run_turn(&leader, TimeLimits::CHAT, "other", "x")
        .await;
    assert!(other.is_err());
    assert_eq!(env.store.sessions_of("leader").unwrap().len(), 2);
}

// 검증: E38 P21 (AI로 보내는 글은 Redactor를 거친다)
#[tokio::test]
async fn 나가는_글은_모두_가린다() {
    let env = Env::new();
    let mut scout = config(Role::Scout);
    scout.soul = format!("너는 시세 담당이다. {SECRET_MARK}");
    env.ai
        .script(Role::Scout, vec![Step::Say(action("quote")), say("끝")]);
    let input = format!("이 키를 써: {SECRET_MARK}");
    env.runner()
        .run_turn(&scout, TimeLimits::MARKET, "chat", &input)
        .await
        .unwrap();
    let requests = env.ai.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        let system = request.system.clone().unwrap_or_default();
        assert!(!system.contains(SECRET_MARK));
        assert!(system.contains(MASK));
        for message in &request.messages {
            assert!(!message.content.contains(SECRET_MARK));
        }
        assert!(request.messages.iter().any(|m| m.content.contains(MASK)));
    }
}
