//! 백업 설정 화면(저장소 이름 · 수신자 공개키)과 되살리기(작업 폴더를 따로 넘긴다, F9).

use price_inha::SETUP_BACKUP_PATH;
use serde_json::json;

use crate::common::{AgeKeys, Bench, REPO, get, kst, monday, ms, post};

// 검증: F9 C16
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 화면에서_공개키를_바꾸면_그_키로_암호화해_올리고_다시_켜도_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let fresh = AgeKeys::generate();
    let body = json!({"repo": REPO, "recipient": fresh.recipient});
    let saved = post(bench.addr(), SETUP_BACKUP_PATH, &body).await;
    assert_eq!(saved.status, 200, "{}", saved.text());
    let shown = get(bench.addr(), SETUP_BACKUP_PATH).await;
    assert_eq!(shown.body["repo"], REPO);
    assert_eq!(shown.body["recipient"], fresh.recipient.as_str());

    let report = bench.app.back_up_now().await.expect("백업");
    let bytes = bench.github.file(&report.remote_path);
    assert!(
        pi_backup::open(&bytes, &fresh.identity).is_ok(),
        "새 키로 풀린다"
    );
    assert!(
        pi_backup::open(&bytes, &bench.age.identity).is_err(),
        "옛 키로는 풀리지 않는다"
    );

    let (dir, age) = bench.close().await;
    let bench = Bench::with(dir, kst(monday(), 10, 30), Some(age)).await;
    let shown = get(bench.addr(), SETUP_BACKUP_PATH).await;
    assert_eq!(
        shown.body["recipient"],
        fresh.recipient.as_str(),
        "다시 켜도 남는다"
    );
    bench.close().await;
}

// 검증: F9 E15
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 저장소_이름이나_공개키가_틀리면_받지_않고_그대로_둔다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let before = get(bench.addr(), SETUP_BACKUP_PATH).await;
    assert_eq!(before.status, 200, "{}", before.text());
    for body in [
        json!({"repo": "이름만", "recipient": bench.age.recipient}),
        json!({"repo": REPO, "recipient": "age-아님"}),
        json!({"repo": REPO}),
    ] {
        let refused = post(bench.addr(), SETUP_BACKUP_PATH, &body).await;
        assert_eq!(refused.status, 400, "{body}: {}", refused.text());
    }
    let after = get(bench.addr(), SETUP_BACKUP_PATH).await;
    assert_eq!(after.body, before.body, "틀린 값은 남기지 않는다");
    bench.close().await;
}

// 검증: F9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 설정_화면에_백업_설정_칸이_있다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let page = get(bench.addr(), "/").await;
    assert_eq!(page.status, 200);
    assert!(
        page.content_type.starts_with("text/html"),
        "{}",
        page.content_type
    );
    let text = page.text();
    assert!(text.contains(SETUP_BACKUP_PATH), "백업 설정 경로를 부른다");
    assert!(text.contains("data-backup=\"repo\""), "저장소 이름 칸");
    assert!(text.contains("data-backup=\"recipient\""), "공개키 칸");
    bench.close().await;
}

// 검증: V51 C8 E9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 되살리기는_작업_폴더를_남기지_않고_빈_폴더에_되살린다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let report = bench.app.back_up_now().await.expect("백업");
    let encrypted = bench.github.file(&report.remote_path);
    let (_old, age) = bench.close().await;

    let parent = tempfile::tempdir().unwrap();
    let target = parent.path().join("home");
    let restored = price_inha::restore_backup(
        &encrypted,
        &age.identity,
        &target,
        ms(kst(monday(), 10, 20)),
    )
    .expect("빈 폴더에 되살린다");
    assert_eq!(restored.manifest.files.len(), report.manifest.files.len());
    assert!(
        target.join(pi_backup::SNAPSHOT_FILE).exists(),
        "저장소가 되살아난다"
    );
    let left: Vec<String> = std::fs::read_dir(parent.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, vec!["home".to_string()], "작업 폴더가 남지 않는다");
}
