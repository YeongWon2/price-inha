//! `GET|POST /api/setup/models`(R18-4 · C32 · V78): 읽기 모양 · 저장 · 설정 상태에 반영 · 다시 켜도 남음 · 틀린 값 400 ·
//! 원격 403 · 깨진 파일이면 기본 · 응답에 키 값 없음.

use pi_agent::Role;
use price_inha::{MODEL_CHOICES, MODEL_NAME_MAX, SETUP_MODELS_PATH, SETUP_PATH, TEAM_MODELS_FILE};
use serde_json::{Value, json};

use crate::common::{
    Bench, KEY_VALUES, default_model, fresh_home, get, get_with, keys_of, login, models_body,
    models_of, post, post_raw_with, shown_models,
};

/// 고른 모델(진짜 모델 이름처럼 보이지 않는다 — 글자 규칙만 맞춘다).
const PICKED: &str = "zz-model-round57.a";

/// 담당 id 여섯 → 그 모델(기본은 [`default_model`]).
fn expected(changed: &[(&str, &str)]) -> Vec<(String, String)> {
    let base = default_model();
    Role::ALL
        .iter()
        .map(|role| {
            let model = changed
                .iter()
                .find(|(agent, _)| *agent == role.id())
                .map_or(base.clone(), |(_, model)| (*model).to_string());
            (role.id().to_string(), model)
        })
        .collect()
}

