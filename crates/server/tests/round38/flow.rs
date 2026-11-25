//! 구역을 차례로 지나 끝 요약이 나온다 · 이미 설정된 홈에서는 "있음"과 구역 고르기(원본 Hermes 다시 실행) ·
//! 서비스 구역은 `service install`과 같은 일(시험은 `--dry-run` — 쓰지도 부르지도 않는다)(결정 24 · D1 · D4).

use pi_market::Symbol;
use price_inha::{
    ANSWER_CLI, SETUP_ALREADY, SETUP_DOCTOR_HINT, SETUP_DONE, SETUP_MESSENGER_LATER,
    SETUP_RUN_HINT, SETUP_SKIPPED, load_watchlist, save_watchlist,
};

use crate::common::{
    BAIT, Bench, CLIENT_ID, CLIENT_SECRET, FakeCli, Login, Root, plain_files_with, registered,
};
use crate::run::{Env, run_binary, strings};

/// `setup [구역] --stdin-answers --dry-run --home <홈> --port <포트>`.
fn dry_args(section: Option<&str>, root: &Root, port: u16) -> Vec<String> {
    let port = port.to_string();
    let mut list = vec!["setup"];
    list.extend(section);
    list.extend([
        "--stdin-answers",
        "--dry-run",
        "--home",
        root.home_str(),
        "--port",
        &port,
    ]);
    strings(&list)
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect()
}

// 검증: D1 D4 V31 V46 V47
#[tokio::test(flavor = "multi_thread")]
async fn 구역을_차례로_지나_끝_요약이_나온다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::In);
    let bench = Bench::start(&root.home, &["--register-attempts", "10"]).await;
    // ai: cli / broker: 두 칸 / symbols: 한 줄 / service: 예(dry-run) / open: 묻지 않는다.
    let answers = format!("{ANSWER_CLI}\n{CLIENT_ID}\n{CLIENT_SECRET}\n005930\ny\n");
    let ran = run_binary(
        Env::new(&root, &cli),
        dry_args(None, &root, bench.port),
        Some(answers),
    )
    .await;

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.summary(),
        pairs(&[
            ("ai", SETUP_DONE),
            ("broker", SETUP_DONE),
            ("symbols", SETUP_DONE),
            ("service", SETUP_DONE),
            ("open", SETUP_DONE),
        ]),
        "구역마다 한 줄, 차례대로: {}",
        ran.stdout
    );
    assert!(
        ran.stdout.contains(&root.home.display().to_string()),
        "0단계: 보관 폴더 위치를 보인다: {}",
        ran.stdout
    );
    assert!(ran.stdout.contains(SETUP_MESSENGER_LATER), "{}", ran.stdout);
    assert!(
        ran.stdout
            .contains(&format!("http://127.0.0.1:{}/", bench.port)),
        "열기: 대시보드 주소: {}",
        ran.stdout
    );
    assert!(ran.stdout.contains(SETUP_DOCTOR_HINT), "{}", ran.stdout);

    let status = bench.status().await;
    assert_eq!(status["ai"]["method"], "cli", "{status}");
    assert!(registered(&status, "broker"), "{status}");
    assert!(registered(&status, "symbols"), "{status}");
    for secret in [CLIENT_ID, CLIENT_SECRET, BAIT] {
        ran.assert_absent(secret, "비밀값");
        assert!(plain_files_with(&root.home, secret).is_empty());
    }
    assert!(
        !root.user.join("Library").exists() && !root.user.join(".config").exists(),
        "dry-run이라 서비스 정의를 쓰지 않는다"
    );
    bench.stop().await;
}

// 검증: D1
#[tokio::test(flavor = "multi_thread")]
async fn 이미_설정된_홈에서는_있음을_보이고_다시_할_구역을_고른다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    std::fs::create_dir_all(&root.home).unwrap();
    save_watchlist(&root.home, &[Symbol::new("005930")]).unwrap();
    let bench = Bench::start(&root.home, &[]).await;
    assert!(
        registered(&bench.status().await, "symbols"),
        "종목은 이미 있다"
    );

    let ran = run_binary(
        Env::new(&root, &cli),
        dry_args(None, &root, bench.port),
        Some("symbols\n000660\n".to_string()),
    )
    .await;

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert!(ran.stdout.contains(SETUP_ALREADY), "{}", ran.stdout);
    assert_eq!(
        ran.summary(),
        pairs(&[
            ("ai", SETUP_SKIPPED),
            ("broker", SETUP_SKIPPED),
            ("symbols", SETUP_DONE),
            ("service", SETUP_SKIPPED),
            ("open", SETUP_SKIPPED),
        ]),
        "고른 구역만 돈다: {}",
        ran.stdout
    );
    let saved: Vec<String> = load_watchlist(&root.home)
        .unwrap()
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    assert_eq!(saved, ["000660"]);
    bench.stop().await;
}

/// 할 일 글에서 부를 명령 줄(`$ …`)과 정의 파일 줄(앞 공백은 뗀다 — 들여쓰기는 계약이 아니다).
fn plan_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("$ ") || line.starts_with("정의 파일"))
        .map(str::to_string)
        .collect()
}

// 검증: D1
#[tokio::test(flavor = "multi_thread")]
async fn 서비스_구역에서_예는_service_install과_같은_할_일이다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let bench = Bench::start(&root.home, &[]).await;
    let port = bench.port.to_string();
    let expected = run_binary(
        Env::new(&root, &cli),
        strings(&[
            "service",
            "install",
            "--dry-run",
            "--home",
            root.home_str(),
            "--port",
            &port,
        ]),
        None,
    )
    .await;
    assert_eq!(expected.code, Some(0), "{}", expected.all());
    let expected = plan_lines(&expected.stdout);
    assert!(!expected.is_empty(), "service install --dry-run의 할 일");

    let ran = run_binary(
        Env::new(&root, &cli),
        dry_args(Some("service"), &root, bench.port),
        Some("y\n".to_string()),
    )
    .await;

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.result_of("service").as_deref(),
        Some(SETUP_DONE),
        "{}",
        ran.stdout
    );
    for line in &expected {
        assert!(
            ran.stdout.contains(line.as_str()),
            "같은 할 일 `{line}`: {}",
            ran.stdout
        );
    }
    assert!(!root.user.join("Library").exists(), "쓰지 않았다");
    bench.stop().await;
}

// 검증: D1
#[tokio::test(flavor = "multi_thread")]
async fn 서비스_구역에서_아니오는_run을_안내한다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let bench = Bench::start(&root.home, &[]).await;
    let ran = run_binary(
        Env::new(&root, &cli),
        dry_args(Some("service"), &root, bench.port),
        Some("n\n".to_string()),
    )
    .await;

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert!(ran.stdout.contains(SETUP_RUN_HINT), "{}", ran.stdout);
    assert_eq!(
        ran.result_of("service").as_deref(),
        Some(SETUP_SKIPPED),
        "{}",
        ran.stdout
    );
    bench.stop().await;
}
