//! pi-secrets 공개 API 테스트. 기준: `docs/dev/modules/secrets.md`.
//!
//! - 보관 폴더는 모두 임시 폴더다. 실제 홈 폴더와 프로세스 환경 변수를 건드리지 않는다.
//! - 확인기는 가짜다. 바깥 서버를 부르지 않는다.
//! - 비밀 값은 진짜 키처럼 보이지 않는 글자만 쓴다.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use pi_secrets::{
    AUTH_FILE, Credentials, DEFAULT_DIR_NAME, ENV_FILE, HOME_ENV, KeyKind, KeyStatus,
    MIN_SECRET_CHARS, REDACTED, Registered, SecretStore, SecretString, SecretsError, VerifyFailure,
    resolve_dir,
};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

const ID_1: &str = "시험용-아이디-0001";
const SECRET_1: &str = "시험용-비밀-값-0001";
const ID_2: &str = "시험용-아이디-0002";
const SECRET_2: &str = "시험용-비밀-값-0002";
const AI_KEY: &str = "시험용-에이아이-값-0001";
const DATA_KEY: &str = "시험용-자료-값-0001";
const ACCESS: &str = "시험용-접근-토큰-0001";
const REFRESH: &str = "시험용-갱신-토큰-0001";

/// 테스트가 함께 쓰는 도구.
#[cfg(test)]
mod support {
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use base64::Engine;
    use base64::engine::general_purpose::STANDARD_NO_PAD;
    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
    use pi_secrets::{
        Credentials, KeyKind, REDACTED, Registered, SecretStore, Verifier, VerifyFailure,
    };
    use tempfile::tempdir;

    use super::{ACCESS, AI_KEY, DATA_KEY, ID_1, REFRESH, SECRET_1};

    /// 확인기가 받은 것: 종류와 (칸 이름, 값)들.
    pub(super) type Seen = (KeyKind, Vec<(String, String)>);

    /// 가짜 확인기. 정해 둔 결과를 돌려주고, 불린 횟수와 받은 값을 적어 둔다.
    pub(super) struct FakeVerifier {
        result: Result<(), VerifyFailure>,
        calls: AtomicUsize,
        seen: Mutex<Vec<Seen>>,
    }

    impl FakeVerifier {
        pub(super) fn ok() -> Self {
            Self::with(Ok(()))
        }

        pub(super) fn failing(failure: VerifyFailure) -> Self {
            Self::with(Err(failure))
        }

        fn with(result: Result<(), VerifyFailure>) -> Self {
            Self {
                result,
                calls: AtomicUsize::new(0),
                seen: Mutex::new(Vec::new()),
            }
        }

        pub(super) fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        pub(super) fn seen(&self) -> Vec<Seen> {
            self.seen.lock().expect("가짜 확인기 잠금").clone()
        }
    }

    impl Verifier for FakeVerifier {
        async fn verify(&self, kind: KeyKind, values: &Credentials) -> Result<(), VerifyFailure> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let pairs = kind
                .fields()
                .iter()
                .filter_map(|f| {
                    values
                        .get(f)
                        .map(|v| ((*f).to_string(), v.expose().to_string()))
                })
                .collect();
            self.seen
                .lock()
                .expect("가짜 확인기 잠금")
                .push((kind, pairs));
            self.result
        }
    }

    pub(super) fn broker(id: &str, secret: &str) -> Credentials {
        Credentials::new()
            .with("client_id", id)
            .with("client_secret", secret)
    }

    pub(super) fn api_key(value: &str) -> Credentials {
        Credentials::new().with("api_key", value)
    }

    pub(super) fn oauth(access: &str, refresh: &str) -> Credentials {
        Credentials::new()
            .with("access_token", access)
            .with("refresh_token", refresh)
    }

    /// 통과하는 확인기로 등록한다.
    pub(super) async fn register_ok(
        store: &SecretStore,
        kind: KeyKind,
        values: Credentials,
    ) -> Registered {
        store
            .register(kind, values, &FakeVerifier::ok())
            .await
            .expect("확인을 지난 값은 등록된다")
    }

    /// 네 종류를 모두 등록한다.
    pub(super) async fn register_all(store: &SecretStore) {
        register_ok(store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
        register_ok(store, KeyKind::AiApiKey, api_key(AI_KEY)).await;
        register_ok(store, KeyKind::AiOAuth, oauth(ACCESS, REFRESH)).await;
        register_ok(store, KeyKind::Data, api_key(DATA_KEY)).await;
    }

    pub(super) const ALL_VALUES: [&str; 6] = [ID_1, SECRET_1, AI_KEY, ACCESS, REFRESH, DATA_KEY];

    /// 칸의 값을 꺼낸다.
    pub(super) fn value_of(store: &SecretStore, kind: KeyKind, field: &str) -> Option<String> {
        store
            .read(kind)
            .expect("읽기")
            .and_then(|c| c.get(field).map(|v| v.expose().to_string()))
    }

    pub(super) fn registered_kinds(store: &SecretStore) -> Vec<KeyKind> {
        store
            .status()
            .expect("상태 보기")
            .into_iter()
            .filter(|s| s.registered)
            .map(|s| s.kind)
            .collect()
    }

    /// 폴더 안의 이름들(정렬).
    pub(super) fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[cfg(unix)]
    pub(super) fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .expect("권한을 볼 파일이 있어야 한다")
            .permissions()
            .mode()
            & 0o777
    }

    #[cfg(unix)]
    pub(super) fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("권한 바꾸기");
    }

    /// `글`에 `값`이 그대로 · URL 인코딩 · Base64 어느 모양으로도 없어야 한다.
    pub(super) fn assert_no_value(text: &str, value: &str, what: &str) {
        assert!(!text.contains(value), "{what}에 값이 그대로 있다");
        let url = utf8_percent_encode(value, NON_ALPHANUMERIC).to_string();
        assert!(!text.contains(&url), "{what}에 URL 인코딩된 값이 있다");
        let b64 = STANDARD_NO_PAD.encode(value);
        assert!(!text.contains(&b64), "{what}에 Base64 값이 있다");
    }

    /// 값이 `.env`를 다녀와도 글자 하나 바뀌지 않는지 본다. 옆 종류도 그대로여야 한다.
    pub(super) async fn assert_env_round_trip(value: &str) {
        let tmp = tempdir().expect("임시 폴더");
        {
            let store = SecretStore::open(tmp.path()).expect("열기");
            register_ok(&store, KeyKind::AiApiKey, api_key(AI_KEY)).await;
            register_ok(&store, KeyKind::Broker, broker(ID_1, value)).await;
            register_ok(&store, KeyKind::Data, api_key(value)).await;

            assert_eq!(
                value_of(&store, KeyKind::Data, "api_key").as_deref(),
                Some(value),
                "쓴 보관소에서 읽은 값이 다르다: {value:?}"
            );
        }

        let store = SecretStore::open(tmp.path()).expect("다시 열기");
        assert_eq!(
            value_of(&store, KeyKind::Data, "api_key").as_deref(),
            Some(value),
            "다시 열어 읽은 값이 다르다: {value:?}"
        );
        assert_eq!(
            value_of(&store, KeyKind::Broker, "client_secret").as_deref(),
            Some(value),
            "다시 열어 읽은 값이 다르다: {value:?}"
        );
        assert_eq!(
            value_of(&store, KeyKind::Broker, "client_id").as_deref(),
            Some(ID_1),
            "옆 칸이 바뀌었다: {value:?}"
        );
        assert_eq!(
            value_of(&store, KeyKind::AiApiKey, "api_key").as_deref(),
            Some(AI_KEY),
            "옆 종류가 바뀌었다: {value:?}"
        );
        assert_eq!(
            registered_kinds(&store),
            vec![KeyKind::Broker, KeyKind::AiApiKey, KeyKind::Data],
            "등록 상태가 달라졌다: {value:?}"
        );
        assert_eq!(
            store.redact(&format!("앞|{value}|뒤")),
            format!("앞|{REDACTED}|뒤"),
            "다녀온 값을 가리지 못했다: {value:?}"
        );
    }
}

