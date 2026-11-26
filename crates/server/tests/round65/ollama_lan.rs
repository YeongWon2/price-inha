//! 같은 집 다른 기기의 Ollama(결정 34): 켜야만 쓴다.
//! - 뜻 검색 설정에 `private_network: true`를 줄 때만 사설망 주소를 받는다. 주지 않으면 지금처럼 루프백만
//!   (`loopback_only`). 켜도 공인 주소 · 도메인은 `private_only`로 거부하고 남기지 않는다.
//! - 설정 화면의 뜻 검색 칸(`GET`)과 `doctor`의 뜻 검색 줄에 "임베딩을 이 컴퓨터 밖(사설망 <주소>)으로 보냄"을 늘
//!   보인다(켜기 전에는 "켜면 … 보냄"). 루프백이면 그 줄이 없다.
//!
//! 시험은 다른 기기에 닿지 않는다: 사설망 주소는 꺼진 설정으로만 남기고, 켜진 확인은 루프백 가짜 Ollama로 한다.

use price_inha::{
    CheckItem, Os, SEMANTIC_LOOPBACK_ONLY, SEMANTIC_PRIVATE_ONLY, SETUP_SEMANTIC_PATH, Service,
    doctor,
};
use serde_json::json;

use crate::common::{Bench, ScriptedRunner, plain_settings, setup};

const LAN: &str = "http://192.168.0.2:11434";

// 검증: D10 V23
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 켜야만_사설망_주소를_받고_공인_주소는_켜도_받지_않는다() {
    let bench = Bench::start().await;
    let plain = json!({"enabled": false, "base_url": LAN, "model": "bge-m3"});
    let reply = bench.post(SETUP_SEMANTIC_PATH, &plain).await;
    assert_eq!(reply.status, 400, "{}", reply.text);
    assert_eq!(
        reply.body["error"], SEMANTIC_LOOPBACK_ONLY,
        "켜지 않으면 지금처럼"
    );

    for outside in ["http://8.8.8.8:11434", "https://ollama.example:11434"] {
        let body = json!({"enabled": false, "base_url": outside, "model": "bge-m3", "private_network": true});
        let reply = bench.post(SETUP_SEMANTIC_PATH, &body).await;
        assert_eq!(reply.status, 400, "{outside}: {}", reply.text);
        assert_eq!(reply.body["error"], SEMANTIC_PRIVATE_ONLY, "{outside}");
    }

    let lan =
        json!({"enabled": false, "base_url": LAN, "model": "bge-m3", "private_network": true});
    let reply = bench.post(SETUP_SEMANTIC_PATH, &lan).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["private_network"], true);
    assert_eq!(reply.body["base_url"], LAN);
    let shown = bench.get(SETUP_SEMANTIC_PATH).await;
    assert_eq!(shown.body["private_network"], true, "다시 읽어도 같다");
    let outside = shown.body["outside"].as_str().expect("밖으로 보낸다는 줄");
    assert!(
        outside.contains("이 컴퓨터 밖") && outside.contains("192.168.0.2"),
        "{outside}"
    );

    // 루프백으로 돌리면 그 줄은 없다.
    let back = json!({"enabled": false, "base_url": "http://127.0.0.1:11434", "model": "bge-m3"});
    let reply = bench.post(SETUP_SEMANTIC_PATH, &back).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert!(reply.body["outside"].is_null(), "{}", reply.body);
    assert_eq!(reply.body["private_network"], false);
    bench.close().await;
}

// 검증: D10 V23
#[test]
fn 진단은_사설망_ollama로_보낸다는_줄을_늘_보인다() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join(price_inha::SEMANTIC_SETTINGS_FILE),
        json!({"enabled": false, "base_url": LAN, "model": "bge-m3", "private_network": true})
            .to_string(),
    )
    .unwrap();
    let defs = tempfile::tempdir().unwrap();
    let runner = ScriptedRunner::ok();
    let service = Service::new(setup(Os::Linux, home.path(), defs.path(), &[]), &runner);
    let report = doctor(&plain_settings(home.path()), &service);
    let line = report.check(CheckItem::Ollama).unwrap();
    assert!(line.ok, "꺼져 있으면 문제가 아니다: {line:?}");
    assert!(
        line.what.contains("이 컴퓨터 밖") && line.what.contains("사설망 192.168.0.2"),
        "{line:?}"
    );

    // 켜지 않은(`private_network` 없음) 사설망 주소 파일은 지금처럼 기본(루프백 · 꺼짐)으로 읽는다.
    std::fs::write(
        home.path().join(price_inha::SEMANTIC_SETTINGS_FILE),
        json!({"enabled": true, "base_url": LAN, "model": "bge-m3"}).to_string(),
    )
    .unwrap();
    let report = doctor(&plain_settings(home.path()), &service);
    let line = report.check(CheckItem::Ollama).unwrap();
    assert!(!line.what.contains("이 컴퓨터 밖"), "{line:?}");
}
