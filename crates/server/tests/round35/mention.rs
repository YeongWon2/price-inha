//! 33차 1번 · 2번: 팀 방 멘션 글(C18 · E43 · E44 · V58) · 멘션 걸어오기와 방 바꿈 복귀(V59).

use pi_agent::Role;
use price_inha::{
    DemoTurn, KIND_OFFICE_MESSAGE, OFFICE_BAD_INPUT, OFFICE_CHAT_PATH, OFFICE_HINT_NO_MATCH,
    OFFICE_HINT_NO_MENTION, OFFICE_NO_MATCH_TEXT, OFFICE_ROSTER, OFFICE_SUMMON_ARRIVED_PATH,
    OFFICE_SUMMON_VIEW_PATH,
};
use serde_json::{Value, json};

use crate::common::{Bench, events, has_phase, stays_false, summons, team_text, wait_for};

/// 서버가 켜지며 뒤에서 도는 하루 판단(깨우기 엔진의 첫 tick)의 AI 부름이 잦아든 뒤의 부름 수.
/// 그 부름이 "글을 보낸 뒤 늘어난 부름"으로 세어지지 않게, 부름 수가 300ms 동안 그대로일 때까지 기다린다
/// (가장 길게 3초).
async fn settled_prompts(bench: &Bench) -> usize {
    let mut seen = bench.prompts().len();
    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let now = bench.prompts().len();
        if now == seen {
            break;
        }
        seen = now;
    }
    seen
}

/// 그 사용자 글에 붙은 응답 이벤트(`office.chat`) 가운데 그 담당의 것.
fn replies_of(bench: &Bench, source: &str, agent: &str) -> Vec<Value> {
    events(&bench.store(), pi_office::EVENT_CHAT)
        .into_iter()
        .filter(|e| e["sourceMessageId"] == source && e["agent"] == agent)
        .collect()
}

/// 그 사용자 글에 붙은 응답 이벤트를 낸 담당들(처음 나온 순서).
fn repliers(bench: &Bench, source: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for e in events(&bench.store(), pi_office::EVENT_CHAT) {
        if e["sourceMessageId"] != source {
            continue;
        }
        let agent = e["agent"].as_str().unwrap_or_default().to_owned();
        if !seen.contains(&agent) {
            seen.push(agent);
        }
    }
    seen
}

fn finished(bench: &Bench, source: &str, agent: &str) -> bool {
    replies_of(bench, source, agent)
        .iter()
        .any(|e| e["status"] == "complete" || e["status"] == "failed")
}

// 검증: V58 C18 R5-6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 멘션된_담당만_답하고_여럿이면_멘션한_차례대로_답한다() {
    let bench = Bench::start().await;
    bench.script(
        DemoTurn::Chat(Role::Risk),
        "리스크입니다. 손절선은 지킵니다.",
    );
    bench.script(DemoTurn::Chat(Role::Strategy), "전략입니다. 지켜봅니다.");
    let text = "@[리스크] @[전략] 지금 어떻게 보세요?";
    let reply = bench.post(OFFICE_CHAT_PATH, &team_text(text, "m-1")).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["responders"], json!(["risk", "strategy"]));
    let responses = reply.body["responses"].as_array().expect("응답 객체들");
    assert_eq!(responses.len(), 2, "{}", reply.text);
    assert_eq!(responses[0]["agent"], "risk");
    assert_eq!(responses[1]["agent"], "strategy");
    for r in responses {
        assert_eq!(r["status"], "queued", "{r}");
        assert_eq!(r["room"], "team", "{r}");
        assert_eq!(r["sourceMessageId"], "m-1", "{r}");
    }
    assert!(reply.body.get("hint").is_none_or(Value::is_null));

    wait_for("멘션된 두 담당의 답이 끝난다", || {
        finished(&bench, "m-1", "risk") && finished(&bench, "m-1", "strategy")
    })
    .await;
    let last = |agent: &str| replies_of(&bench, "m-1", agent).last().cloned().unwrap();
    assert_eq!(last("risk")["status"], "complete");
    assert_eq!(last("risk")["content"], "리스크입니다. 손절선은 지킵니다.");
    assert_eq!(last("strategy")["status"], "complete");
    assert_eq!(last("strategy")["content"], "전략입니다. 지켜봅니다.");
    assert_eq!(
        repliers(&bench, "m-1"),
        vec!["risk", "strategy"],
        "멘션하지 않은 담당은 답하지 않고, 응답은 멘션한 차례로 생긴다"
    );
    assert!(
        bench.prompts_with("지금 어떻게 보세요?") >= 2,
        "글이 멘션된 담당의 AI 턴으로 간다"
    );
    bench.close().await;
}

