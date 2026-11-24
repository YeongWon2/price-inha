//! 1번 (가) · (나): 설정 상태(`GET /api/setup`)에 켤 때 정한 시장과 담당마다 지금 쓰는 모델 이름이 실린다.
//! 시장은 켤 때 인자로만 정하고, 담당 모델은 조립이 정한 값이다(고르는 저장 길이 없어 보기만 한다).

use pi_agent::Role;
use price_inha::{SETUP_DOCTOR_PATH, SETUP_PATH};
use serde_json::{Value, json};

use crate::common::{Bench, MODEL, UsBench, get, keys_of};

// 검증: R18-4 C32 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 설정_상태에_켤_때_정한_시장과_담당마다_지금_쓰는_모델이_실린다() {
    let bench = Bench::with_keys().await;
    let reply = get(bench.addr(), SETUP_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let status = &reply.body;
    assert_eq!(
        status["market"],
        json!("kr"),
        "인자를 주지 않으면 국내 시장: {status}"
    );
    let models = status["models"]
        .as_array()
        .unwrap_or_else(|| panic!("models는 목록이다: {status}"));
    let agents: Vec<&Value> = models.iter().map(|row| &row["agent"]).collect();
    let expected: Vec<&str> = Role::ALL.iter().map(|role| role.id()).collect();
    assert_eq!(agents, expected, "담당 여섯이 명단 순서대로 있다");
    for row in models {
        assert_eq!(
            keys_of(row),
            ["agent", "model"],
            "줄에는 담당과 모델 이름만 있다"
        );
        assert_eq!(
            row["model"],
            json!(MODEL),
            "조립이 정한 모델 이름을 그대로 보인다: {row}"
        );
    }
    bench.close().await;
}

// 검증: R18-4 C32 V78
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 미국_시장으로_켜면_시장은_us이고_팀을_잇지_않으면_모델_목록은_비어_있다() {
    let bench = UsBench::start().await;
    let addr = bench.app.local_addr();
    let reply = get(addr, SETUP_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["market"], json!("us"), "{}", reply.body);
    assert_eq!(
        reply.body["models"],
        json!([]),
        "팀이 없으면 지금 쓰는 모델이 없다"
    );
    let doctor = get(addr, SETUP_DOCTOR_PATH).await;
    assert_eq!(doctor.status, 200, "{}", doctor.text);
    let market = doctor.body["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["key"] == "market"))
        .unwrap_or_else(|| panic!("진단에 시장 항목이 있다: {}", doctor.body));
    assert!(
        market["detail"]
            .as_str()
            .is_some_and(|text| text.contains("us")),
        "진단의 시장 항목도 미국(us)이다: {market}"
    );
    tokio::time::timeout(crate::common::STEP, bench.app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}
