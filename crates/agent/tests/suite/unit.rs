//! 판단 금액은 통화의 최소 단위다(5차): 국내는 원, 미국은 센트. 팀장 입력에 단위 안내가 들어간다.

use pi_agent::Role;

use crate::common::{Env, Step, model, request, say, score_reply, trader_buy};

fn script_buy(env: &Env, stop: i64) {
    env.ai.script(
        Role::Scout,
        vec![say("공시 요약"), Step::Say(score_reply(70, "a"))],
    );
    env.ai
        .script(Role::Strategy, vec![Step::Say(score_reply(80, "b"))]);
    env.ai
        .script(Role::Risk, vec![Step::Say(score_reply(60, "c"))]);
    let reply = format!(
        "{{\"decision\": \"buy\", \"hypothesis\": \"실적\", \"trigger\": \"공시\", \
         \"entry\": 70000, \"stop\": {stop}, \"target\": 80000, \"expected_value\": 3000}}"
    );
    env.ai.script(Role::Leader, vec![say(&reply)]);
    env.ai
        .script(Role::Trader, vec![Step::Forever(trader_buy("사자"))]);
}

// 검증: V4 (팀장에게 가는 입력에 시장별 금액 단위가 적힌다)
#[tokio::test]
async fn leader_input_names_the_amount_unit_of_each_market() {
    let env = Env::new();
    let id = env.trade();
    script_buy(&env, 65000);
    env.team().discuss(request(id)).await.unwrap();

    let leader = model(Role::Leader);
    let text: String = env
        .ai
        .requests()
        .iter()
        .filter(|r| r.model == leader)
        .flat_map(|r| r.messages.iter().map(|m| m.content.clone()))
        .collect();

    assert!(text.contains("최소 단위"));
    assert!(text.contains("원"));
    assert!(text.contains("센트"));
}
