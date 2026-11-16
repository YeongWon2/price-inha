//! 공용 도우미: 가짜 GitHub contents API, 시험 환경, 한국 시각 날짜 계산.

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use age::secrecy::ExposeSecret;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use pi_backup::{
    Backup, BackupSettings, Bundle, BundleFile, MANIFEST_VERSION, Manifest, ManifestFile,
};
use pi_secrets::{
    Credentials, KeyKind, KeySlot, SecretStore, SecretString, Verifier, VerifyFailure,
};
use pi_store::{ApprovalId, NewApprovalRequest, Store, UnixMillis};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use wiremock::matchers::path_regex;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// 시험용 백업 토큰. 진짜 토큰 모양이 아니다.
pub const TOKEN: &str = "fake-backup-token-for-tests";
/// 시험용 자료 키. URL 인코딩 · Base64로 모양이 바뀌는 글자를 넣었다.
pub const DART_VALUE: &str = "fake/dart+value=for tests";
/// 시험용 저장소 이름.
pub const REPO: &str = "owner/repo";
const CONTENTS: &str = "/repos/owner/repo/contents/";
const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;

/// 모든 키를 받아 주는 확인기(바깥으로 나가지 않는다).
pub struct AcceptAll;

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

#[derive(Default)]
struct RemoteState {
    files: BTreeMap<String, Vec<u8>>,
    fail_put: Option<u16>,
    lie_size: bool,
}

/// 상태를 가진 가짜 GitHub contents API. 토큰 · User-Agent · 필수 칸을 실제처럼 따진다.
pub struct FakeGithub {
    server: MockServer,
    state: Arc<Mutex<RemoteState>>,
}

struct Handler(Arc<Mutex<RemoteState>>);

fn fake_sha(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn entry(path: &str, bytes: &[u8], size: u64) -> Value {
    let name = path.rsplit('/').next().unwrap_or(path);
    json!({ "type": "file", "name": name, "path": path, "sha": fake_sha(bytes), "size": size })
}

fn reply(status: u16, body: Value) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(body)
}

impl Handler {
    fn get(&self, state: &RemoteState, path: &str) -> ResponseTemplate {
        let extra = u64::from(state.lie_size);
        if let Some(bytes) = state.files.get(path) {
            let mut found = entry(path, bytes, bytes.len() as u64 + extra);
            found["encoding"] = json!("base64");
            found["content"] = json!(STANDARD.encode(bytes));
            return reply(200, found);
        }
        let prefix = format!("{}/", path.trim_end_matches('/'));
        let listed: Vec<Value> = state
            .files
            .iter()
            .filter(|(name, _)| name.starts_with(&prefix))
            .map(|(name, bytes)| entry(name, bytes, bytes.len() as u64 + extra))
            .collect();
        if listed.is_empty() {
            return reply(404, json!({ "message": "Not Found" }));
        }
        reply(200, Value::Array(listed))
    }

    fn put(&self, state: &mut RemoteState, path: &str, body: &Value) -> ResponseTemplate {
        if let Some(status) = state.fail_put {
            return reply(status, json!({ "message": "Server Error" }));
        }
        let (Some(_), Some(content)) = (body["message"].as_str(), body["content"].as_str()) else {
            return reply(422, json!({ "message": "Invalid request." }));
        };
        if state.files.contains_key(path) && body["sha"].as_str().is_none() {
            return reply(422, json!({ "message": "\"sha\" wasn't supplied." }));
        }
        let cleaned: String = content.chars().filter(|c| !c.is_whitespace()).collect();
        let Ok(bytes) = STANDARD.decode(cleaned) else {
            return reply(422, json!({ "message": "content is not valid Base64" }));
        };
        let size = bytes.len() as u64 + u64::from(state.lie_size);
        let made = entry(path, &bytes, size);
        state.files.insert(path.to_owned(), bytes);
        reply(
            201,
            json!({ "content": made, "commit": { "sha": "c0ffee" } }),
        )
    }

