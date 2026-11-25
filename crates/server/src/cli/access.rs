//! 터미널의 접속 키 만들기(`price-inha access-key [--rotate]`).
//!
//! 세대와 세션은 도는 서버의 메모리에 있다. 그래서 먼저 설정의 포트의 루프백에 화면과 같은 그 컴퓨터 전용
//! 경로(`/api/access/key`)로 요청해 **도는 서버가** 키를 만들게 한다(세대 올리기 · 앞 세대 원격 세션 끊기 ·
//! 열린 WebSocket 닫기가 그 서버 안에서 일어난다, V41). 연결 자체가 안 되면 도는 서버가 없는 것이니 같은
//! 저장소에 같은 규칙(소금 친 SHA-256만 저장, 새 세대, 접속 기록)으로 직접 만든다 — 이 길에서는 끊을 연결이
//! 없다. 그 포트에서 다른 프로그램이 답하면 키를 만들지 않는다([`CliError::NotOurServer`]).
//! (10차 10a 0번) 서비스 정의가 이 홈을 가리키면 그 정의의 주소에서도 도는 서버를 찾는다([`access_key_with`]).
//! 원본 동작: Hermes 2.3 · OpenClaw 3.3(화면 없는 컴퓨터의 처음 설정은 터미널에서 일회용 자격을 한 번 보여 준다).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use pi_gate::{Gate, GateConfig};
use pi_secrets::SecretStore;
use pi_store::Store;

use super::peek::stored_generation;
use super::service::Service;
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
    issue_at(settings, request, vec![Place::loopback(settings.port)]).await
}

/// (10차 10a 0번) 서비스 정의도 보고 접속 키를 만든다. 실행 파일의 `access-key`는 이 길을 쓴다.
///
/// [`access_key`]와 같되, 직접 만들기 전에 도는 서버를 더 찾는다(V41 — 앞 세대 원격 세션 · WebSocket이 남지 않게):
///
/// 1. 설정의 포트의 루프백([`access_key`]와 같다).
/// 2. `service`의 정의가 이 보관 폴더(`settings.home`)를 가리키면([`Service::installed_home`]) 그 정의에 실린
///    `--bind` · `--port` 인자의 루프백 포트. 다른 홈을 가리키는 정의는 쓰지 않는다.
/// 3. 설정의 여는 주소(`settings.bind:port`).
///
/// 어디서든 이 서버를 찾으면 그 서버가 키를 만들게 한다(세대 올리기 · 앞 세대 세션 끊기 · WebSocket 닫기가 그
/// 서버 안에서 일어난다). 도는 서버가 있는데 루프백 경로로 바꿀 수 없으면 키를 만들지 않고
/// [`CliError::NotOurServer`]다(안내: 서버를 멈추거나 `--port`를 맞게). 어디에도 없을 때만 직접 만들고, 그때
/// 안내 글은 "도는 서버를 찾지 못해 직접 만들었습니다"로 시작한다. `rotate` 없이 이미 키가 있으면 어느 길이든
/// [`CliError::AccessKeyExists`]. 발급 뒤 이 홈 저장소의 세대(읽기 전용으로 연다)가 서버가 답한 세대와 다르면
/// 안내 글에 알린다.
pub async fn access_key_with(
    settings: &Settings,
    request: &AccessKeyRequest,
    service: &Service<'_>,
) -> Result<ShownAccessKey, CliError> {
    issue_at(settings, request, places_for(settings, Some(service))).await
}

/// 도는 서버를 찾아볼 주소 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Place {
    host: IpAddr,
    port: u16,
}

impl Place {
    pub(crate) fn loopback(port: u16) -> Place {
        Place {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port,
        }
    }

    /// 여는 주소 → 물어볼 주소. 루프백이거나 모든 주소(`0.0.0.0` · `::`)면 루프백으로 묻는다.
    pub(crate) fn at(bind: IpAddr, port: u16) -> Place {
        if bind.is_loopback() || bind.is_unspecified() {
            Place::loopback(port)
        } else {
            Place { host: bind, port }
        }
    }
}

/// 어느 길로 만들었나(안내 글이 다르다).
enum How {
    /// 도는 서버가 만들었다(앞 세대 연결이 끊긴다).
    Server,
    /// 도는 서버가 만들었는데 이 홈 저장소의 세대가 다르다(다른 홈의 서버일 수 있다).
    ServerElsewhere,
    /// 도는 서버를 찾지 못해 직접 만들었다.
    Direct,
}

/// (37차) 도는 서버를 찾은 결과.
pub(crate) enum Located {
    /// 어디에도 없다.
    Nowhere,
    /// 루프백에서 이 서버가 돈다(그 포트).
    Ours(Running, u16),
    /// 그 포트에서 다른 프로그램이 답하거나, 서버가 도는데 루프백 경로가 아니다.
    NotOurs(u16),
}

/// (37차) [`access_key_with`]와 같은 차례의 찾을 자리: 설정의 포트의 루프백 → 이 홈을 가리키는 서비스 정의의
/// 포트(와 그 여는 주소) → 설정의 여는 주소. `setup` · `open`도 이 차례로 찾는다.
pub(crate) fn places_for(settings: &Settings, service: Option<&Service<'_>>) -> Vec<Place> {
    let mut places = vec![Place::loopback(settings.port)];
    if let Some(listen) = service.and_then(|service| service.installed_listen(&settings.home)) {
        let port = listen.port.unwrap_or(pi_gate::DEFAULT_PORT);
        places.push(Place::loopback(port));
        if let Some(bind) = listen.bind {
            places.push(Place::at(bind, port));
        }
    }
    places.push(Place::at(settings.bind, settings.port));
    places
}

