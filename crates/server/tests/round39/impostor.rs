//! 사칭: 설정의 포트에서 세션 모양(`POST /api/session`에 204 + 세션 쿠키)만 흉내 내는 가짜 서버가 답할 때,
//! `setup` · `access-key` · `open --print`는 증표로 확인하지 못하면 **세션 · 확인 말고는 아무것도 보내지 않고**
//! 멈춘다. 출력에 [`NOT_THIS_HOME_SERVER`], 종료 코드는 지금의 "그 포트에 다른 프로그램"과 같다(38차 3 · 4번).
//! 증표 파일이 없을 때 · 권한이 넓을 때(사칭이 그 증표로 맞게 답해도) · 다른 서버의 증표일 때 모두.

use pi_store::Store;
use price_inha::{
    ANSWER_KEY, DB_FILE, NOT_THIS_HOME_SERVER, OPEN_NO_SERVER_EXIT, SETUP_FAILED_EXIT,
    SETUP_START_SERVER_QUESTION,
};

use crate::common::{
    AI_KEY, BROKEN, CLIENT_ID, CLIENT_SECRET, Impostor, PLANTED_KEY, Root, TOKEN_HOME, Whoami,
    free_port, write_proof,
};
use crate::run::{Env, Ran, Wizard, run_binary, strings};

