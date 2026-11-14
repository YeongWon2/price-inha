//! pi-secrets 2차 작업 테스트. 기준: `docs/dev/modules/secrets.md`의 "2차 작업" 1~4번.
//!
//! - 보관 폴더는 모두 임시 폴더다. 실제 홈 폴더와 프로세스 환경 변수를 건드리지 않는다.
//! - 확인기는 가짜다. 바깥 서버를 부르지 않는다.
//! - 비밀 값은 진짜 키처럼 보이지 않는 글자만 쓴다.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use pi_secrets::{
    AUTH_FILE, Credentials, ENV_FILE, KeyKind, MIN_SECRET_CHARS, REDACT_PREVIOUS_KEPT, REDACTED,
    SecretStore, SecretsError, Verifier, VerifyFailure, resolve_dir,
};
use tempfile::tempdir;

const ID_1: &str = "시험용-아이디-0001";
const SECRET_1: &str = "시험용-비밀-값-0001";
const ID_2: &str = "시험용-아이디-0002";
const SECRET_2: &str = "시험용-비밀-값-0002";
const AI_KEY: &str = "시험용-에이아이-값-0001";
const ACCESS: &str = "시험용-접근-토큰-0001";
const REFRESH: &str = "시험용-갱신-토큰-0001";

fn broker(id: &str, secret: &str) -> Credentials {
    Credentials::new()
        .with("client_id", id)
        .with("client_secret", secret)
}

fn api_key(value: &str) -> Credentials {
    Credentials::new().with("api_key", value)
}

fn oauth(access: &str, refresh: &str) -> Credentials {
    Credentials::new()
        .with("access_token", access)
        .with("refresh_token", refresh)
}

/// 언제나 통과하는 확인기.
struct Pass;