/// `places`를 차례로 물어 도는 서버를 찾는다. 포트 0(빈 포트를 골라 연다)과 이미 물은 자리는 건너뛴다.
pub(crate) async fn locate(places: Vec<Place>) -> Located {
    let mut asked: Vec<Place> = Vec::new();
    for place in places {
        if place.port == 0 || asked.contains(&place) {
            continue;
        }
        asked.push(place);
        match Running::find(place).await {
            Found::None => {}
            Found::Ours(found) if place.host.is_loopback() => {
                return Located::Ours(found, place.port);
            }
            Found::Ours(_) | Found::Other => return Located::NotOurs(place.port),
        }
    }
    Located::Nowhere
}

/// `places`를 차례로 물어 도는 서버를 찾고, 있으면 그 서버가, 어디에도 없으면 직접 만든다.
async fn issue_at(
    settings: &Settings,
    request: &AccessKeyRequest,
    places: Vec<Place>,
) -> Result<ShownAccessKey, CliError> {
    let running = match locate(places).await {
        Located::Nowhere => None,
        Located::Ours(found, port) => Some((found, port)),
        Located::NotOurs(port) => return Err(CliError::NotOurServer { port }),
    };
    let (key, generation, how) = match running {
        None => {
            let (key, generation) = issue_directly(settings, request.rotate).await?;
            (key, generation, How::Direct)
        }
        Some((running, port)) => {
            // 그 서버가 이 보관 폴더로 도는 것이 아니면(저장소가 없다) 다른 서버다. 저장소를 새로 만들지 않고
            // 읽기만 한다(스키마를 올리지 않는다).
            let file = settings.home.join(DB_FILE);
            let Some(before) = stored_generation(&file) else {
                return Err(CliError::NotOurServer { port });
            };
            if before.is_some() && !request.rotate {
                return Err(CliError::AccessKeyExists);
            }
            let (key, generation) = running
                .issue()
                .await
                .ok_or(CliError::NotOurServer { port })?;
            let same = stored_generation(&file).flatten() == Some(generation);
            let how = if same {
                How::Server
            } else {
                How::ServerElsewhere
            };
            (key, generation, how)
        }
    };
    let text = shown_text(&key, generation, request.remote_name.as_deref(), &how);
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
pub(crate) struct Running {
    client: reqwest::Client,
    origin: String,
    cookie: String,
}

impl Running {
    /// 그 주소에 그 컴퓨터의 세션을 청해 본다. 프록시를 거치지 않고 리다이렉트를 따라가지 않는다.
    async fn find(place: Place) -> Found {
        let built = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(ASK_TIMEOUT)
            .build();
        let Ok(client) = built else {
            return Found::Other;
        };
        let origin = format!("http://{}", SocketAddr::new(place.host, place.port));
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

    /// (37차) 그 서버의 경로 하나를 화면과 같은 세션 · `Origin`으로 부른다. `body`가 있으면 `POST`(JSON),
    /// 없으면 `GET`. 답의 (상태, JSON 본문 — 읽지 못하면 `null`). 닿지 못하면 `None`.
    /// 본문(비밀값이 들 수 있다)은 기록하지 않는다.
    pub(crate) async fn send(
        &self,
        path: &str,
        body: Option<Vec<u8>>,
        timeout: Duration,
    ) -> Option<(u16, serde_json::Value)> {
        let url = format!("{}{path}", self.origin);
        let builder = match body {
            Some(body) => self
                .client
                .post(url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body),
            None => self.client.get(url),
        };
        let reply = builder
            .header(reqwest::header::ORIGIN, &self.origin)
            .header(reqwest::header::COOKIE, &self.cookie)
            .timeout(timeout)
            .send()
            .await
            .ok()?;
        let status = reply.status().as_u16();
        let bytes = reply.bytes().await.ok()?;
        let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        Some((status, value))
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
/// (도는 서버가 만든 길)에서만 한다.
fn shown_text(key: &str, generation: u64, remote_name: Option<&str>, how: &How) -> String {
    let how = match how {
        How::Server => "도는 서버가 만들었습니다. 앞 세대 키로 들어온 원격 연결은 끊깁니다.",
        How::ServerElsewhere => {
            "도는 서버가 만들었습니다. 앞 세대 키로 들어온 원격 연결은 끊깁니다. \
다만 이 보관 폴더의 저장소에 적힌 세대가 서버가 답한 세대와 다릅니다. 그 서버가 다른 보관 폴더로 \
도는 것일 수 있으니 --home 과 --port 를 확인하세요."
        }
        How::Direct => {
            "도는 서버를 찾지 못해 직접 만들었습니다. 다른 주소에서 돌고 있다면 서버를 다시 \
띄우세요(그래야 앞 세대 연결이 닫힙니다)."
        }
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
        let text = shown_text(
            "ABCDEFGHJKMNPQRSTVWX",
            3,
            Some("mini.example.test"),
            &How::Server,
        );
        assert_eq!(text.matches("ABCDEFGHJKMNPQRSTVWX").count(), 1);
        assert!(text.contains("https://mini.example.test/"));
        assert!(text.contains("끊깁니다"));
        let direct = shown_text("K", 1, None, &How::Direct);
        assert!(!direct.contains("https://"));
        assert!(!direct.contains("끊깁니다"), "끊을 연결이 없는 길");
    }
}