    fn delete(&self, state: &mut RemoteState, path: &str, body: &Value) -> ResponseTemplate {
        let Some(bytes) = state.files.get(path) else {
            return reply(404, json!({ "message": "Not Found" }));
        };
        if body["message"].as_str().is_none() || body["sha"].as_str() != Some(&fake_sha(bytes)) {
            return reply(409, json!({ "message": "sha does not match" }));
        }
        state.files.remove(path);
        reply(
            200,
            json!({ "content": null, "commit": { "sha": "c0ffee" } }),
        )
    }
}

impl Respond for Handler {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let auth = request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        let ok_auth = [format!("Bearer {TOKEN}"), format!("token {TOKEN}")];
        if !auth.is_some_and(|a| ok_auth.iter().any(|ok| ok == a)) {
            return reply(401, json!({ "message": "Bad credentials" }));
        }
        if request.headers.get("user-agent").is_none() {
            return reply(403, json!({ "message": "User-Agent required" }));
        }
        let Some(path) = request.url.path().strip_prefix(CONTENTS) else {
            return reply(404, json!({ "message": "Not Found" }));
        };
        let path = path.to_owned();
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        let mut state = self.0.lock().unwrap();
        match request.method.as_str() {
            "GET" => self.get(&state, &path),
            "PUT" => self.put(&mut state, &path, &body),
            "DELETE" => self.delete(&mut state, &path, &body),
            _ => reply(405, json!({ "message": "Method Not Allowed" })),
        }
    }
}

impl FakeGithub {
    pub async fn start() -> FakeGithub {
        let server = MockServer::start().await;
        let state = Arc::new(Mutex::new(RemoteState::default()));
        Mock::given(path_regex("^/"))
            .respond_with(Handler(state.clone()))
            .mount(&server)
            .await;
        FakeGithub { server, state }
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// 원격에 있는 파일 경로들.
    pub fn paths(&self) -> Vec<String> {
        self.state.lock().unwrap().files.keys().cloned().collect()
    }

    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        self.state.lock().unwrap().files.get(path).cloned()
    }

    pub fn put_file(&self, path: &str, bytes: &[u8]) {
        self.state
            .lock()
            .unwrap()
            .files
            .insert(path.to_owned(), bytes.to_vec());
    }

    pub fn fail_puts(&self, status: u16) {
        self.state.lock().unwrap().fail_put = Some(status);
    }

    pub fn lie_about_size(&self) {
        self.state.lock().unwrap().lie_size = true;
    }

    pub fn heal(&self) {
        let mut state = self.state.lock().unwrap();
        state.fail_put = None;
        state.lie_size = false;
    }

    pub async fn requests(&self) -> Vec<Request> {
        self.server.received_requests().await.unwrap_or_default()
    }

    pub async fn count(&self, method: &str) -> usize {
        self.requests()
            .await
            .iter()
            .filter(|r| r.method.as_str() == method)
            .count()
    }
}

/// 시험 환경: 보관 폴더(저장소 + 키 파일), 로컬 사본 폴더, 임시 폴더, 가짜 GitHub, age 키.
pub struct Env {
    pub dir: TempDir,
    pub store: Store,
    pub secrets: Arc<SecretStore>,
    pub settings: BackupSettings,
    pub identity: age::x25519::Identity,
    pub fake: FakeGithub,
}

impl Env {
    /// 토큰과 자료 키가 등록된 환경.
    pub async fn new() -> Env {
        let env = Env::without_token().await;
        register(&env.secrets, KeySlot::BackupGithubToken, "token", TOKEN).await;
        env
    }

    /// 자료 키만 등록되고 백업 토큰은 없는 환경.
    pub async fn without_token() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let store = Store::open(home.join(pi_backup::SNAPSHOT_FILE)).unwrap();
        let secrets = Arc::new(SecretStore::open(home).unwrap());
        register(&secrets, KeySlot::Dart, "api_key", DART_VALUE).await;
        let temp_root = dir.path().join("work");
        std::fs::create_dir(&temp_root).unwrap();
        let identity = age::x25519::Identity::generate();
        let fake = FakeGithub::start().await;
        let settings = BackupSettings {
            api_base: fake.uri(),
            repo: REPO.to_owned(),
            recipient: identity.to_public().to_string(),
            local_dir: dir.path().join("local"),
            temp_root,
        };
        Env {
            dir,
            store,
            secrets,
            settings,
            identity,
            fake,
        }
    }

    pub fn backup(&self) -> Backup {
        Backup::new(
            self.settings.clone(),
            self.store.clone(),
            self.secrets.clone(),
        )
        .unwrap()
    }

    pub fn identity_secret(&self) -> SecretString {
        SecretString::new(self.identity.to_string().expose_secret())
    }

    /// 이 환경의 공개키로 묶음을 암호화한다.
    pub fn seal(&self, bundle: &Bundle) -> Vec<u8> {
        pi_backup::seal(bundle, &self.settings.recipient).unwrap()
    }

    /// 새 되살리기 대상 경로(아직 만들지 않음).
    pub fn target(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}

