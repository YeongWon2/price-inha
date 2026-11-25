//! 40차 4번: 알림 전달처 설정 `GET` · `POST /api/notify/settings`(결정 24 · V87 · V91 · C2 · C15) — 기본 모양(모든 종류 ·
//! 잠긴 종류 표시) · 바꾼 값이 답과 읽기에 그대로 · 잠긴 종류를 둘 다 끄면 400 · 원격은 읽기만(바꾸기 403) · 다시 켜도
//! 남음 · 몸통 검사 · 원자적 저장(보관 폴더의 설정 파일).

use pi_notify::NoticeKind;
use price_inha::{
    NOTIFY_KIND_LABELS, NOTIFY_LOCKED_KINDS, NOTIFY_LOCKED_OFF, NOTIFY_SETTINGS_FILE,
    NOTIFY_SETTINGS_PATH, NOTIFY_TEAM_TALK_KIND,
};
use reqwest::Method;
use serde_json::{Value, json};

use crate::common::{Bench, FakeTelegram, Reply, get, get_with, post, send_anonymous, send_with};

/// 한 종류의 바꿀 값: (종류, 폰 푸시, 메신저).
pub type Route<'a> = (&'a str, bool, bool);

/// 바꾸기 몸통: 모든 종류를 켜고 `routes`만 그 값으로, 조용한 시간은 `quiet`(켜기, 시작, 끝).
pub fn settings_body(routes: &[Route<'_>], quiet: (bool, &str, &str)) -> Value {
    let kinds: Vec<Value> = NOTIFY_KIND_LABELS
        .iter()
        .map(|(kind, _)| {
            let (push, messenger) = routes
                .iter()
                .find(|(name, _, _)| name == kind)
                .map_or((true, true), |(_, push, messenger)| (*push, *messenger));
            json!({"kind": kind, "push": push, "messenger": messenger})
        })
        .collect();
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

/// 그 종류의 줄.
fn row<'a>(settings: &'a Value, kind: &str) -> &'a Value {
    settings["kinds"]
        .as_array()
        .unwrap_or_else(|| panic!("kinds는 배열이다: {settings}"))
        .iter()
        .find(|row| row["kind"] == json!(kind))
        .unwrap_or_else(|| panic!("{kind} 줄이 있다: {settings}"))
}

/// `HH:MM`(두 자리 시 · 두 자리 분)인가.
fn is_clock(text: &Value) -> bool {
    let Some(text) = text.as_str() else {
        return false;
    };
    let bytes = text.as_bytes();
    bytes.len() == 5
        && bytes[2] == b':'
        && text[..2].parse::<u8>().is_ok_and(|h| h < 24)
        && text[3..].parse::<u8>().is_ok_and(|m| m < 60)
}

// 검증: V87 V91 C2 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 기본은_모든_종류가_두_전달처로_가고_잠긴_종류가_표시되며_조용한_시간은_꺼짐이다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let settings = read(&bench).await;

    let kinds = settings["kinds"].as_array().expect("kinds는 배열이다");
    let names: Vec<&str> = kinds
        .iter()
        .filter_map(|row| row["kind"].as_str())
        .collect();
    // 41차 고침(대조 표 "고침", 지시서 41차 2번): "팀 논의" 종류가 맨 끝에 하나 더해진다(폰 칸 없음 — 그 줄의 모양은
    // round42/settings.rs가 본다). 아래 여섯 종류의 단언은 그대로다.
    let mut all: Vec<&str> = NoticeKind::ALL.iter().map(|kind| kind.name()).collect();
    all.push(NOTIFY_TEAM_TALK_KIND);
    assert_eq!(
        names, all,
        "지금의 NoticeKind 모두, 그 차례로(그다음 팀 논의)"
    );
    for row in kinds {
        let kind = row["kind"].as_str().unwrap();
        if kind == NOTIFY_TEAM_TALK_KIND {
            continue;
        }
        assert_eq!(row["push"], json!(true), "{row}");
        assert_eq!(row["messenger"], json!(true), "{row}");
        assert_eq!(
            row["locked"],
            json!(NOTIFY_LOCKED_KINDS.contains(&kind)),
            "{row}"
        );
        let label = NOTIFY_KIND_LABELS
            .iter()
            .find(|(name, _)| *name == kind)
            .map(|(_, label)| *label)
            .unwrap();
        assert_eq!(row["label"], json!(label), "{row}");
    }
    for locked in ["approval_request", "stall", "anomaly"] {
        assert!(
            NOTIFY_LOCKED_KINDS.contains(&locked),
            "{locked}는 끌 수 없다"
        );
    }
    assert_eq!(settings["quiet"]["enabled"], json!(false), "{settings}");
    assert!(is_clock(&settings["quiet"]["start"]), "{settings}");
    assert!(is_clock(&settings["quiet"]["end"]), "{settings}");
    assert_eq!(settings["editable"], json!(true), "{settings}");
    bench.close().await;
}

