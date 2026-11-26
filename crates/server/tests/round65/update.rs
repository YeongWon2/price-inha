//! 새 판 알리기(조정자 추천 "알리기만", 원본 Hermes `install-and-cli.md` 2.5: 배경에서 묻고 24시간 기억 · 알림만 ·
//! 자동 설치 없음):
//! - 확인 주소(릴리스의 `LATEST` 파일)에서 판 글자 하나를 읽어 지금 판과 견준다. **내려받거나 바꾸지 않는다.**
//! - 답은 보관 폴더의 확인 기록 파일에 남고, `doctor`는 그 기록만 읽어 한 줄로 알린다(바깥에 묻지 않는다, 문제로 세지
//!   않는다).
//! - 주소는 `https`(또는 시험의 루프백 `http`)만. 판 글자가 아니면 기록하지 않는다.
//! - 시연 모드 · 확인 주소가 없는 조립(개발 · 시험 빌드)에서는 확인을 띄우지 않는다.

use price_inha::{
    CheckItem, Os, Service, UPDATE_CHECK_FILE, UpdateCheckError, VERSION, check_for_update,
    doctor, is_newer_version, read_update_notice, spawn_update_check, update_check_url,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{ScriptedRunner, plain_settings, setup};

const NOW: i64 = 1_795_000_000_000;

async fn latest_server(body: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/releases/latest/download/LATEST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;
    server
}

fn latest_url(server: &MockServer) -> String {
    format!("{}/releases/latest/download/LATEST", server.uri())
}

// 검증: D1
#[test]
fn 판_견주기는_숫자_차례로_한다() {
    assert!(is_newer_version("v0.10.0", "0.9.3"));
    assert!(is_newer_version("1.0.0", "0.99.99"));
    assert!(!is_newer_version("v0.1.0", "0.1.0"));
    assert!(!is_newer_version("v0.0.9", "0.1.0"));
    assert!(!is_newer_version("최신", "0.1.0"), "판 글자가 아니면 새 판이 아니다");
}

// 검증: D1
#[tokio::test]
async fn 새_판이_있으면_기록하고_진단이_알리기만_한다() {
    let server = latest_server("v99.0.0\n").await;
    let home = tempfile::tempdir().unwrap();
    let notice = check_for_update(&latest_url(&server), home.path(), NOW)
        .await
        .expect("확인");
    assert_eq!(notice.latest, "v99.0.0");
    assert_eq!(notice.current, VERSION);
    assert!(notice.newer);
    assert_eq!(server.received_requests().await.unwrap().len(), 1, "한 번만 묻는다");
    assert!(home.path().join(UPDATE_CHECK_FILE).is_file());
    let entries: Vec<_> = std::fs::read_dir(home.path()).unwrap().collect();
    assert_eq!(entries.len(), 1, "확인 기록 말고는 아무것도 받지 않는다");
    assert_eq!(read_update_notice(home.path()), Some(notice));

    let defs = tempfile::tempdir().unwrap();
    let runner = ScriptedRunner::ok();
    let service = Service::new(setup(Os::Linux, home.path(), defs.path(), &[]), &runner);
    let report = doctor(&plain_settings(home.path()), &service);
    let line = report.check(CheckItem::Update).expect("새 판 줄");
    assert!(line.ok, "알리기만 한다(문제로 세지 않는다): {line:?}");
    assert!(line.what.contains("v99.0.0") && line.what.contains(VERSION), "{line:?}");
    assert!(line.what.contains("install"), "다시 설치하는 길을 알린다: {line:?}");
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "진단은 바깥에 묻지 않는다"
    );
}

// 검증: D1
#[tokio::test]
async fn 같은_판이면_새_판이_아니고_판_글자가_아니면_기록하지_않는다() {
    let same = latest_server(&format!("v{VERSION}")).await;
    let home = tempfile::tempdir().unwrap();
    let notice = check_for_update(&latest_url(&same), home.path(), NOW)
        .await
        .unwrap();
    assert!(!notice.newer);

    let junk = latest_server("<html>not found</html>").await;
    let other = tempfile::tempdir().unwrap();
    let error = check_for_update(&latest_url(&junk), other.path(), NOW)
        .await
        .expect_err("판 글자가 아니다");
    assert_eq!(error, UpdateCheckError::NotAVersion);
    assert!(read_update_notice(other.path()).is_none());
    assert!(!other.path().join(UPDATE_CHECK_FILE).exists());
}

// 검증: D1 D2
#[tokio::test]
async fn 바깥_http_주소는_묻지_않고_거부한다() {
    let home = tempfile::tempdir().unwrap();
    let error = check_for_update("http://example.com/LATEST", home.path(), NOW)
        .await
        .expect_err("https가 아니다");
    assert_eq!(error, UpdateCheckError::InsecureUrl);
    assert!(!home.path().join(UPDATE_CHECK_FILE).exists());
}

// 검증: D1
#[tokio::test]
async fn 시연_모드와_주소_없는_조립은_확인을_띄우지_않는다() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        update_check_url().is_none(),
        "시험 빌드에는 바깥 확인 주소가 없다"
    );
    assert!(spawn_update_check(home.path().to_path_buf(), None, false).is_none());
    let server = latest_server("v99.0.0").await;
    let url: &'static str = Box::leak(latest_url(&server).into_boxed_str());
    assert!(
        spawn_update_check(home.path().to_path_buf(), Some(url), true).is_none(),
        "시연 모드는 바깥으로 나가지 않는다"
    );
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(server.received_requests().await.unwrap().is_empty());
}