use support::{
    ALL_VALUES, FakeVerifier, api_key, assert_env_round_trip, assert_no_value, broker, names_in,
    oauth, register_all, register_ok, registered_kinds, value_of,
};
#[cfg(unix)]
use support::{mode_of, set_mode};

// ───────────────────────── 보관 폴더 (만드는 것 1) ─────────────────────────

// 검증: D3
#[test]
fn 환경_변수_값이_있으면_그_경로가_보관_폴더다() {
    let dir = resolve_dir(
        Some(OsStr::new("/시험/다른-보관-폴더")),
        Some(Path::new("/시험/홈")),
    )
    .unwrap();
    assert_eq!(dir, PathBuf::from("/시험/다른-보관-폴더"));
    assert_eq!(HOME_ENV, "PRICE_INHA_HOME");
}

// 검증: D3
#[test]
fn 환경_변수가_없으면_홈_폴더_아래_기본_폴더다() {
    let dir = resolve_dir(None, Some(Path::new("/시험/홈"))).unwrap();
    assert_eq!(dir, Path::new("/시험/홈").join(".price-inha"));
    assert_eq!(DEFAULT_DIR_NAME, ".price-inha");
}

// 검증: D3
#[test]
fn 환경_변수가_비어_있으면_없는_것으로_본다() {
    let dir = resolve_dir(Some(OsStr::new("")), Some(Path::new("/시험/홈"))).unwrap();
    assert_eq!(dir, Path::new("/시험/홈").join(".price-inha"));
}

// 검증: D3
#[test]
fn 환경_변수도_홈_폴더도_없으면_오류다() {
    let result = resolve_dir(None, None);
    assert!(matches!(result, Err(SecretsError::HomeNotFound)));
}

// 검증: D3
#[tokio::test]
async fn 생성자에_준_폴더에만_파일을_쓴다() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("보관");
    let store = SecretStore::open(&dir).unwrap();
    assert_eq!(store.dir(), dir.as_path());

    register_all(&store).await;

    assert!(dir.join(ENV_FILE).is_file());
    assert!(dir.join(AUTH_FILE).is_file());
    assert_eq!(names_in(tmp.path()), vec!["보관".to_string()]);
}

// 검증: D3, N8
#[cfg(unix)]
#[tokio::test]
async fn 새로_만든_보관_폴더는_0700이다() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("새-보관");
    let store = SecretStore::open(&dir).unwrap();
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;

    assert!(dir.is_dir());
    assert_eq!(mode_of(&dir), 0o700);
}

// 검증: D3, N8
#[cfg(unix)]
#[tokio::test]
async fn 이미_있는_넓은_보관_폴더는_0700으로_좁힌다() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("넓은-보관");
    fs::create_dir(&dir).unwrap();
    set_mode(&dir, 0o755);

    let store = SecretStore::open(&dir).unwrap();
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;

    assert_eq!(mode_of(&dir), 0o700);
}

// ───────────────────────── 키 파일 (만드는 것 2) ─────────────────────────

// 검증: D3, C3
#[tokio::test]
async fn api_키는_env에_oauth_토큰은_auth_json에_둔다() {
    assert_eq!(ENV_FILE, ".env");
    assert_eq!(AUTH_FILE, "auth.json");
    assert_eq!(KeyKind::Broker.file_name(), ENV_FILE);
    assert_eq!(KeyKind::AiApiKey.file_name(), ENV_FILE);
    assert_eq!(KeyKind::Data.file_name(), ENV_FILE);
    assert_eq!(KeyKind::AiOAuth.file_name(), AUTH_FILE);

    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();

    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    assert_eq!(names_in(tmp.path()), vec![ENV_FILE.to_string()]);
    let env = fs::read_to_string(tmp.path().join(ENV_FILE)).unwrap();
    assert!(env.contains(SECRET_1));

    register_ok(&store, KeyKind::AiOAuth, oauth(ACCESS, REFRESH)).await;
    assert_eq!(
        names_in(tmp.path()),
        vec![ENV_FILE.to_string(), AUTH_FILE.to_string()]
    );
    let auth = fs::read_to_string(tmp.path().join(AUTH_FILE)).unwrap();
    assert!(auth.contains(ACCESS));
    assert!(auth.contains(REFRESH));

    let env = fs::read_to_string(tmp.path().join(ENV_FILE)).unwrap();
    assert!(
        !env.contains(ACCESS) && !env.contains(REFRESH),
        "OAuth 토큰이 .env에 들어갔다"
    );
    assert!(
        !auth.contains(SECRET_1) && !auth.contains(ID_1),
        "API 키가 auth.json에 들어갔다"
    );
}

// 검증: D3
#[tokio::test]
async fn env는_이름과_값을_등호로_이은_줄이다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    let plain = "plain-test-value-0001";
    register_ok(&store, KeyKind::Data, api_key(plain)).await;

    let env = fs::read_to_string(tmp.path().join(ENV_FILE)).unwrap();
    let lines: Vec<&str> = env
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    assert!(!lines.is_empty());
    let mut found = false;
    for line in lines {
        let (name, value) = line.split_once('=').expect("줄마다 `이름=값` 모양이다");
        assert!(!name.trim().is_empty(), "이름이 비었다");
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        if value == plain {
            found = true;
        }
    }
    assert!(found, "값이 든 `이름=값` 줄이 없다");
}

// 검증: D3, N8
#[cfg(unix)]
#[tokio::test]
async fn env_파일은_0600이다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    assert_eq!(mode_of(&tmp.path().join(ENV_FILE)), 0o600);

    register_ok(&store, KeyKind::Data, api_key(DATA_KEY)).await;
    register_ok(&store, KeyKind::Broker, broker(ID_2, SECRET_2)).await;
    assert_eq!(mode_of(&tmp.path().join(ENV_FILE)), 0o600);
}

// 검증: D3, N8
#[cfg(unix)]
#[tokio::test]
async fn auth_json_파일은_0600이다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::AiOAuth, oauth(ACCESS, REFRESH)).await;
    assert_eq!(mode_of(&tmp.path().join(AUTH_FILE)), 0o600);

    register_ok(
        &store,
        KeyKind::AiOAuth,
        oauth(ACCESS, "시험용-갱신-토큰-0002"),
    )
    .await;
    assert_eq!(mode_of(&tmp.path().join(AUTH_FILE)), 0o600);
}

