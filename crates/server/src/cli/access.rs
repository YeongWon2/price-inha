//! 터미널의 접속 키 만들기(`price-inha access-key [--rotate]`).
//!
//! 세대와 세션은 도는 서버의 메모리에 있다. 그래서 먼저 설정의 포트의 루프백에 화면과 같은 그 컴퓨터 전용
//! 경로(`/api/access/key`)로 요청해 **도는 서버가** 키를 만들게 한다(세대 올리기 · 앞 세대 원격 세션 끊기 ·
//! 열린 WebSocket 닫기가 그 서버 안에서 일어난다, V41). 연결 자체가 안 되면 도는 서버가 없는 것이니 같은
//! 저장소에 같은 규칙(소금 친 SHA-256만 저장, 새 세대, 접속 기록)으로 직접 만든다 — 이 길에서는 끊을 연결이
//! 없다. 그 포트에서 다른 프로그램이 답하면 키를 만들지 않는다([`CliError::NotOurServer`]).
//! 원본 동작: Hermes 2.3 · OpenClaw 3.3(화면 없는 컴퓨터의 처음 설정은 터미널에서 일회용 자격을 한 번 보여 준다).

use std::time::Duration;

use pi_gate::{Gate, GateConfig};
use pi_secrets::SecretStore;
use pi_store::Store;

use crate::error::CliError;
use crate::{ACCESS_KEY_PATH, AppError, DB_FILE, Settings};

/// `access-key`의 선택.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessKeyRequest {
    /// 이미 있는 키를 바꾼다.
    pub rotate: bool,
    /// 바깥에서 들어오는 이름(https 프록시 뒤의 원격 이름, 포트 없이). 있으면 `https://<이름>/` 안내를 함께.
    pub remote_name: Option<String>,
}

/// 터미널에 한 번 보여 줄 접속 키. `Debug`는 키를 찍지 않는다.
pub struct ShownAccessKey {
    key: String,
    generation: u64,
    text: String,
}

impl ShownAccessKey {
    /// 만든 키(터미널에 한 번만 보여 준다).
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 새 세대 번호.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// 터미널에 보여 줄 글. 키는 여기에 한 번만 나온다.
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl std::fmt::Debug for ShownAccessKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShownAccessKey")
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

/// 도는 서버에 묻는 한 요청의 상한(루프백이라 짧다).
const ASK_TIMEOUT: Duration = Duration::from_secs(10);
/// 루프백 연결을 기다리는 상한.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// 접속 키를 만든다. 이미 있으면 `rotate` 없이는 [`CliError::AccessKeyExists`]이고 그대로 둔다.
///
/// 설정의 포트에서 이 서버가 돌고 있으면 그 서버가 만들게 하고, 연결이 안 되면 직접 만든다. 그 포트에서
/// 다른 프로그램이 답하면 [`CliError::NotOurServer`]이고 아무것도 만들지 않는다.
/// 키 글자는 답 말고는 어디에도(기록 · 파일 · 오류) 남기지 않는다.
pub async fn access_key(
    settings: &Settings,
    request: &AccessKeyRequest,
) -> Result<ShownAccessKey, CliError> {
    let port = settings.port;
    let (key, generation, via_server) = match Running::find(port).await {
        Found::None => {
            let (key, generation) = issue_directly(settings, request.rotate).await?;
            (key, generation, false)
        }
        Found::Other => return Err(CliError::NotOurServer { port }),
        Found::Ours(running) => {
            // 그 서버가 이 보관 폴더로 도는 것이 아니면(저장소가 없다) 다른 서버다. 저장소를 새로 만들지 않는다.
            let file = settings.home.join(DB_FILE);
            if !file.is_file() {
                return Err(CliError::NotOurServer { port });
            }
            let has_key = {
                let store = Store::open(file).map_err(AppError::from)?;
                store.access_key().map_err(AppError::from)?.is_some()
            };
            if has_key && !request.rotate {
                return Err(CliError::AccessKeyExists);
            }
            let (key, generation) = running
                .issue()
                .await
                .ok_or(CliError::NotOurServer { port })?;
            (key, generation, true)
        }
    };
    let text = shown_text(&key, generation, request.remote_name.as_deref(), via_server);
    Ok(ShownAccessKey {
        key,
        generation,
        text,
    })
}

/// 도는 서버가 없을 때: 같은 저장소에 직접 만든다. (키, 세대).
async fn issue_directly(settings: &Settings, rotate: bool) -> Result<(String, u64), CliError> {
    // 보관 폴더가 없으면 서버가 켤 때처럼 만든다(0700).
    SecretStore::open(settings.home.clone()).map_err(AppError::from)?;
    let store = Store::open(settings.home.join(DB_FILE)).map_err(AppError::from)?;
    if store.access_key().map_err(AppError::from)?.is_some() && !rotate {
        return Err(CliError::AccessKeyExists);
    }
    // 화면의 `/api/access/key`와 같은 손잡이(`pi_gate::AccessKeys::issue`)로 만든다: 같은 저장소 · 같은 규칙
    // (운영체제 난수, 소금 친 SHA-256만 저장, 새 세대, 접속 기록). 관문은 루프백 빈 포트에 잠깐만 연다.
    let gate = Gate::builder(store, GateConfig::local(0))
        .build()
        .serve()
        .await
        .map_err(AppError::from)?;
    let issued = gate.access_keys().issue().await;
    let closed = gate.shutdown().await;
    let issued = issued.map_err(AppError::from)?;
    closed.map_err(AppError::from)?;
    Ok((issued.key, issued.generation))
}

/// 설정의 포트에 물어본 결과.
enum Found {
    /// 연결이 안 된다(도는 서버 없음).
    None,
    /// 무엇인가 답하지만 이 서버의 모양이 아니다.
    Other,
    /// 이 서버가 돈다.
    Ours(Running),
}

/// 루프백에서 도는 이 서버와, 그 컴퓨터의 세션(화면이 받는 것과 같은 쿠키).
struct Running {
    client: reqwest::Client,
    origin: String,
    cookie: String,
}

impl Running {
    /// `127.0.0.1:<port>`에 그 컴퓨터의 세션을 청해 본다. 프록시를 거치지 않고 리다이렉트를 따라가지 않는다.
    async fn find(port: u16) -> Found {
        // 포트 0은 "빈 포트를 골라 연다"는 설정이라 물어볼 주소가 없다.
        if port == 0 {
            return Found::None;
        }
        let built = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(ASK_TIMEOUT)
            .build();
        let Ok(client) = built else {
            return Found::Other;
        };
        let origin = format!("http://127.0.0.1:{port}");
        let asked = client
            .post(format!("{origin}{}", pi_gate::SESSION_PATH))
            .header(reqwest::header::ORIGIN, &origin)
            .send()
            .await;
        let reply = match asked {
            Ok(reply) => reply,
            // 연결 자체가 안 된다(거부 · 주소 없음 등): 도는 서버가 없다.
            Err(error) if error.is_connect() => return Found::None,
            Err(_) => return Found::Other,
        };
        // 이 서버는 그 컴퓨터의 세션 청에 204와 세션 쿠키로 답한다(`pi_gate`의 세션 경로).
        let cookie = reply
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .and_then(|raw| raw.split(';').next())
            .map(str::trim)
            .filter(|pair| {
                pair.strip_prefix(pi_gate::SESSION_COOKIE)
                    .and_then(|rest| rest.strip_prefix('='))
                    .is_some_and(|value| !value.is_empty())
            });
        match cookie {
            Some(cookie) if reply.status() == reqwest::StatusCode::NO_CONTENT => {
                Found::Ours(Running {
                    cookie: cookie.to_string(),
                    client,
                    origin,
                })
            }
            _ => Found::Other,
        }
    }

