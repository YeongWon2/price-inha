//! `price-inha open [화면] --print`: 도는 서버의 주소를 찾아 화면 주소를 낸다(시험은 브라우저를 열지 않는 길).
//! 주소에 비밀값이 없다. 도는 서버가 없으면 까닭과 `price-inha run` 안내, 종료 코드 1(결정 24 · D1).

use price_inha::{CONFIG_FATAL_EXIT, OPEN_NO_SERVER, OPEN_NO_SERVER_EXIT, OPEN_SCREENS};

use crate::common::{Bench, FakeCli, Login, Root, free_port};
use crate::run::{Env, run_binary, strings};

// 검증: D1 V46
#[tokio::test(flavor = "multi_thread")]
async fn 도는_서버가_있으면_화면_주소를_낸다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let bench = Bench::start(&root.home, &[]).await;
    let port = bench.port.to_string();
    for screen in OPEN_SCREENS {
        let ran = run_binary(
            Env::new(&root, &cli),
            strings(&[
                "open",
                screen.as_str(),
                "--print",
                "--home",
                root.home_str(),
                "--port",
                &port,
            ]),
            None,
        )
        .await;
        assert_eq!(ran.code, Some(0), "{}: {}", screen.as_str(), ran.all());
        let url = format!("http://127.0.0.1:{port}{}", screen.path());
        let line = ran
            .stdout
            .lines()
            .find(|line| line.contains("http://"))
            .unwrap_or_else(|| panic!("주소 한 줄: {}", ran.all()));
        assert_eq!(line.trim(), url, "{}", screen.as_str());
        for word in ["key", "token", "session", "?", "#"] {
            assert!(!line.contains(word), "주소에 비밀 자리가 없다: {line}");
        }
    }
    // 화면을 주지 않으면 첫 화면.
    let ran = run_binary(
        Env::new(&root, &cli),
        strings(&[
            "open",
            "--print",
            "--home",
            root.home_str(),
            "--port",
            &port,
        ]),
        None,
    )
    .await;
    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert!(
        ran.stdout.contains(&format!("http://127.0.0.1:{port}/")),
        "{}",
        ran.all()
    );
    bench.stop().await;
}

// 검증: D1
#[tokio::test(flavor = "multi_thread")]
async fn 도는_서버가_없으면_run을_안내하고_종료_코드_1() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let ran = run_binary(
        Env::new(&root, &cli),
        strings(&[
            "open",
            "--print",
            "--home",
            root.home_str(),
            "--port",
            &free_port().to_string(),
        ]),
        None,
    )
    .await;
    assert_eq!(
        ran.code,
        Some(i32::from(OPEN_NO_SERVER_EXIT)),
        "{}",
        ran.all()
    );
    assert!(ran.all().contains(OPEN_NO_SERVER), "{}", ran.all());
    assert!(!ran.stdout.contains("http://"), "열 주소가 없다");
    assert!(!root.home.exists(), "보관 폴더를 만들지 않는다");
}

// 검증: D1
#[tokio::test(flavor = "multi_thread")]
async fn 모르는_화면은_설정_오류로_거부한다() {
    let root = Root::new();
    let cli = FakeCli::new(Login::Missing);
    let ran = run_binary(
        Env::new(&root, &cli),
        strings(&["open", "nowhere", "--print"]),
        None,
    )
    .await;
    assert_eq!(
        ran.code,
        Some(i32::from(CONFIG_FATAL_EXIT)),
        "{}",
        ran.all()
    );
}
