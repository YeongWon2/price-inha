//! 바깥 키 칸(DART · ECOS · FRED · 백업 GitHub 토큰)을 설정에서 등록한다: 확인 → 저장, 답은 등록 여부뿐.

use pi_secrets::KeySlot;
use price_inha::{
    KEY_BACKUP_GITHUB_TOKEN, KEY_DART, KEY_ECOS, KEY_FRED, REMOTE_KEYS_PATH, SETUP_KEYS_PATH,
    SETUP_PATH, SETUP_SLOTS_PATH,
};
use serde_json::{Value, json};

use crate::common::{
    Bench, get_with, outside_secrets_contains, post_with, remote_session, session,
};

/// 칸마다 `item` 이름 · 칸 이름 · 시험용 가짜 값(실제 키가 아니다).
const CASES: [(&str, KeySlot, &str); 4] = [
    (KEY_DART, KeySlot::Dart, "fake-dart-key-round15-aaaaaaaa"),
    (KEY_ECOS, KeySlot::Ecos, "fake-ecos-key-round15-bbbbbbbb"),
    (KEY_FRED, KeySlot::Fred, "fake-fred-key-round15-cccccccc"),
    (
        KEY_BACKUP_GITHUB_TOKEN,
        KeySlot::BackupGithubToken,
        "fake-github-token-round15-dddddddd",
    ),
];

fn body(item: &str, slot: KeySlot, value: &str) -> Value {
    json!({"item": item, "fields": {slot.fields()[0]: value}})
}

fn slot_flags(view: &Value) -> Vec<(String, bool)> {
    view["slots"]
        .as_array()
        .expect("slots 목록이 있어야 한다")
        .iter()
        .map(|slot| {
            (
                slot["name"].as_str().unwrap_or_default().to_string(),
                slot["registered"].as_bool().unwrap_or_default(),
            )
        })
        .collect()
}

#[tokio::test]
async fn each_slot_registers_after_upstream_accepts_and_value_shows_nowhere() {
    let bench = Bench::start(true).await;
    let cookie = session(bench.addr(), None).await;
    for (item, slot, value) in CASES {
        let reply = post_with(
            bench.addr(),
            SETUP_KEYS_PATH,
            &cookie,
            &body(item, slot, value),
        )
        .await;
        assert_eq!(reply.status, 200, "{item}: {}", reply.text);
        assert_eq!(reply.body, json!({"registered": true}), "{item}");
        assert!(!reply.text.contains(value), "{item}: 답에 값이 없다");
        assert_eq!(
            bench.stored(slot).as_deref(),
            Some(value),
            "{item}: 보관소에 있다"
        );
        assert!(
            !outside_secrets_contains(&bench.home(), value),
            "{item}: 키 파일 밖(저장소 · 기록)에 값이 없다"
        );
    }
    assert!(
        bench.upstream_hits().await >= CASES.len(),
        "칸마다 확인을 한 번은 했다"
    );
    let status = get_with(bench.addr(), SETUP_PATH, Some(&cookie)).await;
    let slots = get_with(bench.addr(), SETUP_SLOTS_PATH, Some(&cookie)).await;
    for (item, _, value) in CASES {
        assert!(
            !status.text.contains(value),
            "{item}: 설정 상태에 값이 없다"
        );
        assert!(!slots.text.contains(value), "{item}: 칸 상태에 값이 없다");
    }
    bench.close().await;
}

#[tokio::test]
async fn slot_status_lists_only_registered_flags_in_order() {
    let bench = Bench::start(true).await;
    let cookie = session(bench.addr(), None).await;
    let before = get_with(bench.addr(), SETUP_SLOTS_PATH, Some(&cookie)).await;
    assert_eq!(before.status, 200, "{}", before.text);
    let names: Vec<(String, bool)> = CASES
        .iter()
        .map(|(item, _, _)| ((*item).to_string(), false))
        .collect();
    assert_eq!(slot_flags(&before.body), names, "칸 순서대로 등록 안 됨");
    for slot in before.body["slots"].as_array().unwrap() {
        let mut keys: Vec<&String> = slot.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["name", "registered"], "이름 · 등록 여부만 싣는다");
    }
    let (item, slot, value) = CASES[0];
    let reply = post_with(
        bench.addr(),
        SETUP_KEYS_PATH,
        &cookie,
        &body(item, slot, value),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let after = get_with(bench.addr(), SETUP_SLOTS_PATH, Some(&cookie)).await;
    assert_eq!(slot_flags(&after.body)[0], (item.to_string(), true));
    assert!(!slot_flags(&after.body)[1].1, "다른 칸은 그대로다");
    let status = get_with(bench.addr(), SETUP_PATH, Some(&cookie)).await;
    assert_eq!(status.status, 200);
    assert_eq!(
        status.body["ready"],
        json!(false),
        "자료 키는 시작 조건이 아니다"
    );
    bench.close().await;
}

