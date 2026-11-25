//! 증권 키 · 종목 구역: 마법사는 파일을 직접 쓰지 않고 도는 서버의 설정 경로(`/api/setup/keys` ·
//! `/api/setup/symbols`)를 부른다 — 그래서 저장하자마자 도는 서버의 설정 상태에 보인다. 틀린 값은 서버의 거절 글을
//! 보이고 다시 묻는다. 빈 입력은 건너뛰기(결정 24 · D4 · V31 · V46).

use price_inha::{SETUP_DONE, SETUP_SECTOR_NOTICE, SETUP_SKIPPED, SetupRefusal, load_watchlist};

use crate::common::{
    Bench, CLIENT_ID, CLIENT_SECRET, FakeCli, Login, Root, WRONG_SECRET, plain_files_with,
    registered,
};
use crate::run::{Env, run_binary, strings};

/// `setup <구역> --stdin-answers --home <홈> --port <포트>`.
fn section_args(section: &str, root: &Root, port: u16) -> Vec<String> {
    strings(&[
        "setup",
        section,
        "--stdin-answers",
        "--home",
        root.home_str(),
        "--port",
        &port.to_string(),
    ])
}

// 검증: V31 V46 V47
#[tokio::test(flavor = "multi_thread")]
async fn 증권_키는_서버의_설정_경로로_등록되고_틀리면_두_칸을_다시_묻는다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let bench = Bench::start(&root.home, &["--register-attempts", "10"]).await;
    let ran = run_binary(
        Env::new(&root, &cli),
        section_args("broker", &root, bench.port),
        Some(format!(
            "{CLIENT_ID}\n{WRONG_SECRET}\n{CLIENT_ID}\n{CLIENT_SECRET}\n"
        )),
    )
    .await;

    assert!(
        ran.stdout.contains(&SetupRefusal::KeyRejected.to_string()),
        "가짜 토스가 거절한 까닭을 서버의 글로 보인다: {}",
        ran.stdout
    );
    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.result_of("broker").as_deref(),
        Some(SETUP_DONE),
        "{}",
        ran.stdout
    );
    let status = bench.status().await;
    assert!(
        registered(&status, "broker"),
        "도는 서버가 등록했다: {status}"
    );
    for secret in [CLIENT_ID, CLIENT_SECRET, WRONG_SECRET] {
        ran.assert_absent(secret, "넣은 증권 키 칸");
        assert!(
            plain_files_with(&root.home, secret).is_empty(),
            "비밀 보관 파일 밖에 키 글자가 없다"
        );
        assert!(
            !status.to_string().contains(secret),
            "설정 상태에 키 값이 없다"
        );
    }
    bench.stop().await;
}

// 검증: V31
#[tokio::test(flavor = "multi_thread")]
async fn 증권_키_첫_칸이_비면_건너뛴다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let bench = Bench::start(&root.home, &[]).await;
    let ran = run_binary(
        Env::new(&root, &cli),
        section_args("broker", &root, bench.port),
        Some("\n".to_string()),
    )
    .await;

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.result_of("broker").as_deref(),
        Some(SETUP_SKIPPED),
        "{}",
        ran.stdout
    );
    assert!(!registered(&bench.status().await, "broker"));
    bench.stop().await;
}

// 검증: V31 D1
#[tokio::test(flavor = "multi_thread")]
async fn 종목은_도는_서버에_바로_반영되고_틀리면_거절_글_뒤에_다시_묻는다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let bench = Bench::start(&root.home, &[]).await;
    assert!(
        !registered(&bench.status().await, "symbols"),
        "처음에는 종목이 없다"
    );
    let ran = run_binary(
        Env::new(&root, &cli),
        section_args("symbols", &root, bench.port),
        Some("bad$$\n005930,000660\n".to_string()),
    )
    .await;

    assert!(
        ran.stdout.contains(&SetupRefusal::InvalidInput.to_string()),
        "서버의 거절 글: {}",
        ran.stdout
    );
    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.result_of("symbols").as_deref(),
        Some(SETUP_DONE),
        "{}",
        ran.stdout
    );
    // 파일을 직접 썼다면 도는 서버는 모른다 — 서버의 설정 상태에 바로 보이는 것이 설정 경로를 부른 증거다.
    assert!(
        registered(&bench.status().await, "symbols"),
        "도는 서버가 받은 종목이다"
    );
    let saved: Vec<String> = load_watchlist(&root.home)
        .unwrap()
        .iter()
        .map(|s| s.as_str().to_string())
        .collect();
    assert_eq!(saved, ["005930", "000660"]);
    assert!(
        ran.stdout.contains(SETUP_SECTOR_NOTICE),
        "업종표가 빈 홈이라 업종 안내가 나온다: {}",
        ran.stdout
    );
    bench.stop().await;
}