/// 설정 상태(`GET /api/setup`)의 `models` 줄에서 담당 → 모델.
fn status_models(status: &Value) -> Vec<(String, String)> {
    status["models"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    (
                        row["agent"].as_str().unwrap_or_default().to_string(),
                        row["model"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

// 검증: R18-4 C32 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 읽기는_담당_여섯과_고를_수_있는_모델과_바꿀_수_있는지를_준다() {
    let bench = Bench::start().await;
    let reply = get(bench.addr(), SETUP_MODELS_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let body = &reply.body;
    assert_eq!(keys_of(body), ["choices", "editable", "roles"], "{body}");
    assert_eq!(body["editable"], json!(true), "이 컴퓨터면 바꿀 수 있다");
    let rows = body["roles"].as_array().expect("roles는 목록이다");
    let agents: Vec<&str> = rows.iter().filter_map(|r| r["agent"].as_str()).collect();
    let all: Vec<&str> = Role::ALL.iter().map(|role| role.id()).collect();
    assert_eq!(agents, all, "담당 여섯이 명단 순서대로 있다");
    for row in rows {
        assert_eq!(keys_of(row), ["agent", "default", "model", "name"], "{row}");
        assert_eq!(
            row["model"],
            json!(default_model()),
            "바꾸지 않았으면 기본: {row}"
        );
        assert_eq!(
            row["default"],
            json!(default_model()),
            "기본값을 함께 싣는다: {row}"
        );
        assert!(
            row["name"].as_str().is_some_and(|name| !name.is_empty()),
            "사람이 읽는 담당 이름: {row}"
        );
    }
    let choices = body["choices"].as_array().expect("choices는 목록이다");
    assert_eq!(choices.len(), MODEL_CHOICES.len(), "{body}");
    for (choice, (id, label, note)) in choices.iter().zip(MODEL_CHOICES) {
        assert_eq!(keys_of(choice), ["id", "label", "note"], "{choice}");
        assert_eq!(choice["id"], json!(id));
        assert_eq!(choice["label"], json!(label));
        assert_eq!(choice["note"], json!(note));
    }
    assert!(
        choices.iter().any(|c| c["id"] == json!(default_model())),
        "기본 모델은 고를 수 있는 목록에 있다: {body}"
    );
    bench.close().await;
}

// 검증: R18-4 C32 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 바꾸면_저장되어_읽기와_설정_상태에_보이고_다시_켜도_남는다() {
    let bench = Bench::start().await;
    let addr = bench.addr();
    let body = models_body(&[("scout", PICKED)], &default_model());
    let reply = post(addr, SETUP_MODELS_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        keys_of(&reply.body),
        ["choices", "editable", "roles"],
        "답은 읽기와 같은 모양: {}",
        reply.text
    );
    assert_eq!(shown_models(&reply.body), expected(&[("scout", PICKED)]));
    let read = get(addr, SETUP_MODELS_PATH).await;
    assert_eq!(shown_models(&read.body), expected(&[("scout", PICKED)]));
    let status = get(addr, SETUP_PATH).await;
    assert_eq!(
        status_models(&status.body),
        expected(&[("scout", PICKED)]),
        "설정 상태의 models 칸도 저장된 값을 보인다: {}",
        status.text
    );
    assert!(
        bench.home().join(TEAM_MODELS_FILE).exists(),
        "보관 폴더의 설정 파일에 남는다"
    );
    let bench = bench.restart().await;
    let read = get(bench.addr(), SETUP_MODELS_PATH).await;
    assert_eq!(
        shown_models(&read.body),
        expected(&[("scout", PICKED)]),
        "다시 켜도 같다"
    );
    let back = post(
        bench.addr(),
        SETUP_MODELS_PATH,
        &models_body(&[], &default_model()),
    )
    .await;
    assert_eq!(back.status, 200, "{}", back.text);
    assert_eq!(
        shown_models(&back.body),
        expected(&[]),
        "기본값으로 되돌린다"
    );
    bench.close().await;
}

/// 틀린 바꾸기 몸통들(글자 그대로).
fn wrong_bodies() -> Vec<String> {
    let base = default_model();
    let too_long = "a".repeat(MODEL_NAME_MAX + 1);
    let mut list: Vec<String> = [
        "Claude-Upper",
        "has space",
        "",
        too_long.as_str(),
        "a/b",
        "모델",
        "-starts-with-dash",
    ]
    .iter()
    .map(|model| models_body(&[("scout", model)], &base).to_string())
    .collect();
    let five: Vec<Value> = Role::ALL
        .iter()
        .skip(1)
        .map(|role| json!({"agent": role.id(), "model": base}))
        .collect();
    list.push(json!({ "roles": five }).to_string());
    let mut twice = five.clone();
    twice.push(json!({"agent": "strategy", "model": base}));
    list.push(json!({ "roles": twice }).to_string());
    let mut unknown = five;
    unknown.push(json!({"agent": "nobody", "model": base}));
    list.push(json!({ "roles": unknown }).to_string());
    let mut numbered = models_body(&[], &base);
    numbered["roles"][0]["model"] = json!(7);
    list.push(numbered.to_string());
    list.push(json!({"models": []}).to_string());
    list.push("이것은 JSON이 아니다".to_string());
    list
}

// 검증: R18-4 C32 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 틀린_값은_400이고_아무것도_저장하지_않는다() {
    let bench = Bench::start().await;
    let addr = bench.addr();
    let cookie = login(addr).await;
    for body in wrong_bodies() {
        let reply = post_raw_with(addr, SETUP_MODELS_PATH, &cookie, &body).await;
        assert_eq!(reply.status, 400, "{body} → {}", reply.text);
        assert_eq!(reply.body, json!({"error": "invalid_input"}), "{body}");
    }
    let read = get_with(addr, SETUP_MODELS_PATH, &cookie).await;
    assert_eq!(shown_models(&read.body), expected(&[]), "그대로다");
    assert!(
        !bench.home().join(TEAM_MODELS_FILE).exists(),
        "틀린 값으로는 파일을 만들지 않는다"
    );
    let fine = models_body(
        &[("risk", "a".repeat(MODEL_NAME_MAX).as_str())],
        &default_model(),
    );
    let reply = post_raw_with(addr, SETUP_MODELS_PATH, &cookie, &fine.to_string()).await;
    assert_eq!(reply.status, 200, "길이 한도까지는 받는다: {}", reply.text);
    bench.close().await;
}

// 검증: R18-4 C32 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 다른_기기는_읽기만_하고_바꾸면_403이다() {
    let bench = Bench::start().await;
    let addr = bench.addr();
    let cookie = bench.remote_login().await;
    let read = get_with(addr, SETUP_MODELS_PATH, &cookie).await;
    assert_eq!(read.status, 200, "원격 세션도 읽는다: {}", read.text);
    assert_eq!(read.body["editable"], json!(false), "{}", read.text);
    let body = models_body(&[("scout", PICKED)], &default_model()).to_string();
    let reply = post_raw_with(addr, SETUP_MODELS_PATH, &cookie, &body).await;
    assert_eq!(reply.status, 403, "{}", reply.text);
    let again = get_with(addr, SETUP_MODELS_PATH, &cookie).await;
    assert_eq!(shown_models(&again.body), expected(&[]), "저장하지 않았다");
    assert!(!bench.home().join(TEAM_MODELS_FILE).exists());
    bench.close().await;
}

// 검증: R18-4 C32 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 설정_파일이_깨졌으면_기본으로_돌고_팀은_멈추지_않는다() {
    let dir = fresh_home().await;
    std::fs::write(dir.path().join(TEAM_MODELS_FILE), "{깨진 값").unwrap();
    let bench = Bench::start_in(dir).await;
    let read = get(bench.addr(), SETUP_MODELS_PATH).await;
    assert_eq!(read.status, 200, "{}", read.text);
    assert_eq!(shown_models(&read.body), expected(&[]), "기본으로 본다");
    let from = bench.ai().settled().await;
    bench.discuss("깨진 파일 뒤 논의").await;
    let asked = bench.ai().asked_from(from);
    let scout = models_of(&asked, "scout");
    assert!(!scout.is_empty(), "팀은 그대로 논의한다: {asked:?}");
    assert!(
        asked.iter().all(|a| a.model == default_model()),
        "모두 기본 모델: {asked:?}"
    );
    bench.close().await;
}

// 검증: R18-4 C32 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 읽기와_바꾸기_답에_키_값이_없다() {
    let bench = Bench::start().await;
    let addr = bench.addr();
    let read = get(addr, SETUP_MODELS_PATH).await;
    let changed = post(
        addr,
        SETUP_MODELS_PATH,
        &models_body(&[("leader", PICKED)], &default_model()),
    )
    .await;
    assert_eq!(changed.status, 200, "{}", changed.text);
    for text in [&read.text, &changed.text] {
        for value in KEY_VALUES {
            assert!(!text.contains(value), "응답에 키 값이 없다: {text}");
        }
    }
    let saved = std::fs::read_to_string(bench.home().join(TEAM_MODELS_FILE)).unwrap();
    for value in KEY_VALUES {
        assert!(!saved.contains(value), "설정 파일에도 키 값이 없다");
    }
    bench.close().await;
}
