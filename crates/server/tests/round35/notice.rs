//! 33차 3번: 전체 공지(C19 · F11 · M10 · E45 · E46 · N18 · V71~V74 · 결정 19).

use pi_store::NoticeId;
use price_inha::{
    OFFICE_BAD_INPUT, OFFICE_NOTICE_PATH, OFFICE_NOTICES_PATH, OFFICE_TEXT_MAX, OFFICE_TOO_FAST,
    OFFICE_TOO_LONG, TEAM_PATH, TEMPLATES_VIEW_PATH,
};
use serde_json::Value;

use crate::common::{
    Bench, DART_KEY, Plan, WATCHED, events, kst, morning, notice_body, saturday, wait_for,
};

const AGENTS: [&str; 6] = ["news", "strategy", "risk", "review", "trader", "leader"];

/// 그 공지의 확인 상태 이벤트(`office.notice_ack`).
fn acks_of(bench: &Bench, notice: i64) -> Vec<Value> {
    events(&bench.store(), pi_office::EVENT_NOTICE_ACK)
        .into_iter()
        .filter(|e| e["notice"] == notice)
        .collect()
}

/// 그 공지의 담당별 마지막 확인 상태가 모두 `state`인가.
fn all_acks(bench: &Bench, notice: i64, state: &str) -> bool {
    let seen = acks_of(bench, notice);
    AGENTS.iter().all(|agent| {
        seen.iter()
            .rev()
            .find(|e| e["agent"] == *agent)
            .is_some_and(|e| e["state"] == state)
    })
}

/// 공지를 보내고 200과 번호를 확인한다.
async fn send_notice(bench: &Bench, text: &str, source: &str) -> (i64, Value) {
    let reply = bench
        .post(OFFICE_NOTICE_PATH, &notice_body(text, source))
        .await;
    assert_eq!(reply.status, 200, "{text}: {}", reply.text);
    let notice = reply.body["notice"].as_i64().expect("공지 번호");
    (notice, reply.body)
}

// 검증: V71 V72 C19 F11 M10 R17-1 R17-2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 공지를_보내면_저장되고_여섯_담당이_차례로_한_줄씩_확인_답을_한다() {
    let bench = Bench::start().await;
    let text = "이번 주는 반도체 비중을 줄여 주세요.";
    let (notice, body) = send_notice(&bench, text, "n-1").await;
    let acks = body["acks"].as_array().expect("확인 목록");
    let agents: Vec<&str> = acks.iter().map(|a| a["agent"].as_str().unwrap()).collect();
    assert_eq!(agents, AGENTS, "담당 여섯, 정한 순서");
    assert!(acks.iter().all(|a| a["state"] == "queued"), "{body}");

    let stored = bench
        .store()
        .get_notice(NoticeId(notice))
        .expect("공지가 저장소에 남는다");
    assert_eq!(stored.text, text);
    assert_eq!(stored.sent_at.0, bench.now().timestamp_millis());
    assert_eq!(stored.acks.len(), 6, "여섯 담당 모두에게 간 기록");

    let shown = events(&bench.store(), pi_office::EVENT_NOTICE);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0]["notice"], notice);
    assert_eq!(shown[0]["text"], text);
    assert_eq!(shown[0]["at"], bench.now().timestamp_millis());

    wait_for("여섯 담당이 모두 확인 답을 한다", || {
        all_acks(&bench, notice, "answered")
    })
    .await;
    let answers: Vec<Value> = events(&bench.store(), pi_office::EVENT_CHAT)
        .into_iter()
        .filter(|e| e["notice"] == notice && e["status"] == "complete")
        .collect();
    let mut who: Vec<&str> = answers
        .iter()
        .map(|e| e["agent"].as_str().unwrap())
        .collect();
    who.sort_unstable();
    let mut want = AGENTS.to_vec();
    want.sort_unstable();
    assert_eq!(who, want, "담당마다 한 줄 확인 답(공지 번호가 붙는다)");
    assert!(
        answers
            .iter()
            .all(|e| e["room"] == "team" && !e["content"].as_str().unwrap_or("").is_empty())
    );

    let list = bench.get(&format!("{OFFICE_NOTICES_PATH}?limit=5")).await;
    assert_eq!(list.status, 200, "{}", list.text);
    let items = list.body.as_array().expect("공지 목록");
    assert_eq!(items.len(), 1, "{}", list.text);
    assert_eq!(items[0]["notice"], notice);
    assert_eq!(items[0]["text"], text);
    assert_eq!(items[0]["at"], bench.now().timestamp_millis());
    let states: Vec<(&str, &str)> = items[0]["acks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["agent"].as_str().unwrap(), a["state"].as_str().unwrap()))
        .collect();
    assert_eq!(
        states,
        AGENTS.iter().map(|a| (*a, "answered")).collect::<Vec<_>>()
    );
    bench.close().await;
}