    /// 그 서버가 키를 만들게 한다(화면의 발급과 같은 경로). 답이 발급 답의 모양이 아니면 `None`.
    async fn issue(&self) -> Option<(String, u64)> {
        let reply = self
            .client
            .post(format!("{}{ACCESS_KEY_PATH}", self.origin))
            .header(reqwest::header::ORIGIN, &self.origin)
            .header(reqwest::header::COOKIE, &self.cookie)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body("{}")
            .send()
            .await
            .ok()?;
        if reply.status() != reqwest::StatusCode::OK {
            return None;
        }
        let body = reply.bytes().await.ok()?;
        let value: serde_json::Value = serde_json::from_slice(&body).ok()?;
        let key = value.get(pi_gate::ACCESS_KEY_FIELD)?.as_str()?;
        let generation = value.get("generation")?.as_u64()?;
        (!key.is_empty()).then(|| (key.to_string(), generation))
    }
}

/// 터미널에 보여 줄 글. 키는 한 번만 나온다. 앞 세대 연결이 끊긴다는 말은 실제로 끊기는 길
/// (`via_server`: 도는 서버가 만든 길)에서만 한다.
fn shown_text(key: &str, generation: u64, remote_name: Option<&str>, via_server: bool) -> String {
    let how = if via_server {
        "도는 서버가 만들었습니다. 앞 세대 키로 들어온 원격 연결은 끊깁니다."
    } else {
        "도는 서버가 없어 직접 만들었습니다."
    };
    let mut text = format!(
        "새 접속 키(세대 {generation}):\n\n    {key}\n\n\
이 키는 지금 한 번만 보여 줍니다. 저장소에는 해시만 남습니다. {how}\n"
    );
    match remote_name {
        Some(name) => text.push_str(&format!(
            "다른 기기의 브라우저에서 https://{name}/ 로 들어와 이 키를 넣습니다. \
그다음 설정 화면에서 키를 등록합니다.\n"
        )),
        None => text.push_str(
            "다른 기기의 브라우저에서 이 서버로 들어와 이 키를 넣습니다. 그다음 설정 화면에서 키를 등록합니다.\n",
        ),
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 키는_글에_한_번만() {
        let text = shown_text("ABCDEFGHJKMNPQRSTVWX", 3, Some("mini.example.test"), true);
        assert_eq!(text.matches("ABCDEFGHJKMNPQRSTVWX").count(), 1);
        assert!(text.contains("https://mini.example.test/"));
        assert!(text.contains("끊깁니다"));
        let direct = shown_text("K", 1, None, false);
        assert!(!direct.contains("https://"));
        assert!(!direct.contains("끊깁니다"), "끊을 연결이 없는 길");
    }
}
