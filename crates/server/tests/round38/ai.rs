//! AI 연결 구역(`setup ai --stdin-answers`): `claude`를 `PATH`에서 찾아 확인한 결과로 고르게 하고, 고른 것만
//! 도는 서버의 설정 경로로 등록 전 확인을 거쳐 저장한다(결정 24 · D4). 다른 프로그램의 토큰 파일을 읽지 않는다.

use price_inha::{
    AI_CHOICE_FILE, ANSWER_CLI, ANSWER_KEY, ANSWER_SKIP, SETUP_CLI_BILLING_WARNING,
    SETUP_CLI_LOGIN_HINT, SETUP_DONE, SETUP_FAILED, SETUP_FAILED_EXIT, SETUP_SKIPPED, SetupRefusal,
};

use crate::common::{
    AI_KEY, BAIT, Bench, ENV_KEY_VALUE, FakeCli, Login, Root, WRONG_AI_KEY, plain_files_with,
    registered,
};
use crate::run::{Env, Wizard, run_binary, strings};

/// `setup ai --stdin-answers --home <홈> --port <포트>`.
fn ai_args(root: &Root, port: u16) -> Vec<String> {
    strings(&[
        "setup",
        "ai",
        "--stdin-answers",
        "--home",
        root.home_str(),
        "--port",
        &port.to_string(),
    ])
}

// 검증: D4 V31
#[tokio::test(flavor = "multi_thread")]
async fn 로그인된_cli를_찾아_고르면_cli_방식이_된다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::In);
    let bench = Bench::start(&root.home, &[]).await;
    let ran = run_binary(
        Env::new(&root, &cli),
        ai_args(&root, bench.port),
        Some(format!("{ANSWER_CLI}\n")),
    )
    .await;

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.summary(),
        vec![("ai".to_string(), SETUP_DONE.to_string())],
        "구역 하나만 돌았다: {}",
        ran.stdout
    );
    let status = bench.status().await;
    assert_eq!(status["ai"]["method"], "cli", "{status}");
    assert_eq!(status["ai"]["cli"], "logged_in", "{status}");
    assert_eq!(
        status["ai"]["command"].as_str(),
        cli.bin.to_str(),
        "PATH에서 찾은 실행 파일: {status}"
    );
    assert!(registered(&status, "ai"), "{status}");
    assert!(
        cli.calls().iter().any(|c| c.starts_with("auth")),
        "로그인 확인을 거쳤다: {:?}",
        cli.calls()
    );
    ran.assert_absent(BAIT, "미끼 토큰");
    assert!(
        plain_files_with(&root.home, BAIT).is_empty(),
        "미끼가 보관 폴더로 옮겨지지 않는다"
    );
    bench.stop().await;
}

// 검증: D4
#[tokio::test(flavor = "multi_thread")]
async fn 로그인_안_됐으면_안내하고_엔터_뒤에_다시_확인한다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Out);
    let bench = Bench::start(&root.home, &[]).await;
    let env = Env::new(&root, &cli);
    let args = ai_args(&root, bench.port);
    let (ran, saw_hint) = tokio::task::spawn_blocking(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut wizard = Wizard::spawn(&env, &args, true);
        wizard.write(&format!("{ANSWER_CLI}\n"));
        // 안내가 나온 뒤에야 "다른 창에서" 로그인하고 Enter를 누른다.
        let saw_hint = wizard.wait_line(SETUP_CLI_LOGIN_HINT);
        cli.log_in();
        wizard.write("\n");
        (wizard.finish(), saw_hint)
    })
    .await
    .unwrap();

    assert!(saw_hint, "로그인 안내 한 줄이 나와야 한다: {}", ran.all());
    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.result_of("ai").as_deref(),
        Some(SETUP_DONE),
        "{}",
        ran.stdout
    );
    let status = bench.status().await;
    assert_eq!(status["ai"]["method"], "cli", "{status}");
    assert_eq!(
        status["ai"]["cli"], "logged_in",
        "다시 확인해 로그인됐다: {status}"
    );
    bench.stop().await;
}

// 검증: D4 V46 V47
#[tokio::test(flavor = "multi_thread")]
async fn api_키를_고르면_등록되고_출력과_기록_어디에도_키_글자가_없다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let bench = Bench::start(&root.home, &[]).await;
    let ran = run_binary(
        Env::new(&root, &cli),
        ai_args(&root, bench.port),
        Some(format!("{ANSWER_KEY}\n{AI_KEY}\n")),
    )
    .await;

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.result_of("ai").as_deref(),
        Some(SETUP_DONE),
        "{}",
        ran.stdout
    );
    let status = bench.status().await;
    assert!(
        registered(&status, "ai"),
        "가짜 Anthropic이 받은 키가 등록됐다: {status}"
    );
    assert_eq!(status["ai"]["method"], "api_key", "{status}");
    ran.assert_absent(AI_KEY, "넣은 API 키");
    assert!(
        plain_files_with(&root.home, AI_KEY).is_empty(),
        "비밀 보관 파일 밖(기록 · 설정 파일)에 키 글자가 없다: {:?}",
        plain_files_with(&root.home, AI_KEY)
    );
    assert!(
        !status.to_string().contains(AI_KEY),
        "설정 상태에도 키 값이 없다"
    );
    bench.stop().await;
}

