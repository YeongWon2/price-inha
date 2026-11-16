//! 백업 잇기(T19 · D7 · C13): 마감 뒤 올리기(V48), 키 없음(V49 · E39), 실패 알림(E15),
//! 되살린 뒤 멈춤 · 만료 · 키 다시 넣고 다시 시작(V51 · V52 · C8 · C16 · E9).

use pi_store::TeamState;
use price_inha::{AppError, DayPlan, RESTART_KEYS_REQUIRED, TEAM_PATH, TEAM_RESTART_PATH};
use serde_json::json;

use crate::common::{
    Bench, DART_KEY, GITHUB_TOKEN, events, get, kst, monday, ms, post, register_required, wait_for,
};

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

// 검증: V48 T19 D7 C13
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 장_마감_뒤_비공개_저장소에_암호화된_백업이_새로_올라온다() {
    let bench = Bench::start(kst(monday(), 15, 0)).await;
    assert!(bench.github.paths().is_empty());
    bench.set(kst(monday(), 15, 31));
    wait_for("마감 뒤 백업이 올라간다", || {
        bench
            .github
            .paths()
            .iter()
            .any(|p| p.starts_with(pi_backup::REMOTE_DIR) && p.ends_with(pi_backup::FILE_SUFFIX))
    })
    .await;
    let path = bench.github.paths().pop().unwrap();
    let bytes = bench.github.file(&path);
    let bundle = pi_backup::open(&bytes, &bench.age.identity).expect("우리 키로 풀린다");
    pi_backup::verify(&bundle).expect("온전한 묶음");
    assert!(
        bundle
            .files
            .iter()
            .any(|f| f.path == pi_backup::SNAPSHOT_FILE),
        "저장소 스냅샷이 들어 있다"
    );
    wait_for("마지막 성공 시각이 남는다", || {
        bench
            .app
            .backup_status()
            .is_ok_and(|s| s.last_success.is_some())
    })
    .await;
    bench.close().await;
}

// 검증: V49 E39 N8 P22
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 백업을_풀어_보면_키_파일과_키_글자가_없다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    register_required(bench.home()).await;
    let report = bench.app.back_up_now().await.expect("백업");
    let bytes = bench.github.file(&report.remote_path);
    assert!(!contains(&bytes, GITHUB_TOKEN));
    let bundle = pi_backup::open(&bytes, &bench.age.identity).unwrap();
    for file in &bundle.files {
        let name = file.path.rsplit('/').next().unwrap_or(&file.path);
        assert!(
            !pi_secrets::BACKUP_EXCLUDED_FILES.contains(&name) && name != pi_notify::VAPID_KEY_FILE,
            "키 파일이 없다: {name}"
        );
        for secret in [
            DART_KEY,
            GITHUB_TOKEN,
            "test-ai-key-round11-zz",
            "test-client-secret-round11",
        ] {
            assert!(!contains(&file.bytes, secret), "{name}에 키 글자가 없다");
        }
    }
    bench.close().await;
}

// 검증: E39 E15 T19
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 비밀이_든_저장소는_올리지_않고_실패를_알린다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let store = bench.store();
    store
        .append_at("test.leak", &json!({"value": DART_KEY}), ms(bench.now()))
        .unwrap();
    let refused = bench.app.back_up_now().await;
    assert!(matches!(refused, Err(AppError::Backup(_))), "{refused:?}");
    assert_eq!(bench.github.puts().await, 0, "아무것도 올리지 않는다");
    let notices = events(&store, "notice.backup_failure");
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(!notices[0].to_string().contains(DART_KEY));
    let status = bench.app.backup_status().unwrap();
    assert!(status.retry_pending, "다음 마감에 다시 한다");
    assert!(status.last_success.is_none());
    bench.close().await;
}

// 검증: V51 V52 C8 C16 E9 T19
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 되살리면_팀이_멈추고_승인은_만료되며_키를_넣고_다시_시작해야_돈다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench
        .ready_with(json!({"symbol_weight_pct": "10", "auto_order_limit_pct": "2"}))
        .await;
    let (_, number, fingerprint) = bench.awaiting().await;
    let report = bench.app.back_up_now().await.expect("백업");
    let encrypted = bench.github.file(&report.remote_path);
    let (_old, age) = bench.close().await;

    let dir = tempfile::tempdir().unwrap();
    let restored = pi_backup::restore(
        &encrypted,
        &age.identity,
        dir.path(),
        ms(kst(monday(), 10, 20)),
    )
    .expect("빈 폴더에 되살린다");
    assert!(!restored.outcome.expired_approvals.is_empty());

    let bench = Bench::with(dir, kst(monday(), 10, 30), Some(age)).await;
    let team = get(bench.addr(), TEAM_PATH).await;
    assert_eq!(team.body["state"], "full_stop", "되살리면 팀은 멈춘다");
    let card = get(bench.addr(), &format!("/api/approvals/{}", number.as_str())).await;
    assert_eq!(card.body["status"], "expired", "그 승인은 만료다");
    let answer = post(
        bench.addr(),
        &format!("/api/approvals/{}/approve", number.as_str()),
        &json!({"fingerprint": fingerprint.as_str()}),
    )
    .await;
    assert_eq!(
        answer.body["result"], "rejected",
        "만료된 승인은 실행하지 않는다"
    );
    // 전체 정지에서는 하루 운용을 시작하지 못한다(V9): 멈춤 오류가 나고 상태는 그대로다.
    let refused = bench.trading().decide_day(DayPlan::Trade);
    assert!(
        matches!(
            refused,
            Err(price_inha::TradingError::Halted(TeamState::FullStop))
        ),
        "{refused:?}"
    );
    let state = bench.store().team_state().unwrap();
    assert_eq!(
        state,
        TeamState::FullStop,
        "다시 시작 전에는 팀이 돌지 않는다"
    );
    let restart = post(bench.addr(), TEAM_RESTART_PATH, &json!({})).await;
    assert_eq!(restart.status, 409);
    assert_eq!(
        restart.body["error"], RESTART_KEYS_REQUIRED,
        "키를 먼저 넣는다"
    );
    let (dir, age) = bench.close().await;
    register_required(dir.path()).await;
    let bench = Bench::with(dir, kst(monday(), 10, 40), Some(age)).await;
    assert_eq!(bench.store().team_state().unwrap(), TeamState::FullStop);
    let restart = post(bench.addr(), TEAM_RESTART_PATH, &json!({})).await;
    assert_eq!(restart.status, 200, "{}", restart.text());
    assert_eq!(restart.body["state"], "resting");
    let state = bench.trading().decide_day(DayPlan::Trade).unwrap();
    assert_eq!(state, TeamState::Operating, "다시 시작한 뒤에는 돈다");
    bench.close().await;
}