#[tokio::test]
async fn slot_status_needs_a_session() {
    let bench = Bench::start(true).await;
    bench.go_remote();
    let reply = get_with(bench.addr(), SETUP_SLOTS_PATH, None).await;
    assert_eq!(reply.status, 401, "{}", reply.text);
    bench.close().await;
}

#[tokio::test]
async fn rejected_by_upstream_is_not_stored() {
    let bench = Bench::start(false).await;
    let cookie = session(bench.addr(), None).await;
    for (item, slot, value) in CASES {
        let reply = post_with(
            bench.addr(),
            SETUP_KEYS_PATH,
            &cookie,
            &body(item, slot, value),
        )
        .await;
        assert_eq!(reply.status, 422, "{item}: {}", reply.text);
        assert_eq!(reply.body, json!({"error": "key_rejected"}), "{item}");
        assert_eq!(bench.stored(slot), None, "{item}: 저장하지 않는다");
        assert!(!reply.text.contains(value), "{item}");
    }
    bench.close().await;
}

#[tokio::test]
async fn remote_session_registers_only_through_remote_path() {
    let bench = Bench::start(true).await;
    let cookie = remote_session(&bench).await;
    let (item, slot, value) = CASES[3];
    let local = post_with(
        bench.addr(),
        SETUP_KEYS_PATH,
        &cookie,
        &body(item, slot, value),
    )
    .await;
    assert_eq!(local.status, 403, "{}", local.text);
    assert_eq!(bench.stored(slot), None);
    let remote = post_with(
        bench.addr(),
        REMOTE_KEYS_PATH,
        &cookie,
        &body(item, slot, value),
    )
    .await;
    assert_eq!(remote.status, 200, "{}", remote.text);
    assert_eq!(remote.body, json!({"registered": true}));
    assert!(!remote.text.contains(value));
    assert_eq!(bench.stored(slot).as_deref(), Some(value));
    bench.close().await;
}

#[tokio::test]
async fn unknown_item_or_wrong_field_is_bad_request() {
    let bench = Bench::start(true).await;
    let cookie = session(bench.addr(), None).await;
    let value = "fake-unknown-key-round15-eeeeeeee";
    for wrong in [
        json!({"item": "kakao", "fields": {"api_key": value}}),
        json!({"item": KEY_DART, "fields": {"token": value}}),
        json!({"item": KEY_BACKUP_GITHUB_TOKEN, "fields": {"api_key": value}}),
        json!({"item": KEY_FRED, "fields": {"api_key": "   "}}),
    ] {
        let reply = post_with(bench.addr(), SETUP_KEYS_PATH, &cookie, &wrong).await;
        assert_eq!(reply.status, 400, "{wrong}: {}", reply.text);
        assert_eq!(reply.body, json!({"error": "invalid_input"}));
    }
    assert_eq!(bench.upstream_hits().await, 0, "확인기를 부르지 않는다");
    bench.close().await;
}

#[tokio::test]
async fn pasted_spaces_are_trimmed_before_storing() {
    let bench = Bench::start(true).await;
    let cookie = session(bench.addr(), None).await;
    let (item, slot, value) = CASES[2];
    let padded = format!("  {value}\t ");
    let reply = post_with(
        bench.addr(),
        SETUP_KEYS_PATH,
        &cookie,
        &body(item, slot, &padded),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(bench.stored(slot).as_deref(), Some(value));
    bench.close().await;
}
