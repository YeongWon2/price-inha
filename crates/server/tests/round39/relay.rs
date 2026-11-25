//! 38차 보강 — **중계 사칭**: 진짜 서버가 포트 A에서 돌고, 설정 포트 B를 차지한 가짜 서버가 세션은 스스로
//! 답하고(204 + 쿠키) CLI의 `whoami`는 진짜 서버로 중계해 그 답을 그대로 돌려준다. 증표 파일은 진짜 서버가 쓴
//! 그대로(0600)다. 요약값이 포트와 묶였으니 CLI(`setup` · `access-key` · `open --print`)는 포트 B로 계산해
//! 대조하고 멈춘다 — 키 · 주소가 나가지 않는다. 그리고 요약값 식은 포트가 다르면 다르다.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use price_inha::{
    NOT_THIS_HOME_SERVER, OPEN_NO_SERVER_EXIT, RUN_PROOF_DOMAIN, SETUP_FAILED_EXIT,
    WHOAMI_NONCE_MIN_LEN, WHOAMI_PATH, run_proof,
};

use crate::common::{
    CLIENT_ID, CLIENT_SECRET, Root, TOKEN_HOME, close, expected_proof, launch, read_proof,
    relay_whoami,
};
use crate::run::{Env, Ran, run_binary, strings};

/// 중계 사칭이 주는 세션 쿠키 값.
const RELAY_SESSION: &str = "zz-round39-relay-session";

/// 중계 사칭이 받은 요청 하나: 방법 · 경로 · 머리글과 본문을 합친 글.
struct Seen {
    method: String,
    path: String,
    text: String,
}

#[derive(Clone)]
struct Shared {
    /// 진짜 서버의 주소(포트 A).
    real: SocketAddr,
    seen: Arc<Mutex<Vec<Seen>>>,
    /// 중계한 `whoami`에 진짜 서버가 준 상태들.
    relayed: Arc<Mutex<Vec<u16>>>,
}

/// 포트 B의 중계 사칭(로컬 axum 서버).
struct Relay {
    port: u16,
    shared: Shared,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn answer(
    State(shared): State<Shared>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let path = uri.path().to_string();
    shared.seen.lock().unwrap().push(Seen {
        method: method.to_string(),
        path: path.clone(),
        text: format!("{uri} {headers:?} {}", String::from_utf8_lossy(&body)),
    });
    if method == Method::POST && path == pi_gate::SESSION_PATH {
        return (
            StatusCode::NO_CONTENT,
            [(
                "set-cookie",
                format!(
                    "{}={RELAY_SESSION}; Path=/; HttpOnly; SameSite=Strict",
                    pi_gate::SESSION_COOKIE
                ),
            )],
        )
            .into_response();
    }
    if method == Method::POST && path == WHOAMI_PATH {
        // 받은 본문(nonce)을 진짜 서버의 whoami로 그대로 넘기고 답을 그대로 돌려준다.
        let (status, text) = relay_whoami(shared.real, &String::from_utf8_lossy(&body)).await;
        shared.relayed.lock().unwrap().push(status);
        return (
            StatusCode::from_u16(status).unwrap(),
            [("content-type", "application/json")],
            text,
        )
            .into_response();
    }
    // 그 밖의 요청은 받아들였다는 200.
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        "{}".to_string(),
    )
        .into_response()
}

impl Relay {
    /// 빈 루프백 포트에 세운다. `whoami`는 `real`로 중계한다.
    async fn start(real: SocketAddr) -> Relay {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let shared = Shared {
            real,
            seen: Arc::new(Mutex::new(Vec::new())),
            relayed: Arc::new(Mutex::new(Vec::new())),
        };
        let router = Router::new().fallback(answer).with_state(shared.clone());
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Relay { port, shared, task }
    }

    /// 받은 요청 어디(주소 · 머리글 · 본문)에든 `needle`이 들었나.
    fn saw(&self, needle: &str) -> bool {
        self.shared
            .seen
            .lock()
            .unwrap()
            .iter()
            .any(|seen| seen.text.contains(needle))
    }

    /// 세션 청과 `whoami` 말고 받은 쓰기 요청(GET이 아닌 것)의 경로.
    fn writes(&self) -> Vec<String> {
        self.shared
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|seen| seen.method != "GET")
            .filter(|seen| seen.path != pi_gate::SESSION_PATH && seen.path != WHOAMI_PATH)
            .map(|seen| seen.path.clone())
            .collect()
    }

    /// 중계한 `whoami`에 진짜 서버가 준 상태들.
    fn relayed(&self) -> Vec<u16> {
        self.shared.relayed.lock().unwrap().clone()
    }
}

/// 진짜 서버(포트 A)와 그 앞의 중계 사칭(포트 B). 증표 파일은 진짜 서버가 쓴 그대로 0600이다.
async fn stage(root: &Root) -> (price_inha::App, Relay) {
    let app = launch(&root.home).await;
    let (_, mode) = read_proof(&root.home).expect("진짜 서버가 증표 파일을 썼다");
    assert_eq!(mode, 0o600, "증표 파일은 진짜 서버가 쓴 그대로 0600이다");
    let relay = Relay::start(app.local_addr()).await;
    assert_ne!(relay.port, app.local_addr().port(), "포트 A와 B는 다르다");
    (app, relay)
}