/// `setup <구역> --stdin-answers --home <홈> --port <포트>`.
fn setup_args(section: &str, root: &Root, port: u16) -> Vec<String> {
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

/// 사칭에게 아무 비밀도 가지 않았고 세션 · 확인 말고는 쓰기 요청이 없다.
async fn nothing_sent(impostor: &Impostor, secrets: &[&str], case: &str, ran: &Ran) {
    for secret in secrets {
        assert!(
            !impostor.saw(secret).await,
            "{case}: 사칭 서버가 받은 요청에 키 글자가 없다"
        );
        assert!(
            !ran.all().contains(secret),
            "{case}: 출력에도 키 글자가 없다"
        );
    }
    assert_eq!(
        impostor.writes().await,
        Vec::<String>::new(),
        "{case}: 세션 · 확인 말고는 보낸 것이 없다(설정 · 키 경로 0건)"
    );
}

// 검증: D4 V46 V47 N8
#[tokio::test(flavor = "multi_thread")]
async fn 사칭_서버에는_setup이_증권_키를_보내지_않고_멈춘다() {
    for case in BROKEN {
        let root = Root::new();
        let impostor = case.stage(&root).await;
        let ran = run_binary(
            Env::new(&root),
            setup_args("broker", &root, impostor.port()),
            Some(format!("{CLIENT_ID}\n{CLIENT_SECRET}\n")),
        )
        .await;
        let label = format!("{case:?}");
        assert_eq!(
            ran.code,
            Some(i32::from(SETUP_FAILED_EXIT)),
            "{label}: {}",
            ran.all()
        );
        assert!(
            ran.all().contains(NOT_THIS_HOME_SERVER),
            "{label}: 까닭을 말한다: {}",
            ran.all()
        );
        nothing_sent(&impostor, &[CLIENT_ID, CLIENT_SECRET], &label, &ran).await;
    }
}

// 검증: D4 V46 N8
#[tokio::test(flavor = "multi_thread")]
async fn 확인_경로를_모르는_사칭에는_setup이_ai_키를_보내지_않는다() {
    let root = Root::new();
    write_proof(&root, TOKEN_HOME, 0o600);
    let impostor = Impostor::start(Whoami::Silent).await;
    let ran = run_binary(
        Env::new(&root),
        setup_args("ai", &root, impostor.port()),
        Some(format!("{ANSWER_KEY}\n{AI_KEY}\n")),
    )
    .await;
    assert_eq!(
        ran.code,
        Some(i32::from(SETUP_FAILED_EXIT)),
        "{}",
        ran.all()
    );
    assert!(ran.all().contains(NOT_THIS_HOME_SERVER), "{}", ran.all());
    nothing_sent(&impostor, &[AI_KEY], "확인 경로 없음", &ran).await;
}

// 검증: D4 V46 N8
#[tokio::test(flavor = "multi_thread")]
async fn 마법사가_띄운_서버_자리에_사칭이_답해도_키를_보내지_않는다() {
    let root = Root::new();
    let port = free_port();
    let env = Env::new(&root);
    let args = setup_args("broker", &root, port);
    // 처음에는 아무도 듣지 않아 마법사가 띄울지 묻는다.
    let (wizard, asked) = tokio::task::spawn_blocking(move || {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let wizard = Wizard::spawn(&env, &args, true);
        let asked = wizard.wait_line(SETUP_START_SERVER_QUESTION);
        (wizard, asked)
    })
    .await
    .unwrap();
    assert!(asked, "띄울지 묻는다");
    // 묻는 사이 사칭이 그 포트를 먼저 차지한다(마법사가 띄운 서버는 그 자리를 얻지 못한다).
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    let impostor = Impostor::start_on(listener, Whoami::Silent).await;
    let ran = tokio::task::spawn_blocking(move || {
        let mut wizard = wizard;
        wizard.write(&format!("y\n{CLIENT_ID}\n{CLIENT_SECRET}\n"));
        wizard.finish()
    })
    .await
    .unwrap();
    assert_eq!(
        ran.code,
        Some(i32::from(SETUP_FAILED_EXIT)),
        "자식 프로세스를 띄웠다는 것만으로 믿지 않는다: {}",
        ran.all()
    );
    nothing_sent(
        &impostor,
        &[CLIENT_ID, CLIENT_SECRET],
        "마법사가 띄운 자리",
        &ran,
    )
    .await;
}

// 검증: V41 D2 N8
#[tokio::test(flavor = "multi_thread")]
async fn 사칭_서버에는_access_key가_키를_만들게_하지_않는다() {
    for case in BROKEN {
        let root = Root::new();
        let impostor = case.stage(&root).await;
        // 이 보관 폴더에는 저장소가 있고 아직 접속 키가 없다.
        drop(Store::open(root.home.join(DB_FILE)).unwrap());
        let ran = run_binary(
            Env::new(&root),
            strings(&[
                "access-key",
                "--home",
                root.home_str(),
                "--port",
                &impostor.port().to_string(),
            ]),
            None,
        )
        .await;
        let label = format!("{case:?}");
        assert_eq!(ran.code, Some(1), "{label}: {}", ran.all());
        assert!(
            ran.all().contains(NOT_THIS_HOME_SERVER),
            "{label}: 까닭을 말한다: {}",
            ran.all()
        );
        assert!(
            !ran.all().contains(PLANTED_KEY),
            "{label}: 사칭이 내민 키를 보여 주지 않는다"
        );
        nothing_sent(&impostor, &[], &label, &ran).await;
        let store = Store::open(root.home.join(DB_FILE)).unwrap();
        assert!(
            store.access_key().unwrap().is_none(),
            "{label}: 이 보관 폴더에 키가 생기지 않는다"
        );
    }
}

// 검증: D1 V46
#[tokio::test(flavor = "multi_thread")]
async fn 사칭_서버면_open은_주소를_내지_않는다() {
    for case in BROKEN {
        let root = Root::new();
        let impostor = case.stage(&root).await;
        let ran = run_binary(
            Env::new(&root),
            strings(&[
                "open",
                "--print",
                "--home",
                root.home_str(),
                "--port",
                &impostor.port().to_string(),
            ]),
            None,
        )
        .await;
        let label = format!("{case:?}");
        assert_eq!(
            ran.code,
            Some(i32::from(OPEN_NO_SERVER_EXIT)),
            "{label}: {}",
            ran.all()
        );
        assert!(
            ran.all().contains(NOT_THIS_HOME_SERVER),
            "{label}: 까닭을 말한다: {}",
            ran.all()
        );
        assert!(
            !ran.stdout.contains("http://"),
            "{label}: 화면 주소를 내지 않는다: {}",
            ran.stdout
        );
        nothing_sent(&impostor, &[], &label, &ran).await;
    }
}
