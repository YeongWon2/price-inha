//! 0번 9차 재검토에서 넘어온 것: 접속 키(서비스 정의의 주소로 도는 서버 찾기 · 직접 만든 길의 안내),
//! 진단(읽기만 하고 파일을 만들지 않기 · 잠긴 저장소의 접속 키는 "확인하지 못함"), 기록 파일(띄우기 실패 까닭).
//!
//! 서버는 루프백 빈 포트에 시연 모드로 띄운다. 다른 기기는 전달 헤더로 흉내 낸다. 서비스 정의는 임시 폴더에
//! 가짜 실행기로 등록한다(실제 서비스 관리자를 부르지 않는다).

use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use pi_secrets::{Credentials, KeyKind, KeySlot, SecretStore, Verifier, VerifyFailure};
use pi_store::{ACCESS_KEY_SALT_MIN_BYTES, AccessKeyHash, Store, UnixMillis};
use price_inha::{
    ACCESS_KEY_PATH, ACCESS_LOG_PATH, AccessKeyRequest, App, CheckItem, CommandLine, CommandOutput,
    CommandRunner, DB_FILE, InstallOptions, Launch, Os, Service, ServiceSetup, Settings,
    TradingWiring, Wiring, access_key, access_key_with, doctor,
};
use serde_json::{Value, json};

/// 한 걸음의 기다림 상한.
const STEP: Duration = Duration::from_secs(15);
/// 프록시를 거쳐 온 요청의 전달 헤더(있기만 해도 다른 기기다).
const PROXIED: (&str, &str) = ("X-Forwarded-For", "100.64.0.7");
/// 시험용 가짜 키 값(진짜 키처럼 보이지 않는다).
const AI_VALUE: &str = "vkzq-round17-ai-value-mxjd";

const ROTATE: AccessKeyRequest = AccessKeyRequest {
    rotate: true,
    remote_name: None,
};

/// 모든 명령에 성공으로 답하는 가짜 실행기.
struct OkRunner;