// 검증: D3, N8
#[cfg(unix)]
#[tokio::test]
async fn 넓은_권한의_env는_읽기_전에_0600으로_좁힌다() {
    let tmp = tempdir().unwrap();
    {
        let store = SecretStore::open(tmp.path()).unwrap();
        register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    }
    let env = tmp.path().join(ENV_FILE);
    set_mode(&env, 0o644);

    let store = SecretStore::open(tmp.path()).unwrap();
    let secret = value_of(&store, KeyKind::Broker, "client_secret");

    assert_eq!(mode_of(&env), 0o600);
    assert_eq!(secret.as_deref(), Some(SECRET_1));
}

// 검증: D3, N8
#[cfg(unix)]
#[tokio::test]
async fn 넓은_권한의_auth_json은_읽기_전에_0600으로_좁힌다() {
    let tmp = tempdir().unwrap();
    {
        let store = SecretStore::open(tmp.path()).unwrap();
        register_ok(&store, KeyKind::AiOAuth, oauth(ACCESS, REFRESH)).await;
    }
    let auth = tmp.path().join(AUTH_FILE);
    set_mode(&auth, 0o666);

    let store = SecretStore::open(tmp.path()).unwrap();
    let access = value_of(&store, KeyKind::AiOAuth, "access_token");

    assert_eq!(mode_of(&auth), 0o600);
    assert_eq!(access.as_deref(), Some(ACCESS));
}

// 검증: D3
#[tokio::test]
async fn 쓰고_나면_임시_파일이_남지_않는다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();

    register_all(&store).await;
    register_ok(&store, KeyKind::Broker, broker(ID_2, SECRET_2)).await;
    register_ok(
        &store,
        KeyKind::AiOAuth,
        oauth(ACCESS, "시험용-갱신-토큰-0002"),
    )
    .await;

    assert_eq!(
        names_in(tmp.path()),
        vec![ENV_FILE.to_string(), AUTH_FILE.to_string()]
    );
}

// 검증: D3, E4
#[tokio::test]
async fn 거부되거나_확인에_실패한_등록도_임시_파일을_남기지_않는다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;

    let failing = FakeVerifier::failing(VerifyFailure::Rejected);
    let _ = store
        .register(KeyKind::Broker, broker(ID_2, SECRET_2), &failing)
        .await;
    let _ = store
        .register(KeyKind::Data, api_key("짧은-값-07"), &FakeVerifier::ok())
        .await;
    let _ = store
        .register(KeyKind::AiOAuth, oauth(ACCESS, REFRESH), &failing)
        .await;

    assert_eq!(names_in(tmp.path()), vec![ENV_FILE.to_string()]);
}

// 검증: D3
#[cfg(unix)]
#[tokio::test]
async fn 덮어쓸_때_제자리에_쓰지_않고_새_파일로_바꿔_끼운다() {
    use std::os::unix::fs::MetadataExt;

    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();

    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    let env = tmp.path().join(ENV_FILE);
    // 옛 파일을 열어 둔다. 이름 바꾸기로 끼우면 옛 내용은 이 손잡이에 그대로 남는다.
    let old_handle = fs::File::open(&env).unwrap();
    let old_inode = old_handle.metadata().unwrap().ino();

    register_ok(&store, KeyKind::Broker, broker(ID_2, SECRET_2)).await;

    let new_inode = fs::metadata(&env).unwrap().ino();
    assert_ne!(
        old_inode, new_inode,
        "제자리에 덮어썼다(원자 쓰기가 아니다)"
    );
    let mut old_text = String::new();
    std::io::Read::read_to_string(&mut &old_handle, &mut old_text).unwrap();
    assert!(old_text.contains(SECRET_1), "옛 파일 내용이 바뀌었다");
    assert!(!old_text.contains(SECRET_2), "옛 파일에 새 값을 썼다");
    assert!(fs::read_to_string(&env).unwrap().contains(SECRET_2));
}

// 검증: D3, N8
#[cfg(unix)]
#[tokio::test]
async fn 쓰는_중에도_넓은_권한의_파일이나_덜_쓰인_env가_보이지_않는다() {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// 테스트가 어떻게 끝나든 지켜보는 스레드를 멈춘다.
    struct StopOnDrop(Arc<AtomicBool>);
    impl Drop for StopOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let value_a = "plain-test-value-aaaa-0001";
    let value_b = "plain-test-value-bbbb-0002";

    let tmp = tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let store = SecretStore::open(&dir).unwrap();
    register_ok(&store, KeyKind::Data, api_key(value_a)).await;

    let stop = Arc::new(AtomicBool::new(false));
    let guard = StopOnDrop(Arc::clone(&stop));
    let watch_dir = dir.clone();
    let watcher = std::thread::spawn(move || {
        let mut problems: Vec<String> = Vec::new();
        let mut looks = 0_u64;
        while !stop.load(Ordering::SeqCst) {
            if let Ok(entries) = fs::read_dir(&watch_dir) {
                for entry in entries.flatten() {
                    // 임시 파일은 보는 사이에 사라질 수 있다. 그때는 넘어간다.
                    if let Ok(meta) = entry.metadata() {
                        let mode = meta.permissions().mode() & 0o777;
                        if meta.is_file() && mode & 0o077 != 0 {
                            problems.push(format!(
                                "넓은 권한 {mode:o}: {}",
                                entry.file_name().to_string_lossy()
                            ));
                        }
                    }
                }
            }
            match fs::read_to_string(watch_dir.join(ENV_FILE)) {
                Ok(text) => {
                    if !text.contains(value_a) && !text.contains(value_b) {
                        problems.push("덜 쓰인 .env가 보였다".to_string());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    problems.push(".env가 잠깐 없어졌다".to_string());
                }
                Err(_) => {}
            }
            looks += 1;
        }
        (problems, looks)
    });

    for round in 0..300 {
        let value = if round % 2 == 0 { value_b } else { value_a };
        register_ok(&store, KeyKind::Data, api_key(value)).await;
    }
    drop(guard);
    let (problems, looks) = watcher.join().unwrap();

    assert!(looks > 0, "지켜보는 스레드가 돌지 않았다");
    assert!(problems.is_empty(), "쓰는 중에 보인 문제: {problems:?}");
    assert_eq!(names_in(&dir), vec![ENV_FILE.to_string()]);
}

// ───────────────────────── 키 종류 (만드는 것 3) ─────────────────────────

// 검증: C3
#[tokio::test]
async fn 종류마다_이름이_정해진_칸을_가진다() {
    assert_eq!(KeyKind::Broker.fields(), ["client_id", "client_secret"]);
    for kind in KeyKind::ALL {
        assert!(!kind.fields().is_empty());
    }

    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_all(&store).await;

    assert_eq!(
        value_of(&store, KeyKind::Broker, "client_id").as_deref(),
        Some(ID_1)
    );
    assert_eq!(
        value_of(&store, KeyKind::Broker, "client_secret").as_deref(),
        Some(SECRET_1)
    );
    assert_eq!(
        value_of(&store, KeyKind::AiApiKey, "api_key").as_deref(),
        Some(AI_KEY)
    );
    assert_eq!(
        value_of(&store, KeyKind::AiOAuth, "access_token").as_deref(),
        Some(ACCESS)
    );
    assert_eq!(
        value_of(&store, KeyKind::AiOAuth, "refresh_token").as_deref(),
        Some(REFRESH)
    );
    assert_eq!(
        value_of(&store, KeyKind::Data, "api_key").as_deref(),
        Some(DATA_KEY)
    );
    // 그 종류에 없는 칸은 없다.
    assert_eq!(value_of(&store, KeyKind::Broker, "api_key"), None);
}

// 검증: C3
#[tokio::test]
async fn 칸이_빠지면_확인기를_부르지_않고_거부한다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    let verifier = FakeVerifier::ok();

    let only_id = Credentials::new().with("client_id", ID_1);
    let result = store.register(KeyKind::Broker, only_id, &verifier).await;

    assert!(matches!(
        result,
        Err(SecretsError::MissingField {
            field: "client_secret"
        })
    ));
    assert_eq!(verifier.calls(), 0);
    assert!(registered_kinds(&store).is_empty());
    assert!(!tmp.path().join(ENV_FILE).exists());
}

