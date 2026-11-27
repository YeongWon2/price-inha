//! 가짜 GitHub contents API와 시험용 age 키(round24의 도우미를 본떴다 — 그 폴더는 고치지 않는다). 실제 GitHub는
//! 부르지 않는다.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use pi_secrets::SecretString;
use serde_json::{Value, json};
use wiremock::matchers::path_regex;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// 가짜 GitHub가 받는 토큰(시험 값).
pub const GITHUB_TOKEN: &str = "test-github-token-round62";
/// 가짜 비공개 저장소.
pub const REPO: &str = "owner/price-inha-backup";

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct Remote {
    files: BTreeMap<String, Vec<u8>>,
}

/// 상태를 가진 가짜 GitHub contents API(토큰을 따진다).
pub struct FakeGithub {
    server: MockServer,
    state: Arc<Mutex<Remote>>,
}

struct Contents(Arc<Mutex<Remote>>);

fn entry(path: &str, bytes: &[u8]) -> Value {
    let name = path.rsplit('/').next().unwrap_or(path);
    let sha = format!(
        "{:016x}",
        bytes.len() * 7919 + usize::from(bytes.first().copied().unwrap_or(0))
    );
    json!({"type": "file", "name": name, "path": path, "sha": sha, "size": bytes.len()})
}

impl Respond for Contents {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let reply = |status: u16, body: Value| ResponseTemplate::new(status).set_body_json(body);
        let auth = request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        let good = [
            format!("Bearer {GITHUB_TOKEN}"),
            format!("token {GITHUB_TOKEN}"),
        ];
        if !auth.is_some_and(|a| good.iter().any(|g| g == a)) {
            return reply(401, json!({"message": "Bad credentials"}));
        }
        let prefix = format!("/repos/{REPO}/contents/");
        let Some(file) = request.url.path().strip_prefix(&prefix) else {
            return reply(404, json!({"message": "Not Found"}));
        };
        let file = file.to_owned();
        let body: Value = serde_json::from_slice(&request.body).unwrap_or(Value::Null);
        let mut state = locked(&self.0);
        match request.method.as_str() {
            "PUT" => {
                let text: String = body["content"]
                    .as_str()
                    .unwrap_or_default()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                let Ok(bytes) = STANDARD.decode(text) else {
                    return reply(422, json!({"message": "content is not valid Base64"}));
                };
                let made = entry(&file, &bytes);
                state.files.insert(file, bytes);
                reply(201, json!({"content": made, "commit": {"sha": "c0ffee"}}))
            }
            "GET" => {
                if let Some(bytes) = state.files.get(&file) {
                    return reply(200, entry(&file, bytes));
                }
                let dir = format!("{}/", file.trim_end_matches('/'));
                let listed: Vec<Value> = state
                    .files
                    .iter()
                    .filter(|(name, _)| name.starts_with(&dir))
                    .map(|(name, bytes)| entry(name, bytes))
                    .collect();
                if listed.is_empty() {
                    return reply(404, json!({"message": "Not Found"}));
                }
                reply(200, Value::Array(listed))
            }
            "DELETE" => {
                state.files.remove(&file);
                reply(200, json!({"content": null, "commit": {"sha": "c0ffee"}}))
            }
            _ => reply(405, json!({"message": "Method Not Allowed"})),
        }
    }
}

impl FakeGithub {
    pub async fn start() -> FakeGithub {
        let server = MockServer::start().await;
        let state = Arc::new(Mutex::new(Remote::default()));
        Mock::given(path_regex("^/"))
            .respond_with(Contents(Arc::clone(&state)))
            .mount(&server)
            .await;
        FakeGithub { server, state }
    }

    /// 주소.
    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// 원격 파일 내용.
    pub fn file(&self, path: &str) -> Vec<u8> {
        locked(&self.state)
            .files
            .get(path)
            .cloned()
            .expect("원격 파일")
    }
}

/// 시험용 age 키 한 쌍(테스트 안에서 만든다).
pub struct AgeKeys {
    pub recipient: String,
    pub identity: SecretString,
}

impl AgeKeys {
    pub fn generate() -> AgeKeys {
        let identity = age::x25519::Identity::generate();
        let recipient = identity.to_public().to_string();
        let secret = identity.to_string();
        let identity = SecretString::new(age::secrecy::ExposeSecret::expose_secret(&secret));
        AgeKeys {
            recipient,
            identity,
        }
    }
}