impl Verifier for Pass {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

async fn register_ok(store: &SecretStore, kind: KeyKind, values: Credentials) {
    store
        .register(kind, values, &Pass)
        .await
        .expect("확인을 지난 값은 등록된다");
}

/// 값이 든 글. 값 앞뒤에 다른 글자가 있다.
fn wrap(value: &str) -> String {
    format!("앞|{value}|뒤")
}

/// 값 자리가 가려진 글.
fn wrapped_redacted() -> String {
    wrap(REDACTED)
}

/// `store`가 `value`를 그대로 · URL 인코딩 · Base64 모양 모두 가리는지 본다.
fn assert_redacted(store: &SecretStore, value: &str, what: &str) {
    assert_eq!(
        store.redact(&wrap(value)),
        wrapped_redacted(),
        "{what}: 값이 가려지지 않았다"
    );
    let url = utf8_percent_encode(value, NON_ALPHANUMERIC).to_string();
    assert!(
        !store.redact(&wrap(&url)).contains(&url),
        "{what}: URL 인코딩된 값이 가려지지 않았다"
    );
    let b64 = STANDARD.encode(value);
    assert!(
        !store.redact(&wrap(&b64)).contains(&b64),
        "{what}: Base64 값이 가려지지 않았다"
    );
}

/// `store`가 `value`를 더는 가리지 않는지 본다(가리기 대상에서 빠졌다).
fn assert_not_redacted(store: &SecretStore, value: &str, what: &str) {
    assert_eq!(
        store.redact(&wrap(value)),
        wrap(value),
        "{what}: 가리기 대상에서 빠졌어야 한다"
    );
}

/// 확인하는 동안 보관소의 `redact`를 불러 보는 가짜 확인기.
///
/// 받은 값마다, 그리고 따로 정해 둔 값(`also`)마다 `redact(값이 든 글)`의 결과를 적어 둔다.
/// `block_dir`이 있으면 확인을 마치기 직전에 그 폴더를 지우고 같은 자리에 파일을 둔다
/// (확인은 지났는데 저장이 실패하는 경우를 만든다).
struct WatchingVerifier {
    store: Arc<SecretStore>,
    result: Result<(), VerifyFailure>,
    also: Vec<String>,
    block_dir: Option<PathBuf>,
    calls: AtomicUsize,
    /// (본 값, 그 값이 든 글을 `redact`한 결과)
    seen: Mutex<Vec<(String, String)>>,
}

impl WatchingVerifier {
    fn new(store: &Arc<SecretStore>, result: Result<(), VerifyFailure>) -> Self {
        Self {
            store: Arc::clone(store),
            result,
            also: Vec::new(),
            block_dir: None,
            calls: AtomicUsize::new(0),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn also(mut self, value: &str) -> Self {
        self.also.push(value.to_owned());
        self
    }

    fn blocking(mut self, dir: &Path) -> Self {
        self.block_dir = Some(dir.to_path_buf());
        self
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn seen(&self) -> Vec<(String, String)> {
        self.seen.lock().expect("가짜 확인기 잠금").clone()
    }

    /// 확인하는 동안 `values`가 모두 한 번씩, 가려진 채로 보였는지 단언한다.
    fn assert_saw_redacted(&self, values: &[&str]) {
        assert_eq!(self.calls(), 1, "확인기는 한 번 불린다");
        let seen = self.seen();
        for value in values {
            let found = seen
                .iter()
                .find(|(seen_value, _)| seen_value == value)
                .unwrap_or_else(|| panic!("확인기가 값을 받지 못했다: {value}"));
            assert_eq!(
                found.1,
                wrapped_redacted(),
                "확인기가 도는 동안 값이 가려지지 않았다"
            );
        }
    }
}

impl Verifier for WatchingVerifier {
    async fn verify(&self, kind: KeyKind, values: &Credentials) -> Result<(), VerifyFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut probes: Vec<String> = kind
            .fields()
            .iter()
            .filter_map(|field| values.get(field).map(|v| v.expose().to_owned()))
            .collect();
        probes.extend(self.also.iter().cloned());
        let looked: Vec<(String, String)> = probes
            .into_iter()
            .map(|value| {
                let out = self.store.redact(&wrap(&value));
                (value, out)
            })
            .collect();
        self.seen.lock().expect("가짜 확인기 잠금").extend(looked);
        if let Some(dir) = &self.block_dir {
            fs::remove_dir_all(dir).expect("보관 폴더 지우기");
            fs::write(dir, "폴더가 아니다").expect("폴더 자리에 파일 두기");
        }
        self.result
    }
}

fn open_shared(dir: &Path) -> Arc<SecretStore> {
    Arc::new(SecretStore::open(dir).expect("열기"))
}

fn is_registered(store: &SecretStore, kind: KeyKind) -> bool {
    store
        .status()
        .expect("상태 보기")
        .into_iter()
        .any(|s| s.kind == kind && s.registered)
}

fn value_of(store: &SecretStore, kind: KeyKind, field: &str) -> Option<String> {
    store
        .read(kind)
        .expect("읽기")
        .and_then(|c| c.get(field).map(|v| v.expose().to_owned()))
}

/// 키 파일 안의 `from`을 `to`로 바꿔 쓴다(사람이 손으로 고친 것처럼).
fn rewrite_by_hand(path: &Path, from: &str, to: &str) {
    let text = fs::read_to_string(path).expect("키 파일 읽기");
    assert_eq!(text.matches(from).count(), 1, "바꿀 값이 파일에 한 번 있다");
    fs::write(path, text.replace(from, to)).expect("키 파일 고쳐 쓰기");
}

/// `n`번째 돌림 값. 서로 겹치지 않고 길이가 같다.
fn rotated(label: &str, n: usize) -> String {
    format!("시험용-{label}-돌림-값-{n:04}")
}

// ───────────────────────── 2차 1번: 확인 중인 값도 가린다 ─────────────────────────

// 검증: N8, E38, C3
#[tokio::test]
async fn 확인기가_도는_동안에도_확인_중인_값을_가린다() {
    let tmp = tempdir().unwrap();
    let store = open_shared(tmp.path());
    let verifier = WatchingVerifier::new(&store, Ok(()));

    store
        .register(KeyKind::Broker, broker(ID_1, SECRET_1), &verifier)
        .await
        .expect("확인을 지난 값은 등록된다");

    verifier.assert_saw_redacted(&[ID_1, SECRET_1]);
    assert_redacted(&store, ID_1, "등록 뒤");
    assert_redacted(&store, SECRET_1, "등록 뒤");
}

// 검증: N8, E38, E4
#[tokio::test]
async fn 확인이_거부된_값도_확인_중과_그_뒤에_계속_가린다() {
    let tmp = tempdir().unwrap();
    let store = open_shared(tmp.path());
    let verifier = WatchingVerifier::new(&store, Err(VerifyFailure::Rejected));

    let result = store
        .register(KeyKind::Broker, broker(ID_1, SECRET_1), &verifier)
        .await;

    assert!(matches!(
        result,
        Err(SecretsError::VerifyFailed(VerifyFailure::Rejected))
    ));
    verifier.assert_saw_redacted(&[ID_1, SECRET_1]);
    assert_redacted(&store, ID_1, "거부된 뒤");
    assert_redacted(&store, SECRET_1, "거부된 뒤");
    // 저장하지 않은 것은 그대로다(E4).
    assert!(!is_registered(&store, KeyKind::Broker));
    assert!(!tmp.path().join(ENV_FILE).exists());
}

// 검증: N8, E38, E4
#[tokio::test]
async fn 서버에_닿지_못한_값도_확인_중과_그_뒤에_계속_가린다() {
    let tmp = tempdir().unwrap();
    let store = open_shared(tmp.path());
    let verifier = WatchingVerifier::new(&store, Err(VerifyFailure::Unreachable));

    let result = store
        .register(KeyKind::AiApiKey, api_key(AI_KEY), &verifier)
        .await;

    assert!(matches!(
        result,
        Err(SecretsError::VerifyFailed(VerifyFailure::Unreachable))
    ));
    verifier.assert_saw_redacted(&[AI_KEY]);
    assert_redacted(&store, AI_KEY, "닿지 못한 뒤");
    assert!(!is_registered(&store, KeyKind::AiApiKey));
}

// 검증: N8, E38, E4
#[tokio::test]
async fn auth_json에_둘_토큰도_확인에_실패한_뒤_계속_가린다() {
    let tmp = tempdir().unwrap();
    let store = open_shared(tmp.path());
    let verifier = WatchingVerifier::new(&store, Err(VerifyFailure::Rejected));

    let result = store
        .register(KeyKind::AiOAuth, oauth(ACCESS, REFRESH), &verifier)
        .await;

    assert!(matches!(result, Err(SecretsError::VerifyFailed(_))));
    verifier.assert_saw_redacted(&[ACCESS, REFRESH]);
    assert_redacted(&store, ACCESS, "거부된 뒤");
    assert_redacted(&store, REFRESH, "거부된 뒤");
    assert!(!tmp.path().join(AUTH_FILE).exists());
}

// 검증: N8, E38, D3
#[tokio::test]
async fn 확인은_지났지만_저장에_실패한_값도_계속_가린다() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("보관");
    let store = open_shared(&dir);
    // 확인기가 끝나기 직전에 보관 폴더 자리를 파일로 바꿔, 저장이 실패하게 한다.
    let verifier = WatchingVerifier::new(&store, Ok(())).blocking(&dir);

    let result = store
        .register(KeyKind::Broker, broker(ID_1, SECRET_1), &verifier)
        .await;

    assert!(matches!(result, Err(SecretsError::Io { .. })));
    verifier.assert_saw_redacted(&[ID_1, SECRET_1]);
    assert_redacted(&store, ID_1, "저장에 실패한 뒤");
    assert_redacted(&store, SECRET_1, "저장에 실패한 뒤");
    assert_eq!(fs::read_to_string(&dir).unwrap(), "폴더가 아니다");
}

// 검증: N8, E38, C16, E4
#[tokio::test]
async fn 다시_등록이_확인에_실패해도_옛_값과_새_값을_모두_가린다() {
    let tmp = tempdir().unwrap();
    let store = open_shared(tmp.path());
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    let verifier = WatchingVerifier::new(&store, Err(VerifyFailure::Rejected))
        .also(ID_1)
        .also(SECRET_1);

    let result = store
        .register(KeyKind::Broker, broker(ID_2, SECRET_2), &verifier)
        .await;

    assert!(matches!(result, Err(SecretsError::VerifyFailed(_))));
    // 확인하는 동안: 확인 중인 새 값도, 지금 등록된 옛 값도 가려진다.
    verifier.assert_saw_redacted(&[ID_2, SECRET_2, ID_1, SECRET_1]);
    for value in [ID_1, SECRET_1, ID_2, SECRET_2] {
        assert_redacted(&store, value, "다시 등록이 거부된 뒤");
    }
    // 저장된 것은 옛 값 그대로다.
    assert_eq!(
        value_of(&store, KeyKind::Broker, "client_secret").as_deref(),
        Some(SECRET_1)
    );
}

// ───────────────────────── 2차 2번: 짧은 값은 읽을 때도 받지 않는다 ─────────────────────────

// 검증: N8, E38, D3
#[tokio::test]
async fn env에_손으로_적힌_여덟_자_미만_값은_등록된_것으로_보지_않는다() {
    let long = "plain-test-value-0001";
    let eight = "여덟글자인시험값";
    let seven = "일곱글자시험값";
    assert_eq!(eight.chars().count(), MIN_SECRET_CHARS);
    assert_eq!(seven.chars().count(), MIN_SECRET_CHARS - 1);

    let tmp = tempdir().unwrap();
    let env = tmp.path().join(ENV_FILE);
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Data, api_key(long)).await;