// 검증: C3
#[tokio::test]
async fn 그_종류에_없는_칸이_오면_확인기를_부르지_않고_거부한다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    let verifier = FakeVerifier::ok();

    let extra = broker(ID_1, SECRET_1).with("api_key", DATA_KEY);
    let result = store.register(KeyKind::Broker, extra, &verifier).await;

    assert!(matches!(result, Err(SecretsError::UnknownField)));
    assert_eq!(verifier.calls(), 0);
    assert!(registered_kinds(&store).is_empty());
    assert!(!tmp.path().join(ENV_FILE).exists());
}

// ───────────────────────── 등록 (만드는 것 4) ─────────────────────────

// 검증: C3, V46
#[tokio::test]
async fn 확인을_지나면_등록됨만_돌려주고_값은_돌려주지_않는다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();

    let done = store
        .register(KeyKind::Broker, broker(ID_1, SECRET_1), &FakeVerifier::ok())
        .await
        .unwrap();

    assert_eq!(
        done,
        Registered {
            kind: KeyKind::Broker
        }
    );
    let shown = format!("{done:?}");
    assert_no_value(&shown, ID_1, "등록 결과");
    assert_no_value(&shown, SECRET_1, "등록 결과");
    assert_eq!(registered_kinds(&store), vec![KeyKind::Broker]);
}

// 검증: C3, E4
#[tokio::test]
async fn 확인기는_넣은_종류와_값을_그대로_한_번_받는다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    let verifier = FakeVerifier::ok();

    store
        .register(KeyKind::Broker, broker(ID_1, SECRET_1), &verifier)
        .await
        .unwrap();

    assert_eq!(verifier.calls(), 1);
    assert_eq!(
        verifier.seen(),
        vec![(
            KeyKind::Broker,
            vec![
                ("client_id".to_string(), ID_1.to_string()),
                ("client_secret".to_string(), SECRET_1.to_string()),
            ]
        )]
    );
}

// 검증: E4, C3
#[tokio::test]
async fn 확인에_실패한_키는_저장하지_않는다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    let verifier = FakeVerifier::failing(VerifyFailure::Rejected);

    let result = store
        .register(KeyKind::Broker, broker(ID_1, SECRET_1), &verifier)
        .await;

    assert!(matches!(
        result,
        Err(SecretsError::VerifyFailed(VerifyFailure::Rejected))
    ));
    assert_eq!(verifier.calls(), 1);
    assert!(registered_kinds(&store).is_empty());
    assert!(store.read(KeyKind::Broker).unwrap().is_none());
    assert!(!tmp.path().join(ENV_FILE).exists());
    assert!(!tmp.path().join(AUTH_FILE).exists());

    // 다시 열어도 없다.
    let reopened = SecretStore::open(tmp.path()).unwrap();
    assert!(registered_kinds(&reopened).is_empty());
}

// 검증: E4
#[tokio::test]
async fn 확인할_서버에_닿지_못해도_저장하지_않는다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    let verifier = FakeVerifier::failing(VerifyFailure::Unreachable);

    let result = store
        .register(KeyKind::AiOAuth, oauth(ACCESS, REFRESH), &verifier)
        .await;

    assert!(matches!(
        result,
        Err(SecretsError::VerifyFailed(VerifyFailure::Unreachable))
    ));
    assert!(registered_kinds(&store).is_empty());
    assert!(!tmp.path().join(AUTH_FILE).exists());
}

// 검증: E4, C16
#[tokio::test]
async fn 다시_등록이_확인에_실패하면_파일과_옛_값이_그대로다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_all(&store).await;
    let env_before = fs::read(tmp.path().join(ENV_FILE)).unwrap();
    let auth_before = fs::read(tmp.path().join(AUTH_FILE)).unwrap();

    let failing = FakeVerifier::failing(VerifyFailure::Rejected);
    let broker_result = store
        .register(KeyKind::Broker, broker(ID_2, SECRET_2), &failing)
        .await;
    let oauth_result = store
        .register(
            KeyKind::AiOAuth,
            oauth("시험용-접근-토큰-0002", "시험용-갱신-토큰-0002"),
            &failing,
        )
        .await;

    assert!(matches!(broker_result, Err(SecretsError::VerifyFailed(_))));
    assert!(matches!(oauth_result, Err(SecretsError::VerifyFailed(_))));
    assert_eq!(fs::read(tmp.path().join(ENV_FILE)).unwrap(), env_before);
    assert_eq!(fs::read(tmp.path().join(AUTH_FILE)).unwrap(), auth_before);
    assert_eq!(
        value_of(&store, KeyKind::Broker, "client_secret").as_deref(),
        Some(SECRET_1)
    );
    assert_eq!(
        value_of(&store, KeyKind::AiOAuth, "access_token").as_deref(),
        Some(ACCESS)
    );
    // 실패한 새 값은 가릴 대상도 아니다: 옛 값은 계속 가린다.
    assert!(!store.redact(SECRET_1).contains(SECRET_1));
}

// 검증: C16
#[tokio::test]
async fn 다시_등록하면_덮어쓴다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;

    let done = register_ok(&store, KeyKind::Broker, broker(ID_2, SECRET_2)).await;

    assert_eq!(done.kind, KeyKind::Broker);
    assert_eq!(
        value_of(&store, KeyKind::Broker, "client_id").as_deref(),
        Some(ID_2)
    );
    assert_eq!(
        value_of(&store, KeyKind::Broker, "client_secret").as_deref(),
        Some(SECRET_2)
    );
    let env = fs::read_to_string(tmp.path().join(ENV_FILE)).unwrap();
    assert!(env.contains(SECRET_2));
    assert!(!env.contains(SECRET_1), "옛 값이 파일에 남았다");
    assert!(!env.contains(ID_1), "옛 값이 파일에 남았다");
    assert_eq!(registered_kinds(&store), vec![KeyKind::Broker]);
}

