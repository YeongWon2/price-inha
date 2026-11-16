//! 백업 설정 바꾸기(`POST /api/setup/backup`)는 키 등록처럼 그 컴퓨터에서만 받는다.

use price_inha::SETUP_BACKUP_PATH;
use serde_json::json;

use crate::common::{AgeKeys, Bench, REPO, dead_base_url, get_with, login, origin_of, post_with};

// 검증: F9 C16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기는_백업_설정을_읽어도_공개키를_바꾸지_못한다() {
    let bench = Bench::start(&dead_base_url()).await;
    let addr = bench.addr();
    let cookie = bench.remote_login().await;
    let shown = get_with(addr, SETUP_BACKUP_PATH, Some(&cookie)).await;
    assert_eq!(
        shown.status,
        200,
        "원격 세션은 읽을 수 있다: {}",
        shown.text()
    );

    let other = AgeKeys::generate();
    let body = json!({"repo": REPO, "recipient": other.recipient});
    let refused = post_with(
        addr,
        SETUP_BACKUP_PATH,
        Some(&cookie),
        &origin_of(addr),
        &body,
    )
    .await;
    assert_eq!(
        refused.status,
        pi_gate::LOCAL_ONLY_STATUS,
        "그 컴퓨터에서만 바꾼다: {}",
        refused.text()
    );

    bench.back_home();
    let local = login(addr).await;
    let after = get_with(addr, SETUP_BACKUP_PATH, Some(&local)).await;
    assert_eq!(after.status, 200, "{}", after.text());
    assert_eq!(
        after.body["recipient"],
        bench.age.recipient.as_str(),
        "공개키가 그대로다"
    );
    let saved = post_with(
        addr,
        SETUP_BACKUP_PATH,
        Some(&local),
        &origin_of(addr),
        &body,
    )
    .await;
    assert_eq!(
        saved.status,
        200,
        "그 컴퓨터에서는 바꾼다: {}",
        saved.text()
    );
    bench.close().await;
}