// 검증: D4 V46
#[tokio::test(flavor = "multi_thread")]
async fn 틀린_키는_서버의_거절_글을_보이고_다시_묻는다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let bench = Bench::start(&root.home, &["--register-attempts", "10"]).await;
    let ran = run_binary(
        Env::new(&root, &cli),
        ai_args(&root, bench.port),
        Some(format!("{ANSWER_KEY}\n{WRONG_AI_KEY}\n{AI_KEY}\n")),
    )
    .await;

    assert!(
        ran.stdout.contains(&SetupRefusal::KeyRejected.to_string()),
        "서버의 거절 글을 보인다: {}",
        ran.stdout
    );
    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.result_of("ai").as_deref(),
        Some(SETUP_DONE),
        "{}",
        ran.stdout
    );
    assert!(
        registered(&bench.status().await, "ai"),
        "다시 물어 받은 키가 등록됐다"
    );
    ran.assert_absent(WRONG_AI_KEY, "틀린 키");
    ran.assert_absent(AI_KEY, "넣은 키");
    bench.stop().await;
}

// 검증: D4 V31
#[tokio::test(flavor = "multi_thread")]
async fn 시도_제한에_걸리면_더_묻지_않고_실패로_적는다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let bench = Bench::start(&root.home, &["--register-attempts", "1"]).await;
    // 첫 키는 틀리고, 다음 줄들은 맞는 키다. 시도 제한에 걸린 뒤에는 그 줄을 쓰지 않아야 한다.
    let ran = run_binary(
        Env::new(&root, &cli),
        ai_args(&root, bench.port),
        Some(format!(
            "{ANSWER_KEY}\n{WRONG_AI_KEY}\n{AI_KEY}\n{AI_KEY}\n"
        )),
    )
    .await;

    assert!(
        ran.stdout
            .contains(&SetupRefusal::TooManyAttempts.to_string()),
        "시도 제한의 거절 글: {}",
        ran.stdout
    );
    assert_eq!(
        ran.code,
        Some(i32::from(SETUP_FAILED_EXIT)),
        "{}",
        ran.all()
    );
    assert_eq!(
        ran.result_of("ai").as_deref(),
        Some(SETUP_FAILED),
        "{}",
        ran.stdout
    );
    assert!(
        !registered(&bench.status().await, "ai"),
        "제한 뒤의 키는 등록되지 않는다"
    );
    ran.assert_absent(AI_KEY, "넣은 키");
    bench.stop().await;
}

// 검증: D4
#[tokio::test(flavor = "multi_thread")]
async fn 건너뛰면_아무것도_저장하지_않는다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let bench = Bench::start(&root.home, &[]).await;
    let ran = run_binary(
        Env::new(&root, &cli),
        ai_args(&root, bench.port),
        Some(format!("{ANSWER_SKIP}\n")),
    )
    .await;

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert_eq!(
        ran.result_of("ai").as_deref(),
        Some(SETUP_SKIPPED),
        "{}",
        ran.stdout
    );
    assert!(!registered(&bench.status().await, "ai"));
    assert!(
        !root.home.join(AI_CHOICE_FILE).exists(),
        "고르지 않았으니 AI 연결 고르기를 저장하지 않는다"
    );
    bench.stop().await;
}

// 검증: D4
#[tokio::test(flavor = "multi_thread")]
async fn 환경에_api_키가_있는데_cli를_고르면_과금_경고를_한_줄_한다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::In);
    let bench = Bench::start(&root.home, &[]).await;
    let ran = run_binary(
        Env::new(&root, &cli).with("ANTHROPIC_API_KEY", ENV_KEY_VALUE),
        ai_args(&root, bench.port),
        Some(format!("{ANSWER_CLI}\n")),
    )
    .await;

    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert!(
        ran.stdout.contains(SETUP_CLI_BILLING_WARNING),
        "과금 경고: {}",
        ran.stdout
    );
    ran.assert_absent(ENV_KEY_VALUE, "환경의 API 키 값");
    assert_eq!(bench.status().await["ai"]["method"], "cli");
    bench.stop().await;
}