// 검증: C16
#[tokio::test]
async fn oauth_토큰도_다시_등록하면_덮어쓴다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::AiOAuth, oauth(ACCESS, REFRESH)).await;

    register_ok(
        &store,
        KeyKind::AiOAuth,
        oauth("시험용-접근-토큰-0002", "시험용-갱신-토큰-0002"),
    )
    .await;

    assert_eq!(
        value_of(&store, KeyKind::AiOAuth, "access_token").as_deref(),
        Some("시험용-접근-토큰-0002")
    );
    assert_eq!(
        value_of(&store, KeyKind::AiOAuth, "refresh_token").as_deref(),
        Some("시험용-갱신-토큰-0002")
    );
    let auth = fs::read_to_string(tmp.path().join(AUTH_FILE)).unwrap();
    assert!(
        !auth.contains(ACCESS) && !auth.contains(REFRESH),
        "옛 값이 파일에 남았다"
    );
}

// 검증: C3, C16
#[tokio::test]
async fn 한_종류를_덮어써도_같은_파일의_다른_종류는_그대로다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_all(&store).await;

    register_ok(&store, KeyKind::Broker, broker(ID_2, SECRET_2)).await;
    register_ok(
        &store,
        KeyKind::AiApiKey,
        api_key("시험용-에이아이-값-0002"),
    )
    .await;

    assert_eq!(
        value_of(&store, KeyKind::Broker, "client_secret").as_deref(),
        Some(SECRET_2)
    );
    assert_eq!(
        value_of(&store, KeyKind::AiApiKey, "api_key").as_deref(),
        Some("시험용-에이아이-값-0002")
    );
    assert_eq!(
        value_of(&store, KeyKind::Data, "api_key").as_deref(),
        Some(DATA_KEY)
    );
    assert_eq!(
        value_of(&store, KeyKind::AiOAuth, "refresh_token").as_deref(),
        Some(REFRESH)
    );
    assert_eq!(registered_kinds(&store), KeyKind::ALL.to_vec());
}

// 검증: D3, C16
#[tokio::test]
async fn 보관소를_다시_열어도_등록한_값을_읽는다() {
    let tmp = tempdir().unwrap();
    {
        let store = SecretStore::open(tmp.path()).unwrap();
        register_all(&store).await;
        register_ok(&store, KeyKind::Broker, broker(ID_2, SECRET_2)).await;
    }

    let store = SecretStore::open(tmp.path()).unwrap();

    assert_eq!(
        value_of(&store, KeyKind::Broker, "client_id").as_deref(),
        Some(ID_2)
    );
    assert_eq!(
        value_of(&store, KeyKind::Broker, "client_secret").as_deref(),
        Some(SECRET_2)
    );
    assert_eq!(
        value_of(&store, KeyKind::AiApiKey, "api_key").as_deref(),
        Some(AI_KEY)
    );
    assert_eq!(
        value_of(&store, KeyKind::AiOAuth, "access_token").as_deref(),
        Some(ACCESS)
    );
    assert_eq!(
        value_of(&store, KeyKind::AiOAuth, "refresh_token").as_deref(),
        Some(REFRESH)
    );
    assert_eq!(
        value_of(&store, KeyKind::Data, "api_key").as_deref(),
        Some(DATA_KEY)
    );
    assert_eq!(registered_kinds(&store), KeyKind::ALL.to_vec());
}

// 검증: C3
#[tokio::test]
async fn 등록하지_않은_종류를_읽으면_없다고_한다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    for kind in KeyKind::ALL {
        assert!(store.read(kind).unwrap().is_none());
    }

    register_ok(&store, KeyKind::Data, api_key(DATA_KEY)).await;

    assert!(store.read(KeyKind::Data).unwrap().is_some());
    assert!(store.read(KeyKind::AiApiKey).unwrap().is_none());
    assert!(store.read(KeyKind::Broker).unwrap().is_none());
    assert!(store.read(KeyKind::AiOAuth).unwrap().is_none());
}

// ───────────────────────── 상태 보기 (만드는 것 5) ─────────────────────────

// 검증: C3, V46
#[tokio::test]
async fn 상태_보기는_종류별_등록_여부만_알려_준다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();

    let empty = store.status().unwrap();
    assert_eq!(
        empty,
        KeyKind::ALL
            .iter()
            .map(|&kind| KeyStatus {
                kind,
                registered: false
            })
            .collect::<Vec<_>>()
    );

    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    register_ok(&store, KeyKind::AiOAuth, oauth(ACCESS, REFRESH)).await;

    let expected: Vec<KeyStatus> = KeyKind::ALL
        .iter()
        .map(|&kind| KeyStatus {
            kind,
            registered: kind == KeyKind::Broker || kind == KeyKind::AiOAuth,
        })
        .collect();
    assert_eq!(store.status().unwrap(), expected);

    let reopened = SecretStore::open(tmp.path()).unwrap();
    assert_eq!(reopened.status().unwrap(), expected);
}

// 검증: V46, N8
#[tokio::test]
async fn 상태_보기에_값도_값의_일부도_길이도_없다() {
    let short_value = "시험용-값-01";
    let long_value = "시험용-아주-아주-긴-비밀-값-0000000000000000000000000000000001";

    let tmp_short = tempdir().unwrap();
    let store_short = SecretStore::open(tmp_short.path()).unwrap();
    register_ok(&store_short, KeyKind::Data, api_key(short_value)).await;

    let tmp_long = tempdir().unwrap();
    let store_long = SecretStore::open(tmp_long.path()).unwrap();
    register_ok(&store_long, KeyKind::Data, api_key(long_value)).await;

    let shown_short = format!("{:?}", store_short.status().unwrap());
    let shown_long = format!("{:?}", store_long.status().unwrap());

    // 값의 길이가 달라도 보이는 것이 똑같다.
    assert_eq!(shown_short, shown_long);
    assert_no_value(&shown_short, short_value, "상태 보기");
    assert_no_value(&shown_long, long_value, "상태 보기");
    for part in ["시험용-값", "아주-긴", "0000000001"] {
        assert!(
            !shown_short.contains(part) && !shown_long.contains(part),
            "상태 보기에 값의 일부가 있다"
        );
    }
    for length in [
        short_value.len(),
        short_value.chars().count(),
        long_value.len(),
        long_value.chars().count(),
    ] {
        assert!(
            !shown_long.contains(&length.to_string()),
            "상태 보기에 길이가 있다"
        );
    }
}

// ───────────────────────── 가려진 값 타입 (만드는 것 6) ─────────────────────────