    // 딱 8자로 고친 값은 받는다(경계).
    rewrite_by_hand(&env, long, eight);
    assert_eq!(
        value_of(&store, KeyKind::Data, "api_key").as_deref(),
        Some(eight)
    );
    assert!(is_registered(&store, KeyKind::Data));

    // 7자로 고친 값은 받지 않는다: 열려 있던 보관소도, 새로 연 보관소도.
    rewrite_by_hand(&env, eight, seven);
    let reopened = SecretStore::open(tmp.path()).unwrap();
    for (what, s) in [("열려 있던 보관소", &store), ("다시 연 보관소", &reopened)] {
        assert!(!is_registered(s, KeyKind::Data), "{what}: 등록됨으로 본다");
        let read = s.read(KeyKind::Data);
        assert!(matches!(read, Ok(None)), "{what}: 짧은 값을 내주면 안 된다");
    }
}

// 검증: N8, E38, C3
#[tokio::test]
async fn 한_칸만_짧아도_그_종류는_등록되지_않은_것이고_옆_종류는_그대로다() {
    let id = "plain-test-id-0001";
    let secret = "plain-test-secret-0001";
    let seven = "일곱글자시험값";

    let tmp = tempdir().unwrap();
    {
        let store = SecretStore::open(tmp.path()).unwrap();
        register_ok(&store, KeyKind::Broker, broker(id, secret)).await;
        register_ok(&store, KeyKind::AiApiKey, api_key(AI_KEY)).await;
    }
    rewrite_by_hand(&tmp.path().join(ENV_FILE), secret, seven);

    let store = SecretStore::open(tmp.path()).unwrap();

    assert!(!is_registered(&store, KeyKind::Broker));
    assert!(matches!(store.read(KeyKind::Broker), Ok(None)));
    // 같은 파일의 다른 종류는 그대로 읽힌다.
    assert!(is_registered(&store, KeyKind::AiApiKey));
    assert_eq!(
        value_of(&store, KeyKind::AiApiKey, "api_key").as_deref(),
        Some(AI_KEY)
    );
}

// 검증: N8, E38, D3
#[tokio::test]
async fn auth_json에_손으로_적힌_여덟_자_미만_토큰도_등록된_것으로_보지_않는다() {
    let seven = "일곱글자시험값";

    let tmp = tempdir().unwrap();
    {
        let store = SecretStore::open(tmp.path()).unwrap();
        register_ok(&store, KeyKind::AiOAuth, oauth(ACCESS, REFRESH)).await;
    }
    rewrite_by_hand(&tmp.path().join(AUTH_FILE), REFRESH, seven);

    let store = SecretStore::open(tmp.path()).unwrap();

    assert!(!is_registered(&store, KeyKind::AiOAuth));
    assert!(matches!(store.read(KeyKind::AiOAuth), Ok(None)));
}

// ───────────────────────── 2차 3번: 보관 폴더는 절대 경로여야 한다 ─────────────────────────

// 검증: D3
#[test]
fn 환경_변수가_상대_경로면_홈_폴더가_있어도_거부한다() {
    for relative in ["상대/보관-폴더", "./보관-폴더", "../보관-폴더", "."] {
        let result = resolve_dir(Some(OsStr::new(relative)), Some(Path::new("/시험/홈")));
        assert!(
            matches!(result, Err(SecretsError::HomeNotFound)),
            "상대 경로를 받았다: {relative}"
        );
    }
}

// 검증: D3
#[test]
fn 홈_폴더가_빈_글자면_거부한다() {
    let result = resolve_dir(None, Some(Path::new("")));
    assert!(matches!(result, Err(SecretsError::HomeNotFound)));
}

// 검증: D3
#[test]
fn 홈_폴더가_상대_경로면_거부한다() {
    for relative in ["상대/홈", "./홈", "."] {
        let result = resolve_dir(None, Some(Path::new(relative)));
        assert!(
            matches!(result, Err(SecretsError::HomeNotFound)),
            "상대 경로를 받았다: {relative}"
        );
    }
}

// 검증: D3
#[test]
fn 환경_변수도_홈_폴더도_빈_글자면_거부한다() {
    let result = resolve_dir(Some(OsStr::new("")), Some(Path::new("")));
    assert!(matches!(result, Err(SecretsError::HomeNotFound)));
}

// 검증: D3
#[test]
fn 절대_경로인_환경_변수는_홈_폴더가_이상해도_그대로_받는다() {
    // 이미 통과하는 것이 정상이다: 2차 규칙이 멀쩡한 절대 경로까지 막지 않는지 붙잡아 둔다.
    let dir = resolve_dir(Some(OsStr::new("/시험/보관")), Some(Path::new(""))).unwrap();
    assert_eq!(dir, PathBuf::from("/시험/보관"));
}

// 검증: D3
#[test]
fn 상대_경로로는_보관소를_열_수_없고_현재_폴더_아래에_아무것도_만들지_않는다() {
    let relative = PathBuf::from(format!("시험용-상대-보관-{}", std::process::id()));
    assert!(relative.is_relative());
    assert!(!relative.exists(), "시험 전에 같은 이름이 없어야 한다");

    let result = SecretStore::open(&relative);
    let created = relative.exists();
    if created {
        // 실패해도 현재 폴더에 흔적을 남기지 않는다.
        fs::remove_dir_all(&relative).expect("만들어진 폴더 치우기");
    }

    assert!(matches!(result, Err(SecretsError::HomeNotFound)));
    assert!(!created, "현재 폴더 아래에 보관 폴더를 만들었다");
}

// ───────────────────────── 2차 4번: 가리기 대상에 끝이 있다 ─────────────────────────

// 검증: N1, N8, E38, C16
#[tokio::test]
async fn 같은_칸을_여러_번_다시_등록하면_오래된_값은_가리기_대상에서_빠진다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    let total = REDACT_PREVIOUS_KEPT + 5;
    let values: Vec<String> = (0..total).map(|n| rotated("자료", n)).collect();