// 검증: V58 C18 E44 R5-6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 멘션이_없는_팀_방_글은_아무도_답하지_않고_no_mention이며_글은_남는다() {
    let bench = Bench::start().await;
    let before = settled_prompts(&bench).await;
    // 괄호 없는 `@이름`은 멘션이 아니다(원본).
    let text = "@리스크 오늘 시장 어때요?";
    let reply = bench.post(OFFICE_CHAT_PATH, &team_text(text, "n-1")).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["responders"], json!([]));
    assert_eq!(reply.body["hint"], OFFICE_HINT_NO_MENTION);
    assert_eq!(
        reply.body["responses"].as_array().map_or(0, Vec::len),
        0,
        "{}",
        reply.text
    );
    let saved = events(&bench.store(), KIND_OFFICE_MESSAGE);
    assert!(
        saved
            .iter()
            .any(|e| e["sourceMessageId"] == "n-1" && e["text"] == text && e["room"] == "team"),
        "글은 팀 방 이벤트로 남는다: {saved:?}"
    );
    stays_false(
        "아무 담당도 답하지 않고 AI를 부르지 않는다",
        || !repliers(&bench, "n-1").is_empty() || bench.prompts().len() > before,
    )
    .await;
    bench.close().await;
}

// 검증: V58 C18 E43 R5-6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 명단과_맞지_않는_이름이면_no_match이고_보낸_사람에게만_알린다() {
    let bench = Bench::start().await;
    let before = settled_prompts(&bench).await;
    let message_events = events(&bench.store(), KIND_OFFICE_MESSAGE).len();
    // 부분 일치("리스")와 없는 이름은 멘션이 맞지 않는다(정확 일치만).
    for (i, text) in ["@[없는사람] 안녕하세요", "@[리스] 안녕하세요"]
        .iter()
        .enumerate()
    {
        let source = format!("x-{i}");
        let reply = bench
            .post(OFFICE_CHAT_PATH, &team_text(text, &source))
            .await;
        assert_eq!(reply.status, 200, "{text}: {}", reply.text);
        assert_eq!(reply.body["responders"], json!([]), "{text}");
        assert_eq!(reply.body["hint"], OFFICE_HINT_NO_MATCH, "{text}");
        bench.pass(3);
    }
    assert_eq!(
        OFFICE_NO_MATCH_TEXT,
        "지목한 상대가 이 방에 없어 아무도 답하지 않았습니다."
    );
    stays_false(
        "아무 담당도 답하지 않고 AI를 부르지 않는다",
        || {
            !repliers(&bench, "x-0").is_empty()
                || !repliers(&bench, "x-1").is_empty()
                || bench.prompts().len() > before
        },
    )
    .await;
    let hinted = events(&bench.store(), KIND_OFFICE_MESSAGE)
        .into_iter()
        .skip(message_events)
        .any(|e| e.to_string().contains(OFFICE_NO_MATCH_TEXT));
    assert!(!hinted, "안내는 이벤트로 다른 화면에 퍼지지 않는다");
    bench.close().await;
}

// 검증: V58 C18 C5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 담당을_주면_지금처럼_그_담당_하나가_답한다() {
    let bench = Bench::start().await;
    bench.script(
        DemoTurn::Chat(Role::Leader),
        "팀장입니다. 오늘은 지켜봅니다.",
    );
    // 글에 다른 담당 멘션이 있어도 `agent`를 주면 그 담당 하나다.
    let body = json!({
        "room": "team", "agent": "leader", "text": "@[리스크] 오늘 계획은?", "sourceMessageId": "a-1"
    });
    let reply = bench.post(OFFICE_CHAT_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["agent"], "leader", "응답 객체 모양 그대로");
    assert_eq!(reply.body["status"], "queued");
    assert_eq!(reply.body["room"], "team");
    assert!(reply.body.get("responders").is_none(), "{}", reply.text);
    wait_for("팀장의 답이 끝난다", || {
        finished(&bench, "a-1", "leader")
    })
    .await;
    assert_eq!(repliers(&bench, "a-1"), vec!["leader"]);
    // 1:1 방은 담당이 있어야 한다.
    bench.pass(3);
    let reply = bench
        .post(
            OFFICE_CHAT_PATH,
            &json!({"room": "direct", "text": "안녕", "sourceMessageId": "a-2"}),
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.text);
    assert_eq!(reply.body["error"], OFFICE_BAD_INPUT);
    bench.close().await;
}