// 검증: N8, V46
#[test]
fn 비밀_값_타입은_찍어도_가려진_글자만_나온다() {
    // 직렬화할 수 없고, 버려질 때 지워지고, 보관소는 여러 곳에서 함께 쓸 수 있다.
    static_assertions::assert_not_impl_any!(SecretString: serde::Serialize);
    static_assertions::assert_not_impl_any!(Credentials: serde::Serialize);
    static_assertions::assert_impl_all!(SecretString: zeroize::ZeroizeOnDrop);
    static_assertions::assert_impl_all!(SecretStore: Send, Sync);

    let short = SecretString::new("시험용-값-01");
    let long = SecretString::new("시험용-아주-아주-긴-비밀-값-0000000000000000000000000000000001");

    assert_eq!(short.expose(), "시험용-값-01");
    assert_eq!(format!("{short}"), REDACTED);
    assert_eq!(format!("{long}"), REDACTED);

    let debug_short = format!("{short:?}");
    let debug_long = format!("{long:?}");
    assert!(debug_short.contains(REDACTED));
    assert_no_value(&debug_short, short.expose(), "Debug");
    assert_no_value(&debug_long, long.expose(), "Debug");
    assert!(!debug_short.contains("시험용"), "Debug에 값의 일부가 있다");
    // 길이가 달라도 찍힌 글이 같다(길이가 새지 않는다).
    assert_eq!(debug_short, debug_long);
    assert_eq!(format!("{short:#?}"), format!("{long:#?}"));

    assert!(!REDACTED.is_empty());
}

// 검증: N8, V46
#[test]
fn 값_묶음을_찍어도_칸_이름과_가려진_글자만_나온다() {
    let values = broker(ID_1, SECRET_1);

    assert_eq!(values.field_names(), vec!["client_id", "client_secret"]);
    assert_eq!(values.get("client_secret").unwrap().expose(), SECRET_1);
    assert!(values.get("없는-칸").is_none());

    for shown in [format!("{values:?}"), format!("{values:#?}")] {
        assert!(shown.contains("client_id"));
        assert!(shown.contains("client_secret"));
        assert!(shown.contains(REDACTED));
        assert_no_value(&shown, ID_1, "값 묶음 Debug");
        assert_no_value(&shown, SECRET_1, "값 묶음 Debug");
    }

    // 같은 칸을 다시 주면 나중 값이 남는다.
    let replaced = values.with("client_secret", SECRET_2);
    assert_eq!(replaced.get("client_secret").unwrap().expose(), SECRET_2);
    assert_eq!(replaced.field_names(), vec!["client_id", "client_secret"]);
}

// 검증: V46, N8
#[tokio::test]
async fn 등록한_뒤_어떤_반환값과_출력에도_값이_없다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();

    let mut shown: Vec<(&str, String)> = Vec::new();
    for (kind, values) in [
        (KeyKind::Broker, broker(ID_1, SECRET_1)),
        (KeyKind::AiApiKey, api_key(AI_KEY)),
        (KeyKind::AiOAuth, oauth(ACCESS, REFRESH)),
        (KeyKind::Data, api_key(DATA_KEY)),
    ] {
        let done = store
            .register(kind, values, &FakeVerifier::ok())
            .await
            .unwrap();
        shown.push(("등록 결과", format!("{done:?}")));
    }

    shown.push(("상태 보기", format!("{:?}", store.status().unwrap())));
    shown.push(("보관소 Debug", format!("{store:?}")));
    shown.push(("보관소 Debug(#)", format!("{store:#?}")));
    shown.push(("보관 폴더", store.dir().display().to_string()));
    for kind in KeyKind::ALL {
        let read = store.read(kind);
        shown.push(("읽은 결과 Debug", format!("{read:?}")));
        let values = read.unwrap().unwrap();
        for field in kind.fields() {
            let value = values.get(field).unwrap();
            shown.push(("읽은 값 Debug", format!("{value:?}")));
            shown.push(("읽은 값 Display", format!("{value}")));
        }
    }
    // 등록된 값을 그대로 넣은 글도 가려져 나온다.
    shown.push(("가린 글", store.redact(&ALL_VALUES.join(" / "))));

    let reopened = SecretStore::open(tmp.path()).unwrap();
    shown.push(("다시 연 보관소 Debug", format!("{reopened:?}")));
    shown.push((
        "다시 연 상태 보기",
        format!("{:?}", reopened.status().unwrap()),
    ));

    for (what, text) in &shown {
        for value in ALL_VALUES {
            assert_no_value(text, value, what);
        }
    }
}

// ───────────────────────── 가리기 (만드는 것 7) ─────────────────────────

// 검증: E38, N8
#[tokio::test]
async fn 글에_든_등록된_값을_가려진_글자로_바꾼다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;

    let text = format!("주문 실패: 키 {SECRET_1} 로 불렀다. 다시 {SECRET_1}, 아이디 {ID_1} 끝");
    let expected =
        format!("주문 실패: 키 {REDACTED} 로 불렀다. 다시 {REDACTED}, 아이디 {REDACTED} 끝");

    assert_eq!(store.redact(&text), expected);
}

// 검증: E38
#[tokio::test]
async fn 비밀_값이_없는_글은_그대로_돌려준다() {
    let tmp = tempdir().unwrap();
    let empty_store = SecretStore::open(tmp.path()).unwrap();
    let text = "삼성전자 10주 매수 주문을 넣었습니다. key=없음 a%20b QUJD==\n둘째 줄";
    assert_eq!(empty_store.redact(text), text);

    register_ok(&empty_store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    assert_eq!(empty_store.redact(text), text);
    assert_eq!(empty_store.redact(""), "");
}

// 검증: E38, N8
#[tokio::test]
async fn 모든_종류의_모든_칸을_가린다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_all(&store).await;

    for value in ALL_VALUES {
        let out = store.redact(&format!("앞 {value} 뒤"));
        assert_eq!(out, format!("앞 {REDACTED} 뒤"));
    }
    let joined = ALL_VALUES.join("|");
    let out = store.redact(&joined);
    for value in ALL_VALUES {
        assert_no_value(&out, value, "가린 글");
    }
}

// 검증: E38, N8
#[tokio::test]
async fn 다시_연_보관소도_등록된_값을_가린다() {
    let tmp = tempdir().unwrap();
    {
        let store = SecretStore::open(tmp.path()).unwrap();
        register_all(&store).await;
    }

    let store = SecretStore::open(tmp.path()).unwrap();

    for value in ALL_VALUES {
        assert_eq!(
            store.redact(&format!("앞 {value} 뒤")),
            format!("앞 {REDACTED} 뒤")
        );
    }
}

// 검증: E38, C16
#[tokio::test]
async fn 덮어쓴_새_값도_가린다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    register_ok(&store, KeyKind::Broker, broker(ID_2, SECRET_2)).await;

    assert_eq!(
        store.redact(&format!("새 값 {SECRET_2} 과 {ID_2}")),
        format!("새 값 {REDACTED} 과 {REDACTED}")
    );
}

// 검증: E38, N8
#[tokio::test]
async fn url_인코딩된_값도_가린다() {
    let secret = "시험용/비밀=값&0001";
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Data, api_key(secret)).await;

    let upper = utf8_percent_encode(secret, NON_ALPHANUMERIC).to_string();
    let lower = upper.to_lowercase();
    assert_ne!(upper, secret);
    assert_ne!(upper, lower);

    for encoded in [&upper, &lower] {
        let text = format!("GET http://127.0.0.1:9/시세?key={encoded}&종목=005930 실패");
        let out = store.redact(&text);
        assert!(!out.contains(encoded.as_str()), "URL 인코딩된 값이 남았다");
        assert!(out.contains(REDACTED));
        assert!(out.starts_with("GET http://127.0.0.1:9/시세?key="));
        assert!(out.ends_with("&종목=005930 실패"));
    }
}