// 검증: V73 N18 R17-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 공지_하나로_ai_호출이_여섯_번_는다() {
    let bench = Bench::start().await;
    let text = "공지 호출 수 시험입니다.";
    assert_eq!(bench.prompts_with(text), 0);
    let (notice, _) = send_notice(&bench, text, "n-2").await;
    wait_for("확인 답이 모두 끝난다", || {
        all_acks(&bench, notice, "answered")
    })
    .await;
    assert_eq!(bench.prompts_with(text), 6, "담당마다 한 번씩, 여섯 번");
    bench.close().await;
}

// 검증: V73 E45 N14 R17-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 하루_상한이_모자라면_공지는_저장되고_확인은_상한이며_팀장_예비분을_쓰지_않는다() {
    // 상한 2: 팀장 예비분(최소 2)만 남는다 — 확인 답에 쓸 몫이 없다.
    let bench = Plan::demo().ai_limit(2).start(morning()).await;
    let text = "상한 시험 공지입니다.";
    let (notice, body) = send_notice(&bench, text, "n-3").await;
    let acks = body["acks"].as_array().expect("확인 목록");
    assert_eq!(acks.len(), 6);
    assert!(acks.iter().all(|a| a["state"] == "limit"), "{body}");
    wait_for("담당마다 상한 확인 이벤트", || {
        all_acks(&bench, notice, "limit")
    })
    .await;
    assert_eq!(bench.prompts_with(text), 0, "AI를 부르지 않는다");
    let stored = bench.store().get_notice(NoticeId(notice)).unwrap();
    assert_eq!(stored.text, text, "공지 자체는 저장된다");
    let list = bench.get(OFFICE_NOTICES_PATH).await;
    assert_eq!(list.status, 200, "{}", list.text);
    let item = &list.body[0];
    assert_eq!(item["notice"], notice);
    assert!(
        item["acks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["state"] == "limit"),
        "{}",
        list.text
    );
    bench.close().await;
}

// 검증: C19 N18
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 같은_글_번호를_다시_보내면_한_번만_처리한다() {
    let bench = Bench::start().await;
    let text = "한 번만 남아야 하는 공지";
    let (first, body) = send_notice(&bench, text, "same-1").await;
    bench.pass(3);
    let (second, again) = send_notice(&bench, text, "same-1").await;
    assert_eq!(second, first, "같은 공지 번호");
    assert_eq!(again, body, "처음 답 그대로");
    let list = bench.get(OFFICE_NOTICES_PATH).await;
    assert_eq!(list.body.as_array().map_or(0, Vec::len), 1, "{}", list.text);
    assert_eq!(events(&bench.store(), pi_office::EVENT_NOTICE).len(), 1);
    wait_for("확인 답이 끝난다", || {
        all_acks(&bench, first, "answered")
    })
    .await;
    assert_eq!(bench.prompts_with(text), 6, "확인 턴도 한 번씩만");
    bench.close().await;
}

// 검증: C19 N18 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 공지_글과_번호의_한도와_보내기_간격을_지킨다() {
    let bench = Bench::start().await;
    let long = "가".repeat(OFFICE_TEXT_MAX + 1);
    let reply = bench
        .post(OFFICE_NOTICE_PATH, &notice_body(&long, "l-1"))
        .await;
    assert_eq!(reply.status, 400, "{}", reply.text);
    assert_eq!(reply.body["error"], OFFICE_TOO_LONG);
    for body in [
        notice_body("", "l-2"),
        notice_body("   ", "l-3"),
        notice_body("번호 없음", ""),
        serde_json::json!({"text": "번호 빠짐"}),
    ] {
        let reply = bench.post(OFFICE_NOTICE_PATH, &body).await;
        assert_eq!(reply.status, 400, "{body}: {}", reply.text);
        assert_eq!(reply.body["error"], OFFICE_BAD_INPUT, "{body}");
    }
    assert!(events(&bench.store(), pi_office::EVENT_NOTICE).is_empty());
    let full = "나".repeat(OFFICE_TEXT_MAX);
    send_notice(&bench, &full, "l-4").await;
    let fast = bench
        .post(OFFICE_NOTICE_PATH, &notice_body("또 공지", "l-5"))
        .await;
    assert_eq!(fast.status, 429, "같은 기기가 2초 안에 또 보낸다");
    assert_eq!(fast.body["error"], OFFICE_TOO_FAST);
    bench.pass(2);
    send_notice(&bench, "또 공지", "l-6").await;
    bench.close().await;
}