    for value in &values {
        register_ok(&store, KeyKind::Data, api_key(value)).await;
    }

    let current = total - 1;
    let oldest_kept = current - REDACT_PREVIOUS_KEPT;
    // 지금 값과 바로 앞의 정한 개수는 가려진다.
    assert_redacted(&store, &values[current], "지금 값");
    for value in &values[oldest_kept..current] {
        assert_redacted(&store, value, "정한 개수 안의 앞 값");
    }
    // 그보다 오래된 값은 더는 쥐고 있지 않다.
    for value in &values[..oldest_kept] {
        assert_not_redacted(&store, value, "정한 개수를 넘은 옛 값");
    }
    // 저장된 것은 지금 값이다.
    assert_eq!(
        value_of(&store, KeyKind::Data, "api_key").as_deref(),
        Some(values[current].as_str())
    );
}

// 검증: N1, N8, E38
#[tokio::test]
async fn 개수는_칸마다_따로_세고_바뀌지_않은_칸과_다른_종류는_계속_가린다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    register_ok(&store, KeyKind::AiApiKey, api_key(AI_KEY)).await;

    // 접근 토큰만 돌린다. 갱신 토큰은 내내 같은 값이다.
    let total = REDACT_PREVIOUS_KEPT + 5;
    let access: Vec<String> = (0..total).map(|n| rotated("접근", n)).collect();
    for value in &access {
        register_ok(&store, KeyKind::AiOAuth, oauth(value, REFRESH)).await;
    }

    let current = total - 1;
    let oldest_kept = current - REDACT_PREVIOUS_KEPT;
    for value in &access[oldest_kept..] {
        assert_redacted(&store, value, "지금 값과 정한 개수 안의 앞 값");
    }
    for value in &access[..oldest_kept] {
        assert_not_redacted(&store, value, "정한 개수를 넘은 옛 접근 토큰");
    }
    // 같은 종류의 바뀌지 않은 칸, 한 번만 등록한 다른 종류는 밀려나지 않는다.
    for value in [REFRESH, ID_1, SECRET_1, AI_KEY] {
        assert_redacted(&store, value, "지금 등록된 값");
    }
}