impl CommandRunner for OkRunner {
    fn run(&self, _command: &CommandLine) -> io::Result<CommandOutput> {
        Ok(CommandOutput {
            success: true,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// 보관 폴더 `home`을 가리키고 `--port <port>`로 띄우는 서비스 재료(정의 폴더는 임시 폴더).
fn setup(home: &Path, definitions: &Path, port: u16) -> ServiceSetup {
    ServiceSetup {
        os: Os::MacOs,
        exe: PathBuf::from("/opt/price-inha/bin/price-inha"),
        home: home.to_path_buf(),
        definition_dir: definitions.to_path_buf(),
        uid: 501,
        path_env: "/usr/local/bin:/usr/bin:/bin".to_string(),
        run_args: vec!["--port".to_string(), port.to_string()],
    }
}

fn settings(home: &Path, bind: &str, port: u16) -> Settings {
    let port = port.to_string();
    let args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--bind",
        bind,
        "--port",
        &port,
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    Settings::parse(&args, &|_| None, None).unwrap()
}

/// 보관 폴더 `home`으로 서버를 루프백 빈 포트에 띄운다(시연 모드, 팀 · 바깥 잇기 없음).
async fn launch(home: &Path) -> App {
    let args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--demo",
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    let settings = Settings::parse(&args, &|_| None, None).unwrap();
    let wiring = Wiring {
        cli_locations: Vec::new(),
        ..Wiring::default()
    };
    let launch = Launch {
        trading: TradingWiring::default(),
        team: None,
        secrets: None,
    };
    tokio::time::timeout(STEP, App::launch(settings, wiring, launch))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// HTTP 응답 하나: 상태 · JSON 본문 · `Set-Cookie`의 첫 마디.
struct Reply {
    status: u16,
    body: Value,
    cookie: Option<String>,
}

async fn send(request: reqwest::RequestBuilder, remote: bool) -> Reply {
    let request = if remote {
        request.header(PROXIED.0, PROXIED.1)
    } else {
        request
    };
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let cookie = reply
        .headers()
        .get("set-cookie")
        .and_then(|value| value.to_str().ok())
        .map(|raw| raw.split(';').next().unwrap().trim().to_string());
    let bytes = reply.bytes().await.expect("본문을 읽어야 한다");
    Reply {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        cookie,
    }
}

/// 세션을 청한다. `key`가 있으면 접속 키를 싣고 다른 기기로 보인다.
async fn enter(addr: SocketAddr, key: Option<&str>) -> Reply {
    let origin = origin_of(addr);
    let mut request = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin);
    if let Some(key) = key {
        request = request
            .header("Content-Type", "application/json")
            .body(json!({ pi_gate::ACCESS_KEY_FIELD: key }).to_string());
    }
    send(request, key.is_some()).await
}

/// 다른 기기에서 그 쿠키로 접속 기록을 청한 답의 상태.
async fn remote_get(addr: SocketAddr, cookie: &str) -> u16 {
    let request = reqwest::Client::new()
        .get(format!("{}{ACCESS_LOG_PATH}", origin_of(addr)))
        .header("Cookie", cookie);
    send(request, true).await.status
}

/// 그 컴퓨터의 화면 경로로 접속 키를 발급받는다.
async fn issue_on_screen(addr: SocketAddr) -> String {
    let cookie = enter(addr, None)
        .await
        .cookie
        .expect("그 컴퓨터는 키 없이 세션을 받는다");
    let origin = origin_of(addr);
    let request = reqwest::Client::new()
        .post(format!("{origin}{ACCESS_KEY_PATH}"))
        .header("Origin", &origin)
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .body("{}");
    let reply = send(request, false).await;
    assert_eq!(reply.status, 200, "화면 경로로 발급이 되어야 한다");
    reply.body[pi_gate::ACCESS_KEY_FIELD]
        .as_str()
        .expect("답에 키가 있어야 한다")
        .to_string()
}

/// 지금 아무도 듣지 않는 루프백 포트.
fn refused_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// 폴더 아래 모든 파일 이름(정렬).
fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// 보관 폴더를 0700, 안의 파일을 0600으로 좁힌다(유닉스).
#[cfg(unix)]
fn narrow(home: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700)).unwrap();
    for entry in std::fs::read_dir(home).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
}

/// 유닉스가 아니면 권한을 바꾸지 않는다.
#[cfg(not(unix))]
fn narrow(_home: &Path) {}

/// 무엇이든 받는 시험용 확인자(바깥으로 나가지 않는다).
struct AcceptAll;

impl Verifier for AcceptAll {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_slot(
        &self,
        _slot: KeySlot,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

/// 저장소가 있는 보관 폴더를 만든다(0700 · 0600). `with_key`면 접속 키도 넣어 둔다.
fn stored_home(home: &Path, with_key: bool) {
    drop(SecretStore::open(home).unwrap());
    let store = Store::open(home.join(DB_FILE)).unwrap();
    if with_key {
        let hash = AccessKeyHash {
            sha256: [7; 32],
            salt: vec![3; ACCESS_KEY_SALT_MIN_BYTES],
        };
        store.set_access_key(hash, UnixMillis(1_000)).unwrap();
    }
    drop(store);
    narrow(home);
}

// 검증: V41 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 포트를_주지_않아도_서비스_정의의_포트로_도는_서버를_찾아_앞_세대_세션을_끊는다() {
    let home = tempfile::tempdir().unwrap();
    let app = launch(home.path()).await;
    let addr = app.local_addr();
    let first = issue_on_screen(addr).await;
    let old = enter(addr, Some(&first))
        .await
        .cookie
        .expect("맞는 키로 원격 세션을 받는다");
    assert_eq!(
        remote_get(addr, &old).await,
        200,
        "원격 세션으로 들어와 있다"
    );

    // 서비스는 `--port <도는 포트>`로 등록했는데 `access-key`는 다른(아무도 듣지 않는) 포트로 부른다.
    let defs = tempfile::tempdir().unwrap();
    let runner = OkRunner;
    let service = Service::new(setup(home.path(), defs.path(), addr.port()), &runner);
    let options = InstallOptions {
        start_now: false,
        dry_run: false,
    };
    service.install(options).unwrap();
    let cli = settings(home.path(), "127.0.0.1", refused_port());

    let shown = access_key_with(&cli, &ROTATE, &service)
        .await
        .expect("정의의 포트에서 도는 서버에게 키를 바꾸게 한다");
    assert_eq!(shown.generation(), 2, "세대는 1만 오른다");
    assert!(
        shown.text().contains("끊깁니다"),
        "실제로 끊기는 길의 안내다"
    );
    assert_eq!(
        remote_get(addr, &old).await,
        401,
        "앞 세대의 원격 세션은 끊겨야 한다"
    );
    assert_eq!(
        enter(addr, Some(&first)).await.status,
        401,
        "앞 키는 맞지 않는다"
    );
    assert!(
        enter(addr, Some(shown.key())).await.cookie.is_some(),
        "새 키로는 들어온다"
    );
    tokio::time::timeout(STEP, app.shutdown())
        .await
        .unwrap()
        .unwrap();
}

// 검증: V41 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_홈을_가리키는_정의는_쓰지_않고_직접_만든_길은_찾지_못했다고_알린다() {
    // 다른 홈의 서버가 돌고 있고, 서비스 정의는 그 다른 홈을 가리킨다.
    let other = tempfile::tempdir().unwrap();
    let app = launch(other.path()).await;
    let defs = tempfile::tempdir().unwrap();
    let runner = OkRunner;
    let theirs = Service::new(
        setup(other.path(), defs.path(), app.local_addr().port()),
        &runner,
    );
    let options = InstallOptions {
        start_now: false,
        dry_run: false,
    };
    theirs.install(options).unwrap();

    let home = tempfile::tempdir().unwrap();
    let cli = settings(home.path(), "127.0.0.1", refused_port());
    let shown = access_key_with(&cli, &AccessKeyRequest::default(), &theirs)
        .await
        .expect("이 홈으로 도는 서버가 없으면 직접 만든다");
    assert_eq!(shown.generation(), 1);
    assert!(
        shown
            .text()
            .contains("도는 서버를 찾지 못해 직접 만들었습니다"),
        "직접 만든 길의 안내: {}",
        shown.text().replace(shown.key(), "<키>")
    );
    let theirs_store = app.store();
    assert!(
        theirs_store.access_key().unwrap().is_none(),
        "다른 홈의 서버에는 키를 만들게 하지 않는다"
    );

    // 정의 없이 부르는 길(`access_key`)도 직접 만들었으면 같은 안내다.
    let plain = tempfile::tempdir().unwrap();
    let cli = settings(plain.path(), "127.0.0.1", refused_port());
    let direct = access_key(&cli, &AccessKeyRequest::default())
        .await
        .expect("처음은 만든다");
    assert!(
        direct
            .text()
            .contains("도는 서버를 찾지 못해 직접 만들었습니다"),
        "직접 만든 길의 안내"
    );
    tokio::time::timeout(STEP, app.shutdown())
        .await
        .unwrap()
        .unwrap();
}

// 검증: D1 D2
#[tokio::test]
async fn 진단은_저장소를_읽기만_하고_보관_폴더에_파일을_만들지_않는다() {
    let home = tempfile::tempdir().unwrap();
    stored_home(home.path(), false);
    let before = file_names(home.path());
    assert!(
        !before
            .iter()
            .any(|name| name.ends_with("-wal") || name.ends_with("-shm")),
        "닫힌 저장소에는 곁 파일이 없다: {before:?}"
    );

    let defs = tempfile::tempdir().unwrap();
    let runner = OkRunner;
    let service = Service::new(setup(home.path(), defs.path(), 8123), &runner);
    let report = doctor(&settings(home.path(), "127.0.0.1", 0), &service);
    let database = report.check(CheckItem::Database).expect("저장소 항목");
    assert!(database.ok, "건강한 저장소다: {}", database.what);
    assert_eq!(
        file_names(home.path()),
        before,
        "진단 뒤 폴더의 파일 목록이 그대로다(-shm · -wal을 만들지 않는다)"
    );
}

// 검증: D1 D2
#[tokio::test]
async fn 저장소가_잠겨_있으면_접속_키를_없다고_하지_않고_확인하지_못함으로_둔다() {
    let home = tempfile::tempdir().unwrap();
    stored_home(home.path(), true);
    let file = home.path().join(DB_FILE);

    // 다른 프로그램이 저장소를 혼자 잡고 쓰는 중이다.
    let holder = rusqlite::Connection::open(&file).unwrap();
    holder
        .pragma_update(None, "locking_mode", "EXCLUSIVE")
        .unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE;").unwrap();
    let probe =
        rusqlite::Connection::open_with_flags(&file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .and_then(|conn| {
                conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
                    row.get::<_, i64>(0)
                })
            });
    assert!(probe.is_err(), "시험 준비: 다른 연결은 읽지 못해야 한다");

    let defs = tempfile::tempdir().unwrap();
    let runner = OkRunner;
    let service = Service::new(setup(home.path(), defs.path(), 8123), &runner);
    let report = doctor(&settings(home.path(), "0.0.0.0", 0), &service);
    let check = report.check(CheckItem::AccessKey).expect("접속 키 항목");
    assert!(
        check.ok,
        "키가 있는데 읽지 못했다고 ✗로 알리지 않는다: {}",
        check.what
    );
    assert!(
        check.what.contains("확인하지 못"),
        "확인하지 못했다고 알린다: {}",
        check.what
    );
    drop(holder);
}

// 검증: D1 N8
#[tokio::test]
async fn 띄우기_실패_까닭이_기록_파일에도_남고_비밀_글자는_없다() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("pi-home");
    let secrets = SecretStore::open(&home).unwrap();
    let ai = Credentials::new().with("api_key", AI_VALUE);
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    drop(secrets);
    let log = home.join("logs").join("server.log");

    // 이미 쓰는 포트로 띄우면 실패한다.
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port().to_string();
    let output = Command::new(env!("CARGO_BIN_EXE_price-inha"))
        .args(["run", "--demo", "--port", &port, "--log-file"])
        .arg(&log)
        .env_clear()
        .env(price_inha::ENV_HOME, &home)
        .env("HOME", root.path().join("user"))
        .env("USERPROFILE", root.path().join("user"))
        .output()
        .unwrap();
    assert!(!output.status.success(), "쓰는 포트로는 뜨지 못한다");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reason = stderr
        .lines()
        .filter_map(|line| line.strip_prefix("price-inha: "))
        .next_back()
        .expect("표준 오류에 실패 까닭이 있다")
        .trim()
        .to_string();
    assert!(!reason.is_empty());

    let written = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        written.contains(&reason),
        "예약 작업은 표준 오류를 볼 수 없으니 실패 까닭이 기록 파일에도 남는다({reason}): {written}"
    );
    assert!(!written.contains(AI_VALUE), "기록 파일에 키 글자가 없다");
    assert!(!stderr.contains(AI_VALUE), "표준 오류에도 없다");
}
