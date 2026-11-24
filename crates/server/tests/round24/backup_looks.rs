//! 21차 6번(뒤): 외형 저장(`office-looks.json`)이 백업에 들어간다. 가짜 GitHub에 올라온 묶음을 우리 age 키로 풀어
//! 외형 파일이 들어 있고 고른 외형이 그 안에 있는지 본다. 실제 GitHub는 부르지 않는다.
//!
//! 지금 백업 묶음에는 저장소 스냅샷 한 파일뿐이다 — 들어가게 하려면 backup 모듈의 묶음 만들기를 넓혀야 한다
//! (server만으로는 안 된다: 조정자에게 알린다).

use price_inha::{OFFICE_LOOK_PATH, OFFICE_LOOKS_PATH};
use serde_json::json;

use crate::common::{AgeKeys, FakeGithub, Plan, get, kst, monday, post};

// 검증: V48 D7 V15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 백업을_풀어_보면_고른_외형이_들어_있다() {
    let github = FakeGithub::start().await;
    let age = AgeKeys::generate();
    let bench = Plan::demo()
        .backing_up(&github, &age)
        .start(kst(monday(), 10, 0))
        .await;

    // 지금 외형이 아닌 외형 하나를 고른다.
    let looks = get(bench.addr(), OFFICE_LOOKS_PATH).await;
    let current = looks.body["agents"]["news"].as_str().expect("지금 외형");
    let other = looks.body["available"]
        .as_array()
        .expect("고를 수 있는 외형들")
        .iter()
        .filter_map(|look| look.as_str())
        .find(|look| *look != current)
        .expect("다른 외형")
        .to_string();
    let chosen = post(
        bench.addr(),
        OFFICE_LOOK_PATH,
        &json!({"agent": "news", "look": other}),
    )
    .await;
    assert_eq!(chosen.status, 200, "외형을 고른다: {}", chosen.text);
    let shown = other;

    let report = bench.app.back_up_now().await.expect("백업");
    let bytes = github.file(&report.remote_path);
    let bundle = pi_backup::open(&bytes, &age.identity).expect("우리 키로 풀린다");
    pi_backup::verify(&bundle).expect("온전한 묶음");
    let looks = bundle
        .files
        .iter()
        .find(|file| file.path == "office-looks.json")
        .unwrap_or_else(|| {
            let names: Vec<&str> = bundle.files.iter().map(|f| f.path.as_str()).collect();
            panic!("백업에 외형 파일이 있다: 지금 든 파일 {names:?}")
        });
    let text = String::from_utf8_lossy(&looks.bytes);
    assert!(
        text.contains(&shown),
        "고른 외형({shown})이 백업의 외형 파일에 있다: {text}"
    );
    bench.close().await;
}