// 검증: N1, N8, E38, E4
#[tokio::test]
async fn 가리기_대상이_찬_뒤에도_지금_값과_확인_중인_값은_가린다() {
    let tmp = tempdir().unwrap();
    let store = open_shared(tmp.path());
    let total = REDACT_PREVIOUS_KEPT + 5;
    let values: Vec<String> = (0..total).map(|n| rotated("자료", n)).collect();
    for value in &values {
        register_ok(&store, KeyKind::Data, api_key(value)).await;
    }
    let current = values[total - 1].as_str();

    // 확인에 실패하는 다시 등록을 정한 개수보다 많이 되풀이한다.
    let attempts = REDACT_PREVIOUS_KEPT + 3;
    let mut last_attempt = String::new();
    for n in 0..attempts {
        let attempt = rotated("실패", n);
        let verifier = WatchingVerifier::new(&store, Err(VerifyFailure::Rejected)).also(current);

        let result = store
            .register(KeyKind::Data, api_key(&attempt), &verifier)
            .await;

        assert!(matches!(result, Err(SecretsError::VerifyFailed(_))));
        // 확인하는 동안: 확인 중인 값과 지금 등록된 값이 모두 가려진다.
        verifier.assert_saw_redacted(&[attempt.as_str(), current]);
        last_attempt = attempt;
    }

    // 실패가 아무리 쌓여도 지금 등록된 값은 밀려나지 않는다.
    assert_redacted(&store, current, "지금 등록된 값");
    // 방금 확인에 실패한 값도 가려진다(2차 1번).
    assert_redacted(&store, &last_attempt, "방금 확인에 실패한 값");
    assert_eq!(
        value_of(&store, KeyKind::Data, "api_key").as_deref(),
        Some(current)
    );
}