// 검증: V58 C18
#[test]
fn 서버_명단의_이름은_화면_지도의_담당_이름과_같다() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/web/public/office/office-map.json"
    );
    let map: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let ids: Vec<&str> = pi_office::Agent::ALL.iter().map(|a| a.id()).collect();
    let roster_ids: Vec<&str> = OFFICE_ROSTER.iter().map(|(id, _)| *id).collect();
    assert_eq!(roster_ids, ids, "명단 순서는 담당 순서와 같다");
    for (id, name) in OFFICE_ROSTER {
        assert_eq!(map["agents"][id]["name"], name, "{id}");
    }
}

// 검증: V59 C18 C20 R5-7
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 멘션된_담당은_답을_시작할_때_곁으로_오고_팀_방을_보는_동안_남다가_방을_바꾸면_돌아간다() {
    let bench = Bench::start().await;
    bench.script(DemoTurn::Chat(Role::Risk), "리스크입니다.");
    let reply = bench
        .post(OFFICE_CHAT_PATH, &team_text("@[리스크] 잠깐요", "s-1"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for("멘션된 담당이 오는 중이 된다", || {
        has_phase(&bench.store(), "risk", "called")
    })
    .await;
    let called = summons(&bench.store(), "risk")
        .into_iter()
        .find(|e| e["phase"] == "called")
        .unwrap();
    assert_eq!(called["reason"], "mention", "{called}");
    assert!(called["at"].as_i64().is_some(), "{called}");
    // 답 턴이 시작되는 순간에 온다: 그 응답이 생각 중이 된 때에는 이미 오는 중이다.
    wait_for("답이 끝난다", || finished(&bench, "s-1", "risk")).await;
    assert!(
        replies_of(&bench, "s-1", "risk")
            .iter()
            .any(|e| e["status"] == "thinking")
    );
    for agent in ["news", "strategy", "review", "trader", "leader"] {
        assert!(
            summons(&bench.store(), agent).is_empty(),
            "멘션하지 않은 담당({agent})은 오지 않는다"
        );
    }

    let reply = bench
        .post(OFFICE_SUMMON_ARRIVED_PATH, &json!({"agent": "risk"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["phase"], "waiting");
    // 팀 방을 보는 동안은 답한 뒤 10초가 지나도 곁에 남는다.
    bench.pass(30);
    let reply = bench
        .post(OFFICE_SUMMON_VIEW_PATH, &json!({"room": "team"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    stays_false("팀 방을 보는 동안 돌아가지 않는다", || {
        has_phase(&bench.store(), "risk", "returning")
    })
    .await;
    // 다른 방으로 바꾸면 바로 돌아간다.
    let reply = bench
        .post(OFFICE_SUMMON_VIEW_PATH, &json!({"room": "direct"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for("방을 바꾸면 돌아가는 중이 된다", || {
        has_phase(&bench.store(), "risk", "returning")
    })
    .await;
    let back = summons(&bench.store(), "risk")
        .into_iter()
        .find(|e| e["phase"] == "returning")
        .unwrap();
    assert_eq!(back["reason"], "mention", "{back}");
    bench.close().await;
}

// 검증: V59 C20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 보는_방_알림은_모르는_방이면_400이다() {
    let bench = Bench::start().await;
    for body in [json!({"room": "hall"}), json!({}), json!({"room": 3})] {
        let reply = bench.post(OFFICE_SUMMON_VIEW_PATH, &body).await;
        assert_eq!(reply.status, 400, "{body}: {}", reply.text);
        assert_eq!(reply.body["error"], OFFICE_BAD_INPUT, "{body}");
    }
    bench.close().await;
}
