//! 사무실 잇기(T20 · T21): 자리 · 출근 · 논의 모습(V13 · V15), 접속 스냅샷(C2 · E3), 채팅(C5 · V14).

use pi_agent::Role;
use price_inha::{
    DAILY_SUMMARY_PATH, DemoTurn, KIND_OFFICE_MESSAGE, NOTICES_PATH, OFFICE_BAD_INPUT,
    OFFICE_CHAT_PATH, OFFICE_PATH, OFFICE_SOURCE_ID_MAX, OFFICE_TEXT_MAX, OFFICE_TOO_FAST,
    OFFICE_TOO_LONG, PUSH_KEY_PATH,
};
use serde_json::{Value, json};

use crate::common::{
    Bench, HYNIX, events, get, get_anonymous, kst, last_seq, login, monday, origin_of, post, send,
    wait_for,
};

const AGENTS: [&str; 6] = ["news", "strategy", "risk", "review", "trader", "leader"];

/// 그 담당의 `office.agent` 이벤트들의 자리(번호 순서).
fn places(bench: &Bench, agent: &str) -> Vec<String> {
    events(&bench.store(), pi_office::EVENT_AGENT)
        .into_iter()
        .filter(|e| e["agent"] == agent)
        .map(|e| e["place"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn chat(room: &str, agent: &str, text: &str, source: &str) -> Value {
    json!({"room": room, "agent": agent, "text": text, "sourceMessageId": source})
}

// 검증: T20 V15 C2 E37
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 쉬는_중에는_모두_침대에_있고_운용이_시작되면_자기_자리로_간다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    let snapshot = get(bench.addr(), OFFICE_PATH).await;
    assert_eq!(snapshot.status, 200, "{}", snapshot.text());
    let agents = snapshot.body["agents"].as_array().expect("담당 모습들");
    assert_eq!(agents.len(), 6);
    for (shown, id) in agents.iter().zip(AGENTS) {
        assert_eq!(shown["agent"], id);
        assert_eq!(shown["place"], "bed", "쉬는 중에는 침대: {shown}");
        assert_eq!(shown["work"], "idle");
    }
    assert!(
        snapshot.body["pending"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert!(snapshot.body["last_seq"].is_u64());

    bench.ready().await;
    wait_for("모두 자기 자리로 간다(이벤트)", || {
        AGENTS
            .iter()
            .all(|a| places(&bench, a).last().is_some_and(|p| p == "desk"))
    })
    .await;
    let snapshot = get(bench.addr(), OFFICE_PATH).await;
    for shown in snapshot.body["agents"].as_array().unwrap() {
        assert_eq!(shown["place"], "desk", "운용 중에는 자기 자리: {shown}");
    }
    let seq = snapshot.body["last_seq"].as_u64().unwrap();
    let store = bench.store();
    let moved = store
        .read_after(0, 100_000)
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == pi_office::EVENT_AGENT)
        .map(|e| e.seq)
        .max()
        .unwrap();
    assert!(
        seq >= moved && seq <= last_seq(&store),
        "스냅샷 번호 {seq}는 자리 이벤트 {moved} 뒤이고 저장된 마지막 번호를 넘지 않는다(E3)"
    );
    bench.close().await;
}

// 검증: T20 V13 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 논의할_때는_논의_탁자에_모이고_끝나면_자리로_돌아간다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    bench.ready().await;
    // 사무실은 팀 상태를 100ms마다 읽어 맞춘다. 운용 중이 사무실에 보인 뒤에 논의한다
    // (느린 기계에서 논의가 그 사이에 끝나면 모이는 모습이 나오지 않는다).
    wait_for("모두 자기 자리로 간다(이벤트)", || {
        ["news", "strategy", "risk"]
            .iter()
            .all(|a| places(&bench, a).last().is_some_and(|p| p == "desk"))
    })
    .await;
    bench.discuss_buy(HYNIX).await;
    for agent in ["news", "strategy", "risk"] {
        let seen = places(&bench, agent);
        assert!(
            seen.iter().any(|p| p == "meeting"),
            "{agent}는 논의 탁자에 모였다: {seen:?}"
        );
        assert_eq!(
            seen.last().map(String::as_str),
            Some("desk"),
            "{agent}는 논의가 끝나면 자리로 돌아간다: {seen:?}"
        );
    }
    bench.close().await;
}

// 검증: T21 C5 V14 E6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 사무실_채팅에서_담당에게_물으면_그_담당이_답한다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    let answer = "리스크 담당입니다. 손절선 아래로는 버티지 않습니다.";
    bench.script(DemoTurn::Chat(Role::Risk), answer);
    let question = "지금 가장 큰 위험은 무엇인가요?";
    let reply = post(
        bench.addr(),
        OFFICE_CHAT_PATH,
        &chat("direct", "risk", question, "m-1"),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(reply.body["status"], "queued");
    assert_eq!(reply.body["agent"], "risk");
    assert_eq!(reply.body["room"], "direct");
    assert_eq!(reply.body["sourceMessageId"], "m-1");
    let id = reply.body["requestId"]
        .as_str()
        .expect("응답 번호")
        .to_string();

    let mine = |bench: &Bench| -> Vec<Value> {
        events(&bench.store(), pi_office::EVENT_CHAT)
            .into_iter()
            .filter(|e| e["requestId"] == id.as_str())
            .collect()
    };
    wait_for("답이 끝난다", || {
        mine(&bench)
            .iter()
            .any(|e| e["status"] == "complete" || e["status"] == "failed")
    })
    .await;
    let seen = mine(&bench);
    let last = seen.last().unwrap();
    assert_eq!(last["status"], "complete", "{seen:?}");
    assert_eq!(last["content"], answer, "그 담당이 답한다");
    assert_eq!(last["agent"], "risk");
    assert!(seen.iter().any(|e| e["status"] == "thinking"), "{seen:?}");
    assert!(
        bench.ai().prompts().iter().any(|p| p.contains(question)),
        "물은 글이 그 담당의 AI 턴으로 간다"
    );
    let snapshot = get(bench.addr(), OFFICE_PATH).await;
    assert!(
        snapshot.body["pending"].as_array().unwrap().is_empty(),
        "끝난 응답은 대기에 없다"
    );
    bench.close().await;
}

// 검증: T21 C5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 사무실_글은_길이와_번호_길이와_보내는_간격을_지킨다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    let addr = bench.addr();
    let long = "가".repeat(OFFICE_TEXT_MAX + 1);
    let reply = post(addr, OFFICE_CHAT_PATH, &chat("direct", "risk", &long, "a")).await;
    assert_eq!(reply.status, 400);
    assert_eq!(reply.body["error"], OFFICE_TOO_LONG);
    let source = "x".repeat(OFFICE_SOURCE_ID_MAX + 1);
    let reply = post(
        addr,
        OFFICE_CHAT_PATH,
        &chat("direct", "risk", "안녕", &source),
    )
    .await;
    assert_eq!(reply.status, 400);
    assert_eq!(reply.body["error"], OFFICE_TOO_LONG);
    for bad in [
        chat("direct", "risk", "안녕", ""),
        chat("direct", "nobody", "안녕", "b"),
        chat("hall", "risk", "안녕", "c"),
        chat("direct", "risk", "   ", "d"),
    ] {
        let reply = post(addr, OFFICE_CHAT_PATH, &bad).await;
        assert_eq!(reply.status, 400, "{bad}");
        assert_eq!(reply.body["error"], OFFICE_BAD_INPUT, "{bad}");
    }
    // 거부한 글은 간격에 세지 않는다: 상한 그대로의 글은 받는다.
    let full = "나".repeat(OFFICE_TEXT_MAX);
    let ok = post(addr, OFFICE_CHAT_PATH, &chat("direct", "risk", &full, "e")).await;
    assert_eq!(ok.status, 200, "{}", ok.text());
    let fast = post(addr, OFFICE_CHAT_PATH, &chat("direct", "risk", "또", "f")).await;
    assert_eq!(fast.status, 429, "같은 기기가 2초 안에 또 보낸다");
    assert_eq!(fast.body["error"], OFFICE_TOO_FAST);
    bench.set(bench.now() + chrono::Duration::seconds(2));
    let later = post(addr, OFFICE_CHAT_PATH, &chat("direct", "risk", "또", "g")).await;
    assert_eq!(later.status, 200, "2초가 지나면 받는다: {}", later.text());
    bench.close().await;
}

// 검증: T21 C5 E37
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 팀_방의_사용자_글도_이벤트로_남는다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    let body = chat("team", "leader", "오늘 계획은 무엇인가요?", "t-1");
    let reply = post(bench.addr(), OFFICE_CHAT_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(reply.body["room"], "team");
    let saved = events(&bench.store(), KIND_OFFICE_MESSAGE);
    assert_eq!(saved.len(), 1, "{saved:?}");
    assert_eq!(saved[0]["room"], "team");
    assert_eq!(saved[0]["agent"], "leader");
    assert_eq!(saved[0]["text"], "오늘 계획은 무엇인가요?");
    assert_eq!(saved[0]["sourceMessageId"], "t-1");
    bench.close().await;
}

// 검증: C15 M6 D11
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 새_경로는_세션과_같은_출처가_있어야_열린다() {
    let bench = Bench::start(kst(monday(), 9, 5)).await;
    let addr = bench.addr();
    for path in [
        OFFICE_PATH,
        NOTICES_PATH,
        PUSH_KEY_PATH,
        DAILY_SUMMARY_PATH,
        "/api/approvals/abc",
    ] {
        let reply = get_anonymous(addr, path).await;
        assert_eq!(reply.status, 401, "{path}는 세션 없이 열리지 않는다");
    }
    // 세션은 있지만 Origin이 없는 상태 바꾸기는 거부한다.
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .post(format!("{}{OFFICE_CHAT_PATH}", origin_of(addr)))
        .header("Cookie", cookie)
        .header("Content-Type", "application/json")
        .body(chat("direct", "risk", "안녕", "o-1").to_string());
    let reply = send(request).await;
    assert!(
        (400..500).contains(&reply.status),
        "Origin 없는 요청은 거부: {}",
        reply.status
    );
    assert!(events(&bench.store(), pi_office::EVENT_CHAT).is_empty());
    bench.close().await;
}
