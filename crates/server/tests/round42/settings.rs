//! 41차 2번: 알림 설정의 "팀 논의" 종류(결정 26 · V91 · V87 · C15) — 응답 맨 끝에 폰 칸 없이(`push` null) · 잠기지 않음 ·
//! 기본 켜짐 · 바꾼 값이 답과 읽기 · 다시 켜도 남음 · 바꾸기 몸통에 그 줄이 없으면 그대로 · 폰 값은 보지 않음 · 참/거짓이
//! 아니면 400.

use pi_notify::NoticeKind;
use price_inha::{
    NOTIFY_KIND_LABELS, NOTIFY_SETTINGS_PATH, NOTIFY_TEAM_TALK_KIND, NOTIFY_TEAM_TALK_LABEL,
};
use serde_json::{Value, json};

use crate::common::{Bench, FakeTelegram, Reply, get, post};

/// 바꾸기 몸통: 여섯 `NoticeKind`는 모두 켜고 `off`(종류, 폰, 메신저)만 그 값으로, "팀 논의" 줄은 `talk`(없으면 싣지
/// 않음), 조용한 시간은 `quiet`(켜기, 시작, 끝).
pub fn settings_body(
    off: &[(&str, bool, bool)],
    talk: Option<bool>,
    quiet: (bool, &str, &str),
) -> Value {
    let mut kinds: Vec<Value> = NOTIFY_KIND_LABELS
        .iter()
        .map(|(kind, _)| {
            let (push, messenger) = off
                .iter()
                .find(|(name, _, _)| name == kind)
                .map_or((true, true), |(_, push, messenger)| (*push, *messenger));
            json!({"kind": kind, "push": push, "messenger": messenger})
        })
        .collect();
    if let Some(messenger) = talk {
        kinds.push(json!({"kind": NOTIFY_TEAM_TALK_KIND, "push": null, "messenger": messenger}));
    }
    json!({
        "kinds": kinds,
        "quiet": {"enabled": quiet.0, "start": quiet.1, "end": quiet.2},
    })
}

/// 이 컴퓨터에서 설정을 바꾼다.
pub async fn save(bench: &Bench, body: &Value) -> Reply {
    post(bench.addr(), NOTIFY_SETTINGS_PATH, body).await
}

/// 이 컴퓨터에서 설정을 바꾸고 200이어야 한다.
pub async fn save_ok(bench: &Bench, body: &Value) -> Value {
    let reply = save(bench, body).await;
    assert_eq!(reply.status, 200, "설정을 바꾼다: {}", reply.text);
    reply.body
}

/// 이 컴퓨터에서 설정을 읽는다.
pub async fn read(bench: &Bench) -> Value {
    let reply = get(bench.addr(), NOTIFY_SETTINGS_PATH).await;
    assert_eq!(reply.status, 200, "설정을 읽는다: {}", reply.text);
    reply.body
}

/// "팀 논의" 줄.
fn talk_row(settings: &Value) -> Value {
    settings["kinds"]
        .as_array()
        .unwrap_or_else(|| panic!("kinds는 배열이다: {settings}"))
        .iter()
        .find(|row| row["kind"] == json!(NOTIFY_TEAM_TALK_KIND))
        .cloned()
        .unwrap_or_else(|| panic!("팀 논의 줄이 있다: {settings}"))
}

// 검증: V87 V91 C2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 알림_설정의_맨_끝에_팀_논의가_폰_칸_없이_잠기지_않은_켜짐으로_있다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let settings = read(&bench).await;
    let kinds = settings["kinds"].as_array().expect("kinds는 배열이다");
    let names: Vec<&str> = kinds.iter().filter_map(|r| r["kind"].as_str()).collect();
    let mut want: Vec<&str> = NoticeKind::ALL.iter().map(|k| k.name()).collect();
    want.push(NOTIFY_TEAM_TALK_KIND);
    assert_eq!(names, want, "여섯 종류 다음에 팀 논의 하나");
    assert_eq!(
        kinds.last().unwrap(),
        &json!({
            "kind": NOTIFY_TEAM_TALK_KIND, "label": NOTIFY_TEAM_TALK_LABEL,
            "push": null, "messenger": true, "locked": false
        }),
        "{settings}"
    );
    assert_eq!(NOTIFY_TEAM_TALK_LABEL, "팀 논의");
    bench.close().await;
}

// 검증: V91 C15 M6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 팀_논의를_끄면_답과_읽기에_남고_다시_켜도_남는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let answered = save_ok(
        &bench,
        &settings_body(&[], Some(false), (false, "22:00", "07:00")),
    )
    .await;
    assert_eq!(talk_row(&answered)["messenger"], json!(false), "{answered}");
    assert_eq!(talk_row(&answered)["push"], json!(null), "{answered}");
    let settings = read(&bench).await;
    assert_eq!(answered, settings, "답은 읽기와 같은 모양");

    let bench = bench.restart(&tg).await;
    let again = read(&bench).await;
    assert_eq!(again, settings, "다시 켜도 같은 값");
    bench.close().await;
}

// 검증: V91 C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 몸통에_팀_논의_줄이_없으면_그대로이고_폰_값은_보지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    save_ok(
        &bench,
        &settings_body(&[], Some(false), (false, "22:00", "07:00")),
    )
    .await;
    let six_only = save_ok(
        &bench,
        &settings_body(
            &[("daily_summary", false, true)],
            None,
            (false, "22:00", "07:00"),
        ),
    )
    .await;
    assert_eq!(
        talk_row(&six_only)["messenger"],
        json!(false),
        "줄이 없으면 지금 값 그대로: {six_only}"
    );

    let mut body = settings_body(&[], Some(true), (false, "22:00", "07:00"));
    body["kinds"][6]["push"] = json!(true);
    let answered = save_ok(&bench, &body).await;
    assert_eq!(talk_row(&answered)["messenger"], json!(true), "{answered}");
    assert_eq!(
        talk_row(&answered)["push"],
        json!(null),
        "폰 칸은 없다: {answered}"
    );
    bench.close().await;
}

// 검증: V91 E4 C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 팀_논의의_메신저_값이_참거짓이_아니면_400이고_그대로다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let before = read(&bench).await;
    let mut body = settings_body(&[], Some(true), (false, "22:00", "07:00"));
    body["kinds"][6]["messenger"] = json!("no");
    let reply = save(&bench, &body).await;
    assert_eq!(reply.status, 400, "{}", reply.text);
    assert_eq!(
        reply.body["error"],
        json!("invalid_input"),
        "{}",
        reply.text
    );
    assert_eq!(read(&bench).await, before, "값은 그대로다");
    bench.close().await;
}