// 검증: V87 V91 C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 바꾼_값은_답과_읽기에_그대로이고_보관_폴더의_설정_파일에_남는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let body = settings_body(
        &[
            ("daily_summary", false, true),
            ("backup_failure", true, false),
            ("approval_request", true, false),
        ],
        (true, "23:30", "06:15"),
    );
    let answered = save_ok(&bench, &body).await;
    let settings = read(&bench).await;
    assert_eq!(answered, settings, "답은 읽기와 같은 모양(저장한 뒤의 값)");
    assert_eq!(row(&settings, "daily_summary")["push"], json!(false));
    assert_eq!(row(&settings, "daily_summary")["messenger"], json!(true));
    assert_eq!(row(&settings, "backup_failure")["messenger"], json!(false));
    assert_eq!(row(&settings, "approval_request")["push"], json!(true));
    assert_eq!(
        row(&settings, "approval_request")["messenger"],
        json!(false)
    );
    assert_eq!(row(&settings, "stall")["push"], json!(true));
    assert_eq!(
        settings["quiet"],
        json!({"enabled": true, "start": "23:30", "end": "06:15"}),
        "{settings}"
    );
    assert!(
        bench.home().join(NOTIFY_SETTINGS_FILE).is_file(),
        "보관 폴더에 설정 파일이 생긴다"
    );
    bench.close().await;
}

// 검증: V91 E4 C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 잠긴_종류를_둘_다_끄면_400이고_아무것도_바뀌지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let before = read(&bench).await;
    for kind in NOTIFY_LOCKED_KINDS {
        let body = settings_body(
            &[(kind, false, false), ("daily_summary", false, false)],
            (false, "22:00", "07:00"),
        );
        let reply = save(&bench, &body).await;
        assert_eq!(reply.status, 400, "{kind}: {}", reply.text);
        assert_eq!(
            reply.body["error"],
            json!(NOTIFY_LOCKED_OFF),
            "{kind}: {}",
            reply.text
        );
        assert_eq!(reply.body["kind"], json!(kind), "{kind}: {}", reply.text);
    }
    assert_eq!(read(&bench).await, before, "값은 그대로다");
    assert!(!bench.home().join(NOTIFY_SETTINGS_FILE).exists());

    // 잠긴 종류도 한 전달처만 끄는 것은 된다. 잠기지 않은 종류는 둘 다 끌 수 있다.
    let body = settings_body(
        &[
            ("approval_request", false, true),
            ("stall", true, false),
            ("daily_summary", false, false),
        ],
        (false, "22:00", "07:00"),
    );
    let settings = save_ok(&bench, &body).await;
    assert_eq!(row(&settings, "daily_summary")["push"], json!(false));
    assert_eq!(row(&settings, "daily_summary")["messenger"], json!(false));
    bench.close().await;
}

// 검증: E4 C15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 몸통이_틀리면_400이고_저장하지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let before = read(&bench).await;
    let good = settings_body(&[], (false, "22:00", "07:00"));

    let mut unknown = good.clone();
    unknown["kinds"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind": "weather", "push": true, "messenger": true}));
    let mut missing = good.clone();
    missing["kinds"].as_array_mut().unwrap().pop();
    let mut doubled = good.clone();
    let first = doubled["kinds"][0].clone();
    doubled["kinds"].as_array_mut().unwrap().push(first);
    let mut not_bool = good.clone();
    not_bool["kinds"][4]["push"] = json!("false");
    let mut no_quiet = good.clone();
    no_quiet.as_object_mut().unwrap().remove("quiet");

    for (what, body) in [
        ("모르는 종류", unknown),
        ("빠진 종류", missing),
        ("겹친 종류", doubled),
        ("참/거짓이 아님", not_bool),
        ("조용한 시간 없음", no_quiet),
        (
            "배열이 아님",
            json!({"kinds": {}, "quiet": good["quiet"].clone()}),
        ),
    ] {
        let reply = save(&bench, &body).await;
        assert_eq!(reply.status, 400, "{what}: {}", reply.text);
    }
    let cookie = crate::common::login(bench.addr()).await;
    let reply = send_with(
        bench.addr(),
        Method::POST,
        NOTIFY_SETTINGS_PATH,
        &cookie,
        Some("{깨진"),
    )
    .await;
    assert_eq!(reply.status, 400, "JSON 아님: {}", reply.text);

    assert_eq!(read(&bench).await, before, "값은 그대로다");
    assert!(!bench.home().join(NOTIFY_SETTINGS_FILE).exists());
    bench.close().await;
}

// 검증: V91 C15 E3 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기는_읽기만_하고_바꾸면_403이다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let cookie = bench.remote_login().await;

    let reply = get_with(bench.addr(), NOTIFY_SETTINGS_PATH, &cookie).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["editable"], json!(false), "{}", reply.text);

    let body = settings_body(
        &[("daily_summary", false, false)],
        (false, "22:00", "07:00"),
    )
    .to_string();
    let reply = send_with(
        bench.addr(),
        Method::POST,
        NOTIFY_SETTINGS_PATH,
        &cookie,
        Some(&body),
    )
    .await;
    assert_eq!(reply.status, 403, "{}", reply.text);

    let reply = send_anonymous(bench.addr(), Method::GET, NOTIFY_SETTINGS_PATH, None).await;
    assert_eq!(reply.status, 401, "세션 없는 원격: {}", reply.text);

    bench.go_local();
    let settings = read(&bench).await;
    assert_eq!(
        row(&settings, "daily_summary")["push"],
        json!(true),
        "그대로다"
    );
    assert!(!bench.home().join(NOTIFY_SETTINGS_FILE).exists());
    bench.close().await;
}

// 검증: V91 C15 M6
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 바꾼_설정은_다시_켜도_남는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let body = settings_body(&[("daily_summary", false, true)], (true, "21:00", "08:00"));
    let saved = save_ok(&bench, &body).await;

    let bench = bench.restart(&tg).await;
    let settings = read(&bench).await;
    assert_eq!(settings, saved, "다시 켜도 같은 값");
    assert_eq!(row(&settings, "daily_summary")["push"], json!(false));
    bench.close().await;
}