// 검증: E38, N8
#[tokio::test]
async fn base64로_든_값도_가린다() {
    let secret = "시험용/비밀=값&0001";
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Data, api_key(secret)).await;

    let forms = [
        STANDARD.encode(secret),
        STANDARD_NO_PAD.encode(secret),
        URL_SAFE.encode(secret),
        URL_SAFE_NO_PAD.encode(secret),
    ];
    for encoded in &forms {
        let text = format!("머리글 Authorization: Bearer {encoded} 를 보냈다");
        let out = store.redact(&text);
        let core = encoded.trim_end_matches('=');
        assert!(!out.contains(core), "Base64 값이 남았다");
        assert!(out.contains(REDACTED));
        assert!(out.starts_with("머리글 Authorization: Bearer "));
        assert!(out.ends_with(" 를 보냈다"));
    }
}

// 검증: E38, N8
#[tokio::test]
async fn 표준과_url_안전_base64가_다른_값도_둘_다_가린다() {
    // 이 값은 Base64로 바꾸면 `+`나 `/`가 나온다(URL 안전 꼴은 `-` · `_`).
    let secret = "시험용>>비밀??값~~0001";
    let standard = STANDARD.encode(secret);
    let url_safe = URL_SAFE.encode(secret);
    assert_ne!(standard, url_safe, "두 꼴이 달라지는 값을 골라야 한다");

    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::AiApiKey, api_key(secret)).await;

    for encoded in [&standard, &url_safe] {
        let out = store.redact(&format!("본문 {encoded} 끝"));
        assert!(!out.contains(encoded.trim_end_matches('=')));
        assert!(out.contains(REDACTED));
    }
}

// 검증: E38, N8
#[tokio::test]
async fn 여덟_자보다_짧은_값은_등록할_때_거부한다() {
    assert_eq!(MIN_SECRET_CHARS, 8);
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    let verifier = FakeVerifier::ok();

    // 7자.
    let result = store
        .register(KeyKind::Data, api_key("abcd123"), &verifier)
        .await;
    assert!(matches!(
        result,
        Err(SecretsError::TooShort {
            field: "api_key",
            min: 8
        })
    ));

    // 빈 값.
    let result = store.register(KeyKind::Data, api_key(""), &verifier).await;
    assert!(matches!(result, Err(SecretsError::TooShort { .. })));

    // 두 칸 가운데 하나만 짧아도 거부한다.
    let result = store
        .register(KeyKind::Broker, broker(ID_1, "abcd123"), &verifier)
        .await;
    assert!(matches!(
        result,
        Err(SecretsError::TooShort {
            field: "client_secret",
            ..
        })
    ));
    let result = store
        .register(KeyKind::AiOAuth, oauth("abcd123", REFRESH), &verifier)
        .await;
    assert!(matches!(
        result,
        Err(SecretsError::TooShort {
            field: "access_token",
            ..
        })
    ));

    assert_eq!(verifier.calls(), 0, "거부할 값으로 확인기를 불렀다");
    assert!(registered_kinds(&store).is_empty());
    assert!(names_in(tmp.path()).is_empty());
}

// 검증: E38
#[tokio::test]
async fn 여덟_자인_값은_등록되고_가려진다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();

    register_ok(&store, KeyKind::Data, api_key("abcd1234")).await;

    assert_eq!(
        value_of(&store, KeyKind::Data, "api_key").as_deref(),
        Some("abcd1234")
    );
    assert_eq!(
        store.redact("값은 abcd1234 이다"),
        format!("값은 {REDACTED} 이다")
    );
}

// 검증: E38, C16
#[tokio::test]
async fn 짧은_값으로_다시_등록하면_옛_값이_그대로다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Data, api_key(DATA_KEY)).await;
    let before = fs::read(tmp.path().join(ENV_FILE)).unwrap();

    let result = store
        .register(KeyKind::Data, api_key("abcd123"), &FakeVerifier::ok())
        .await;

    assert!(matches!(result, Err(SecretsError::TooShort { .. })));
    assert_eq!(fs::read(tmp.path().join(ENV_FILE)).unwrap(), before);
    assert_eq!(
        value_of(&store, KeyKind::Data, "api_key").as_deref(),
        Some(DATA_KEY)
    );
}

// ───────────────────────── 오류에 값 없음 (만드는 것 8) ─────────────────────────

// 검증: N8, V46, E4
#[tokio::test]
async fn 오류_문장과_오류_debug에_값이_없다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;

    let short = "짧은-값-07";
    let with_newline = "시험용-줄바꿈-값-0001\n시험용-둘째-줄-0001";
    let mut errors: Vec<(SecretsError, Vec<&str>)> = Vec::new();

    for failure in [VerifyFailure::Rejected, VerifyFailure::Unreachable] {
        let e = store
            .register(
                KeyKind::Broker,
                broker(ID_2, SECRET_2),
                &FakeVerifier::failing(failure),
            )
            .await
            .unwrap_err();
        errors.push((e, vec![ID_2, SECRET_2, ID_1, SECRET_1]));
    }

    let e = store
        .register(KeyKind::Data, api_key(short), &FakeVerifier::ok())
        .await
        .unwrap_err();
    errors.push((e, vec![short]));

    let e = store
        .register(KeyKind::Data, api_key(with_newline), &FakeVerifier::ok())
        .await
        .unwrap_err();
    errors.push((
        e,
        vec![with_newline, "시험용-줄바꿈-값-0001", "시험용-둘째-줄-0001"],
    ));

    let e = store
        .register(
            KeyKind::Broker,
            Credentials::new().with("client_id", ID_2),
            &FakeVerifier::ok(),
        )
        .await
        .unwrap_err();
    errors.push((e, vec![ID_2]));

    // 칸 이름 자리에 값을 잘못 넣어도 그 글자가 오류에 실리지 않는다.
    let e = store
        .register(
            KeyKind::Broker,
            broker(ID_2, SECRET_2).with("시험용-잘못-넣은-값-0001", AI_KEY),
            &FakeVerifier::ok(),
        )
        .await
        .unwrap_err();
    errors.push((e, vec![ID_2, SECRET_2, AI_KEY, "시험용-잘못-넣은-값-0001"]));

    assert_eq!(errors.len(), 6);
    for (error, values) in &errors {
        let shown = [
            format!("{error}"),
            format!("{error:?}"),
            format!("{error:#?}"),
        ];
        for text in &shown {
            assert!(!text.is_empty());
            for value in values {
                assert_no_value(text, value, "오류");
            }
        }
    }
}