async fn register(secrets: &SecretStore, slot: KeySlot, field: &str, value: &str) {
    let values = Credentials::new().with(field, value);
    secrets
        .register_slot(slot, values, &AcceptAll)
        .await
        .unwrap();
}

/// 기다림 승인 하나를 만든다. `note`는 요약에 그대로 들어간다(스냅샷 안 글자를 정할 때 쓴다).
pub fn pending_approval(store: &Store, note: &str, at: UnixMillis) -> ApprovalId {
    store
        .create_approval(NewApprovalRequest {
            fingerprint: "sha256:test".to_owned(),
            summary: json!({ "note": note }),
            created_at: at,
            expires_at: UnixMillis(at.0 + 30 * DAY),
        })
        .unwrap()
        .id
}

/// 폴더 안 이름들(정렬). 폴더가 없으면 빈 목록.
pub fn names_in(dir: &Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// 폴더 아래 모든 파일 내용(하위 폴더 포함).
pub fn all_bytes_under(dir: &Path) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for name in names_in(dir) {
        let path = dir.join(name);
        if path.is_dir() {
            out.extend(all_bytes_under(&path));
        } else {
            out.push(std::fs::read(path).unwrap());
        }
    }
    out
}

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

pub fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// 온전한 저장소 스냅샷 바이트(따로 연 저장소에서 뜬다).
pub fn snapshot_bytes() -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("src.sqlite")).unwrap();
    let out = dir.path().join("snap.sqlite");
    store.snapshot_to(&out).unwrap();
    std::fs::read(out).unwrap()
}

pub fn file(path: &str, bytes: &[u8]) -> (ManifestFile, BundleFile) {
    let listed = ManifestFile {
        path: path.to_owned(),
        size: bytes.len() as u64,
        sha256: sha_hex(bytes),
    };
    (
        listed,
        BundleFile {
            path: path.to_owned(),
            bytes: bytes.to_vec(),
        },
    )
}

/// 손으로 만든 묶음. `files`는 manifest와 내용에 같이 든다.
pub fn bundle_of(files: Vec<(ManifestFile, BundleFile)>) -> Bundle {
    let (listed, held) = files.into_iter().unzip();
    Bundle {
        manifest: Manifest {
            version: MANIFEST_VERSION,
            created_at: UnixMillis(1),
            files: listed,
        },
        files: held,
    }
}

/// 온전한 스냅샷 하나만 든 묶음.
pub fn good_bundle() -> Bundle {
    bundle_of(vec![file(pi_backup::SNAPSHOT_FILE, &snapshot_bytes())])
}

// ───── 한국 시각(UTC+9) 날짜 계산: 보존 규칙의 기대값을 따로 센다 ─────

const KST: i64 = 9 * HOUR;

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m)
}

/// 한국 시각 y-m-d h시.
pub fn kst(y: i64, m: i64, d: i64, h: i64) -> UnixMillis {
    UnixMillis(days_from_civil(y, m, d) * DAY + h * HOUR - KST)
}

pub fn plus_days(t: UnixMillis, days: i64) -> UnixMillis {
    UnixMillis(t.0 + days * DAY)
}

pub fn day_of(t: UnixMillis) -> i64 {
    (t.0 + KST).div_euclid(DAY)
}

/// ISO 주(월요일 시작). 1970-01-01은 목요일이다.
pub fn week_of(t: UnixMillis) -> i64 {
    (day_of(t) + 3).div_euclid(7)
}

pub fn month_of(t: UnixMillis) -> i64 {
    let (y, m) = civil_from_days(day_of(t));
    y * 12 + m
}