/// 중계 사칭에게 아무 비밀도 가지 않았고 세션 · 확인 말고는 쓰기 요청이 없으며, 멈춘 까닭이 증표 파일이 아니라
/// 포트로 묶인 요약값이다(중계가 실제로 일어났고 진짜 서버가 200으로 답했다).
fn nothing_sent(relay: &Relay, secrets: &[&str], case: &str, ran: &Ran) {
    for secret in secrets {
        assert!(
            !relay.saw(secret),
            "{case}: 중계 사칭이 받은 요청에 키 글자가 없다"
        );
    }
    assert_eq!(
        relay.writes(),
        Vec::<String>::new(),
        "{case}: 세션 · 확인 말고는 보낸 것이 없다: {}",
        ran.all()
    );
    let relayed = relay.relayed();
    assert!(
        !relayed.is_empty() && relayed.iter().all(|status| *status == 200),
        "{case}: CLI가 확인을 물었고 진짜 서버가 중계된 nonce에 답했다: {relayed:?}"
    );
}

// 검증: D4 V46 N8
#[tokio::test(flavor = "multi_thread")]
async fn 중계_사칭에는_setup이_증권_키를_보내지_않고_멈춘다() {
    let root = Root::new();
    let (app, relay) = stage(&root).await;
    let ran = run_binary(
        Env::new(&root),
        strings(&[
            "setup",
            "broker",
            "--stdin-answers",
            "--home",
            root.home_str(),
            "--port",
            &relay.port.to_string(),
        ]),
        Some(format!("{CLIENT_ID}\n{CLIENT_SECRET}\n")),
    )
    .await;
    assert_eq!(
        ran.code,
        Some(i32::from(SETUP_FAILED_EXIT)),
        "{}",
        ran.all()
    );
    assert!(
        ran.all().contains(NOT_THIS_HOME_SERVER),
        "까닭을 말한다: {}",
        ran.all()
    );
    nothing_sent(&relay, &[CLIENT_ID, CLIENT_SECRET], "setup", &ran);
    drop(relay);
    close(app).await;
}

// 검증: V41 D2 N8
#[tokio::test(flavor = "multi_thread")]
async fn 중계_사칭에는_access_key가_키를_만들게_하지_않는다() {
    let root = Root::new();
    let (app, relay) = stage(&root).await;
    let ran = run_binary(
        Env::new(&root),
        strings(&[
            "access-key",
            "--home",
            root.home_str(),
            "--port",
            &relay.port.to_string(),
        ]),
        None,
    )
    .await;
    assert_eq!(ran.code, Some(1), "{}", ran.all());
    assert!(
        ran.all().contains(NOT_THIS_HOME_SERVER),
        "까닭을 말한다: {}",
        ran.all()
    );
    nothing_sent(&relay, &[], "access-key", &ran);
    assert!(
        app.store().access_key().unwrap().is_none(),
        "진짜 서버의 저장소에도 키가 생기지 않는다"
    );
    drop(relay);
    close(app).await;
}

// 검증: D1 V46
#[tokio::test(flavor = "multi_thread")]
async fn 중계_사칭이면_open은_주소를_내지_않는다() {
    let root = Root::new();
    let (app, relay) = stage(&root).await;
    let ran = run_binary(
        Env::new(&root),
        strings(&[
            "open",
            "--print",
            "--home",
            root.home_str(),
            "--port",
            &relay.port.to_string(),
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
    assert!(
        ran.all().contains(NOT_THIS_HOME_SERVER),
        "까닭을 말한다: {}",
        ran.all()
    );
    assert!(
        !ran.stdout.contains("http://"),
        "화면 주소를 내지 않는다: {}",
        ran.stdout
    );
    nothing_sent(&relay, &[], "open", &ran);
    drop(relay);
    close(app).await;
}

// 검증: N8
#[test]
fn 포트가_다르면_같은_증표와_nonce라도_요약값이_다르다() {
    let asked: String = std::iter::repeat_n('9', WHOAMI_NONCE_MIN_LEN).collect();
    let (a, b) = (41_001_u16, 41_002_u16);
    let at_a = run_proof(TOKEN_HOME, &asked, a);
    let at_b = run_proof(TOKEN_HOME, &asked, b);
    assert_ne!(at_a, at_b, "포트가 다르면 요약값이 다르다");
    assert_eq!(at_a, expected_proof(TOKEN_HOME, &asked, a), "식에 포트 A");
    assert_eq!(at_b, expected_proof(TOKEN_HOME, &asked, b), "식에 포트 B");
    // 포트는 십진수 글자 그대로(앞 0 없이): 1과 65535의 끝.
    assert_eq!(
        run_proof(TOKEN_HOME, &asked, 1),
        expected_proof(TOKEN_HOME, &asked, 1)
    );
    assert_eq!(
        run_proof(TOKEN_HOME, &asked, u16::MAX),
        expected_proof(TOKEN_HOME, &asked, u16::MAX)
    );
    assert!(
        RUN_PROOF_DOMAIN.ends_with("v2"),
        "도메인 글자의 판 번호는 v2다: {RUN_PROOF_DOMAIN}"
    );
}