// 검증: N8, V46
#[tokio::test]
async fn 깨진_키_파일의_내용이_결과나_오류에_실리지_않는다() {
    let tmp = tempdir().unwrap();
    {
        let store = SecretStore::open(tmp.path()).unwrap();
        register_ok(&store, KeyKind::AiOAuth, oauth(ACCESS, REFRESH)).await;
    }
    // 파일을 반쯤 잘린 모양으로 망가뜨린다.
    let broken = "시험용-깨진-내용-0001";
    fs::write(
        tmp.path().join(AUTH_FILE),
        format!("{{\"{broken}\": \"{broken}"),
    )
    .unwrap();

    let mut shown: Vec<String> = Vec::new();
    match SecretStore::open(tmp.path()) {
        Ok(store) => {
            shown.push(format!("{store:?}"));
            let read = store.read(KeyKind::AiOAuth);
            shown.push(format!("{read:?}"));
            if let Err(e) = &read {
                shown.push(format!("{e}"));
            }
            let status = store.status();
            shown.push(format!("{status:?}"));
            if let Err(e) = &status {
                shown.push(format!("{e}"));
            }
        }
        Err(e) => {
            shown.push(format!("{e}"));
            shown.push(format!("{e:?}"));
        }
    }

    assert!(!shown.is_empty());
    for text in &shown {
        assert_no_value(text, broken, "깨진 파일을 본 결과");
    }
}

// 검증: N8, D3
#[tokio::test]
async fn 보관_폴더를_쓸_수_없으면_값_없는_오류를_낸다() {
    let tmp = tempdir().unwrap();
    // 폴더가 있어야 할 자리에 파일이 있다.
    let blocked = tmp.path().join("폴더가-아닌-파일");
    fs::write(&blocked, "폴더가 아니다").unwrap();

    let error = match SecretStore::open(&blocked) {
        Err(e) => e,
        Ok(store) => store
            .register(KeyKind::Broker, broker(ID_1, SECRET_1), &FakeVerifier::ok())
            .await
            .expect_err("폴더가 아닌 곳에는 저장할 수 없다"),
    };

    assert!(matches!(error, SecretsError::Io { .. }));
    for text in [format!("{error}"), format!("{error:?}")] {
        assert_no_value(&text, ID_1, "오류");
        assert_no_value(&text, SECRET_1, "오류");
    }
    assert_eq!(fs::read_to_string(&blocked).unwrap(), "폴더가 아니다");
}

// ───────────────────────── .env와 특수 문자 ─────────────────────────

// 검증: D3, C3
#[tokio::test]
async fn 공백이_든_값이_env를_다녀와도_같다() {
    assert_env_round_trip("공백 있는 시험 값 0001").await;
    assert_env_round_trip("탭\t있는\t시험\t값").await;
}

// 검증: D3, C3
#[tokio::test]
async fn 등호가_든_값이_env를_다녀와도_같다() {
    assert_env_round_trip("등호=든=시험=값=0001").await;
    assert_env_round_trip("끝이-등호인-시험-값==").await;
    assert_env_round_trip("=등호로-시작하는-시험-값").await;
}

// 검증: D3, C3
#[tokio::test]
async fn 샵이_든_값이_env를_다녀와도_같다() {
    assert_env_round_trip("샵#든#시험#값#0001").await;
    assert_env_round_trip("샵 #앞에 공백 든 시험 값").await;
    assert_env_round_trip("#샵으로-시작하는-시험-값").await;
}

// 검증: D3, C3
#[tokio::test]
async fn 따옴표가_든_값이_env를_다녀와도_같다() {
    assert_env_round_trip("\"큰따옴표\"로 싼 시험 값").await;
    assert_env_round_trip("'작은따옴표'로 싼 시험 값").await;
    assert_env_round_trip("\"여는-큰따옴표만-있는-시험-값").await;
    assert_env_round_trip("닫는-작은따옴표만-있는-시험-값'").await;
    assert_env_round_trip("섞은 \"큰\" '작은' `백틱` 시험 값").await;
}

// 검증: D3, C3
#[tokio::test]
async fn 역슬래시와_달러가_든_값이_env를_다녀와도_같다() {
    // 글자 그대로의 `\n`(역슬래시와 n)은 줄바꿈이 아니다. 그대로 돌아와야 한다.
    assert_env_round_trip("역슬래시\\n든\\t시험\\값\\").await;
    assert_env_round_trip("달러 $HOME ${PRICE_INHA_HOME} 든 시험 값").await;
    assert_env_round_trip("다 섞은 \"값\" = '시험' # 0001 \\ $X").await;
}

// 검증: D3, C3, N8
#[tokio::test]
async fn 줄바꿈이_든_값은_거부하고_아무것도_쓰지_않는다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    let verifier = FakeVerifier::ok();

    for value in [
        "시험용-줄바꿈-값-0001\n시험용-둘째-줄-0001",
        "시험용-줄바꿈-값-0001\r\n시험용-둘째-줄-0001",
        "시험용-줄바꿈-값-0001\r시험용-둘째-줄-0001",
        "시험용-끝에-줄바꿈-값-0001\n",
        "\n시험용-앞에-줄바꿈-값-0001",
    ] {
        let result = store
            .register(KeyKind::Data, api_key(value), &verifier)
            .await;
        assert!(
            matches!(result, Err(SecretsError::InvalidValue { field: "api_key" })),
            "줄바꿈이 든 값을 거부하지 않았다: {value:?}"
        );
    }
    let result = store
        .register(
            KeyKind::Broker,
            broker(ID_1, "시험용-줄바꿈-값-0001\n시험용-둘째-줄-0001"),
            &verifier,
        )
        .await;
    assert!(matches!(
        result,
        Err(SecretsError::InvalidValue {
            field: "client_secret"
        })
    ));

    assert_eq!(verifier.calls(), 0, "거부할 값으로 확인기를 불렀다");
    assert!(registered_kinds(&store).is_empty());
    assert!(names_in(tmp.path()).is_empty());
}

// 검증: D3, N8, C16
#[tokio::test]
async fn 줄바꿈으로_다른_줄을_끼워_넣으려_해도_기존_값이_그대로다() {
    let tmp = tempdir().unwrap();
    let store = SecretStore::open(tmp.path()).unwrap();
    register_ok(&store, KeyKind::Broker, broker(ID_1, SECRET_1)).await;
    register_ok(&store, KeyKind::Data, api_key(DATA_KEY)).await;
    let env_file = tmp.path().join(ENV_FILE);
    let before = fs::read(&env_file).unwrap();
    // 방금 쓴 파일에서 증권사 줄의 이름을 알아낸다(끼워 넣기에 쓸 진짜 이름).
    let text = String::from_utf8(before.clone()).unwrap();
    let broker_line_name = text
        .lines()
        .find(|l| l.contains(SECRET_1))
        .and_then(|l| l.split_once('='))
        .map(|(name, _)| name.trim().to_string())
        .unwrap();

    let injected = "시험용-끼워-넣은-값-0001";
    let attack = format!("시험용-자료-값-0002\n{broker_line_name}={injected}");
    let result = store
        .register(KeyKind::Data, api_key(&attack), &FakeVerifier::ok())
        .await;

    assert!(matches!(
        result,
        Err(SecretsError::InvalidValue { field: "api_key" })
    ));
    assert_eq!(fs::read(&env_file).unwrap(), before);

    let reopened = SecretStore::open(tmp.path()).unwrap();
    assert_eq!(
        value_of(&reopened, KeyKind::Broker, "client_secret").as_deref(),
        Some(SECRET_1)
    );
    assert_eq!(
        value_of(&reopened, KeyKind::Data, "api_key").as_deref(),
        Some(DATA_KEY)
    );
    assert_eq!(
        registered_kinds(&reopened),
        vec![KeyKind::Broker, KeyKind::Data]
    );
}