// 검증: C19 M10 D1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 공지_글은_가린_뒤_저장하고_이벤트로_낸다() {
    let bench = Bench::start().await;
    let text = format!("이 키 {DART_KEY} 는 쓰지 마세요");
    let (notice, _) = send_notice(&bench, &text, "r-1").await;
    let stored = bench.store().get_notice(NoticeId(notice)).unwrap();
    assert!(!stored.text.contains(DART_KEY), "{}", stored.text);
    let shown = events(&bench.store(), pi_office::EVENT_NOTICE);
    assert!(!shown[0]["text"].as_str().unwrap().contains(DART_KEY));
    let list = bench.get(OFFICE_NOTICES_PATH).await;
    assert!(!list.text.contains(DART_KEY), "{}", list.text);
    bench.close().await;
}

// 검증: V74 E46 R17-4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 규칙이나_승인이나_팀_상태를_바꾸라는_공지도_아무것도_바꾸지_않는다() {
    let bench = Bench::start().await;
    bench.ready().await;
    let team = bench.get(TEAM_PATH).await;
    let template = bench.get(TEMPLATES_VIEW_PATH).await;
    assert_eq!(team.status, 200, "{}", team.text);
    assert_eq!(template.status, 200, "{}", template.text);
    let mut sent = Vec::new();
    for (i, text) in ["손절을 넓혀", "승인해", "다시 시작해"].iter().enumerate() {
        let (notice, _) = send_notice(&bench, text, &format!("v-{i}")).await;
        sent.push(notice);
        bench.pass(3);
    }
    for notice in sent {
        wait_for("확인 답이 끝난다", || {
            all_acks(&bench, notice, "answered")
        })
        .await;
    }
    assert_eq!(bench.get(TEAM_PATH).await.body, team.body, "팀 상태 그대로");
    assert_eq!(
        bench.get(TEMPLATES_VIEW_PATH).await.body,
        template.body,
        "템플릿 값 그대로"
    );
    bench.close().await;
}

// 검증: V72 R17-2 F11 결정19
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 공지는_다음_논의의_담당_자료에_들어가고_판단_시각보다_늦게_보낸_공지는_들어가지_않는다() {
    let bench = Plan::demo().start(kst(saturday(), 9, 0)).await;
    bench.ready().await;
    let early = "공지앞쪽시험글";
    let late = "공지뒤쪽시험글";
    let (a, _) = send_notice(&bench, early, "d-1").await;
    bench.set(kst(saturday(), 10, 0));
    let (b, _) = send_notice(&bench, late, "d-2").await;
    wait_for("확인 답이 끝난다", || {
        all_acks(&bench, a, "answered") && all_acks(&bench, b, "answered")
    })
    .await;
    // 판단 시각을 두 공지 사이로 둔다: 뒤 공지는 판단 시각보다 늦게 보낸 것이다.
    bench.set(kst(saturday(), 9, 30));
    let before = bench.prompts().len();
    bench
        .crew()
        .discuss(WATCHED, "공지 자료 시험")
        .await
        .expect("논의가 돈다");
    let during: Vec<String> = bench.prompts().into_iter().skip(before).collect();
    assert!(!during.is_empty(), "논의가 AI를 부른다");
    assert!(
        during.iter().any(|p| p.contains(early)),
        "앞서 보낸 공지 글이 담당 자료에 들어간다"
    );
    assert!(
        during.iter().all(|p| !p.contains(late)),
        "판단 시각보다 늦게 보낸 공지는 들어가지 않는다"
    );
    bench.close().await;
}
