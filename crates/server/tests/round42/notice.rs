//! 41차 4번: 메신저에서 공지(결정 26 · 33차 C19 · V59 · V74) — `/notice <글>`은 사무실 전체 공지와 같은 길(저장 · 담당 여섯의
//! 확인 답) · 확인 답들은 메신저로 메시지 하나에 묶여 옴 · 빈 글은 안내(공지를 만들지 않음) · 도움말에 `/notice`.

use std::time::{Duration, Instant};

use price_inha::{
    MESSENGER_ASK_HINT, MESSENGER_HELP, MESSENGER_NOTICE_ACKS, MESSENGER_NOTICE_EMPTY,
    OFFICE_NOTICES_PATH, OFFICE_ROSTER,
};
use serde_json::{Value, json};

use crate::common::{Bench, FakeTelegram, QUIET, STEP, get, said, sent_with};

/// 연결된 사람.
const OWNER: i64 = 87_300_001;

/// 지난 공지들(최근 것 먼저).
async fn notices(bench: &Bench) -> Vec<Value> {
    let reply = get(bench.addr(), OFFICE_NOTICES_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.body.as_array().cloned().unwrap_or_default()
}

// 검증: V59 C19 F11 M10 V74
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 공지를_보내면_저장되고_여섯_확인_답이_메신저로_한_메시지에_묶여_온다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    bench.ai().script_chat("strategy", "전략 확인했습니다.");
    let text = "오늘은 보수적으로 갑니다";
    tg.push_dm(OWNER, &format!("/notice {text}"));

    let message = tg
        .wait_sent(OWNER, "공지 확인 답 묶음", |t| {
            t.contains(MESSENGER_NOTICE_ACKS)
        })
        .await;
    for (_, name) in OFFICE_ROSTER {
        assert!(
            message.contains(&format!("{name}: ")),
            "담당마다 한 줄({name}): {message}"
        );
    }
    assert!(
        message.contains(&said("전략", "전략 확인했습니다.")),
        "{message}"
    );

    let deadline = Instant::now() + STEP;
    loop {
        let saved = notices(&bench).await;
        let done = saved.first().is_some_and(|n| {
            n["text"] == json!(text)
                && n["acks"].as_array().is_some_and(|acks| {
                    acks.len() == 6 && acks.iter().all(|a| a["state"] == "answered")
                })
        });
        if done {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "공지가 저장되고 여섯 확인이 답함이 된다: {saved:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        sent_with(&tg, OWNER, MESSENGER_NOTICE_ACKS).await,
        1,
        "확인 답들은 메시지 하나로 온다"
    );
    bench.close().await;
}

// 검증: V59 C19 E4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 빈_공지는_안내하고_공지를_만들지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    tg.push_dm(OWNER, "/notice");
    tg.wait_sent(OWNER, "빈 공지 안내", |t| {
        t.contains(MESSENGER_NOTICE_EMPTY)
    })
    .await;
    bench.pass(3);
    tg.push_dm(OWNER, "/notice     ");
    tokio::time::sleep(QUIET).await;
    assert_eq!(sent_with(&tg, OWNER, MESSENGER_NOTICE_EMPTY).await, 2);
    assert!(notices(&bench).await.is_empty(), "공지를 만들지 않는다");
    bench.close().await;
}

// 검증: V90 V59
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 도움말에_공지와_묻는_법이_든다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    assert!(MESSENGER_HELP.contains("/notice"), "{MESSENGER_HELP}");
    assert!(
        MESSENGER_HELP.contains(MESSENGER_ASK_HINT),
        "{MESSENGER_HELP}"
    );
    tg.push_dm(OWNER, "/help");
    let help = tg
        .wait_sent(OWNER, "도움말", |t| t.contains(MESSENGER_HELP))
        .await;
    for command in [
        "/status",
        "/approvals",
        "/reject",
        "/stop",
        "/notice",
        "/help",
    ] {
        assert!(help.contains(command), "도움말에 {command}: {help}");
    }
    bench.close().await;
}
