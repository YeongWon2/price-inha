//! 시험 도구: 가짜 푸시 서비스(로컬), 시험용 구독자(키를 테스트가 쥔다), 가짜 시계, 가리기, 풀기.
//!
//! 바깥 푸시 서비스 주소는 쓰지 않는다. 구독 주소는 `https://push.invalid/...`이고,
//! 실제로는 `HttpTransport::redirect_to`로 로컬 가짜 서버(127.0.0.1)에 닿는다.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aes_gcm::aead::Aead;
use aes_gcm::{Aes128Gcm, KeyInit, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hkdf::Hkdf;
use p256::PublicKey;
use p256::SecretKey;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use pi_notify::{
    HttpTransport, NewNotice, NewSubscription, NoticeKind, Notifier, NotifierConfig, QuietHours,
    Redactor, VapidKey,
};
use pi_store::{Store, UnixMillis};
use sha2::Sha256;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

/// 시험의 시작 시각(2027-01-15 08:00:00 UTC쯤).
pub const T0: i64 = 1_800_000_000_000;
/// 1초(밀리초).
pub const SEC: i64 = 1_000;
/// 시험용 푸시 서비스 주소의 앞부분.
pub const PUSH_ORIGIN: &str = "https://push.invalid";
/// 시험용 VAPID `sub`.
pub const SUBJECT: &str = "mailto:owner@example.test";
/// 시험용 비밀 글자. 진짜 키처럼 보이지 않는다.
pub const SECRET_TEXT: &str = "ZQ7비밀ZQ7";

// ───────────── 시계 · 가리기 ─────────────

/// 테스트가 정하는 시계.
pub struct FakeClock(AtomicI64);

impl FakeClock {
    pub fn at(ms: i64) -> Arc<FakeClock> {
        Arc::new(FakeClock(AtomicI64::new(ms)))
    }
    pub fn set(&self, ms: i64) {
        self.0.store(ms, Ordering::SeqCst);
    }
}

impl pi_notify::Clock for FakeClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.load(Ordering::SeqCst))
    }
}

/// [`SECRET_TEXT`]를 `●●●`로 가린다.
pub struct MaskSecret;

impl Redactor for MaskSecret {
    fn redact(&self, text: &str) -> String {
        text.replace(SECRET_TEXT, "●●●")
    }
}

// ───────────── 시험용 구독자 ─────────────

/// 키를 테스트가 쥔 구독자. 보낸 본문을 풀어 본다.
pub struct Subscriber {
    pub endpoint: String,
    secret: SecretKey,
    pub public: Vec<u8>,
    pub auth: Vec<u8>,
}

impl Subscriber {
    /// `seed`로 같은 키가 매번 나온다(`seed`는 0이 아니어야 한다).
    pub fn new(seed: u8) -> Subscriber {
        let secret = SecretKey::from_slice(&[seed; 32]).unwrap();
        let public = secret.public_key().to_sec1_bytes().to_vec();
        Subscriber {
            endpoint: format!("{PUSH_ORIGIN}/send/{seed}"),
            secret,
            public,
            auth: vec![seed ^ 0x55; 16],
        }
    }

    pub fn new_subscription(&self) -> NewSubscription {
        NewSubscription {
            endpoint: self.endpoint.clone(),
            p256dh: URL_SAFE_NO_PAD.encode(&self.public),
            auth: URL_SAFE_NO_PAD.encode(&self.auth),
            device: "시험 폰".to_owned(),
        }
    }

    /// 요청 본문을 풀어 글로 돌려준다.
    pub fn open(&self, body: &[u8]) -> String {
        let plain = decrypt_aes128gcm(body, &self.secret, &self.public, &self.auth);
        String::from_utf8(plain).unwrap()
    }
}

/// RFC 8291 `aes128gcm`(한 레코드)을 푼다. 보낸 쪽의 공개키는 본문 머리의 `keyid`에 있다.
pub fn decrypt_aes128gcm(
    body: &[u8],
    ua_secret: &SecretKey,
    ua_public: &[u8],
    auth_secret: &[u8],
) -> Vec<u8> {
    assert!(body.len() > 21 + 65 + 16, "본문이 머리보다 짧다");
    let salt = &body[..16];
    let record_size = u32::from_be_bytes([body[16], body[17], body[18], body[19]]);
    let key_len = usize::from(body[20]);
    assert_eq!(key_len, 65, "keyid는 보낸 쪽 공개키(65바이트)여야 한다");
    let as_public = &body[21..21 + key_len];
    let record = &body[21 + key_len..];
    assert_eq!(record_size, 4096);
    assert!(record.len() <= 4096);

    let as_key = PublicKey::from_sec1_bytes(as_public).unwrap();
    let shared = p256::ecdh::diffie_hellman(ua_secret.to_nonzero_scalar(), as_key.as_affine());

    let mut key_info = b"WebPush: info\0".to_vec();
    key_info.extend_from_slice(ua_public);
    key_info.extend_from_slice(as_public);
    let mut ikm = [0u8; 32];
    Hkdf::<Sha256>::new(Some(auth_secret), shared.raw_secret_bytes())
        .expand(&key_info, &mut ikm)
        .unwrap();

    let prk = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut cek = [0u8; 16];
    prk.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .unwrap();
    let mut nonce = [0u8; 12];
    prk.expand(b"Content-Encoding: nonce\0", &mut nonce)
        .unwrap();

    let cipher = Aes128Gcm::new_from_slice(&cek).unwrap();
    let nonce = Nonce::try_from(nonce.as_slice()).unwrap();
    let mut plain = cipher.decrypt(&nonce, record).unwrap();
    // 끝의 0을 걷고 구분 글자(0x02, 마지막 레코드)를 뗀다.
    while plain.last() == Some(&0) {
        plain.pop();
    }
    assert_eq!(plain.pop(), Some(2), "마지막 레코드의 구분 글자는 0x02다");
    plain
}

// ───────────── VAPID 확인 ─────────────

/// 검증한 VAPID 내용.
pub struct VapidClaims {
    pub aud: String,
    pub sub: String,
    pub exp: i64,
    pub header: serde_json::Value,
}

/// `Authorization: vapid t=<JWT>, k=<공개키>` 값을 확인한다: 모양, `k`, ES256 서명.
pub fn check_vapid(authorization: &str, expected_public_b64: &str) -> VapidClaims {
    let rest = authorization.strip_prefix("vapid ").unwrap();
    let (t_part, k_part) = rest.split_once(", ").unwrap();
    let jwt = t_part.strip_prefix("t=").unwrap();
    let key = k_part.strip_prefix("k=").unwrap();
    assert_eq!(key, expected_public_b64);

    let parts: Vec<&str> = jwt.split('.').collect();
    assert_eq!(parts.len(), 3);
    let signed = format!("{}.{}", parts[0], parts[1]);
    let signature_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    assert_eq!(signature_bytes.len(), 64, "ES256 서명은 r||s 64바이트");
    let signature = Signature::from_slice(&signature_bytes).unwrap();
    let public = URL_SAFE_NO_PAD.decode(key).unwrap();
    let verifying = VerifyingKey::from_sec1_bytes(&public).unwrap();
    verifying.verify(signed.as_bytes(), &signature).unwrap();

    let header: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    VapidClaims {
        aud: claims["aud"].as_str().unwrap().to_owned(),
        sub: claims["sub"].as_str().unwrap().to_owned(),
        exp: claims["exp"].as_i64().unwrap(),
        header,
    }
}

// ───────────── 가짜 푸시 서비스(로컬) ─────────────

/// 가짜 서버가 경로마다 할 일.
#[derive(Clone, Copy)]
pub enum Behavior {
    /// 이 상태 코드로 답한다.
    Status(u16),
    /// 받기만 하고 답하지 않는다(제한 시간 시험).
    Hang,
}

/// 받은 요청 하나.
#[derive(Clone)]
pub struct Received {
    pub path: String,
    /// 머리글 이름은 소문자.
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Default)]
struct FakeState {
    received: Vec<Received>,
    behaviors: BTreeMap<String, Behavior>,
}

/// 로컬 가짜 푸시 서비스.
pub struct FakePush {
    base: Url,
    state: Arc<Mutex<FakeState>>,
}

impl FakePush {
    pub async fn start() -> FakePush {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let state = Arc::new(Mutex::new(FakeState::default()));
        let shared = Arc::clone(&state);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(serve(socket, Arc::clone(&shared)));
            }
        });
        FakePush { base, state }
    }

    pub fn base(&self) -> Url {
        self.base.clone()
    }

    /// 이 경로로 온 요청에 어떻게 답할지(기본은 201).
    pub fn behave(&self, path: &str, behavior: Behavior) {
        self.state
            .lock()
            .unwrap()
            .behaviors
            .insert(path.to_owned(), behavior);
    }

    pub fn received(&self) -> Vec<Received> {
        self.state.lock().unwrap().received.clone()
    }

    /// 이 경로로 받은 요청.
    pub fn received_at(&self, path: &str) -> Vec<Received> {
        self.received()
            .into_iter()
            .filter(|r| r.path == path)
            .collect()
    }
}

async fn serve(mut socket: tokio::net::TcpStream, state: Arc<Mutex<FakeState>>) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let Ok(n) = socket.read(&mut chunk).await else {
            return;
        };
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let path = request_line
        .split(' ')
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let mut headers = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let want: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body = buf[head_end..].to_vec();
    while body.len() < want {
        let Ok(n) = socket.read(&mut chunk).await else {
            return;
        };
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    let behavior = {
        let mut guard = state.lock().unwrap();
        guard.received.push(Received {
            path: path.clone(),
            headers,
            body,
        });
        guard.behaviors.get(&path).copied()
    };
    match behavior.unwrap_or(Behavior::Status(201)) {
        Behavior::Status(code) => {
            let reply =
                format!("HTTP/1.1 {code} X\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
            let _ = socket.write_all(reply.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
        Behavior::Hang => {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    }
}

// ───────────── 한 판에 필요한 것 ─────────────

pub struct Env {
    /// 임시 폴더를 테스트 끝까지 살려 둔다.
    pub dir: TempDir,
    pub store: Store,
    pub clock: Arc<FakeClock>,
    pub push: FakePush,
    pub notifier: Arc<Notifier>,
}

/// 같은 저장소 · 시계 · 가짜 서버로 알림 보내기를 새로 만든다(다시 켜기 시험에도 쓴다).
pub fn build_notifier(
    store: &Store,
    clock: &Arc<FakeClock>,
    base: Url,
    quiet: Option<QuietHours>,
    timeout_ms: u64,
) -> Arc<Notifier> {
    let transport = HttpTransport::new(Duration::from_secs(5))
        .unwrap()
        .redirect_to(base);
    Arc::new(Notifier::new(NotifierConfig {
        store: store.clone(),
        vapid: VapidKey::generate(),
        subject: SUBJECT.to_owned(),
        transport: Arc::new(transport),
        clock: clock.clone(),
        redactor: Arc::new(MaskSecret),
        quiet_hours: quiet,
        attempt_timeout: Duration::from_millis(timeout_ms),
    }))
}

/// 가짜 서버 + 실제 저장소 + 알림 보내기. 가리기는 [`MaskSecret`], 제한 시간은 `timeout_ms`.
pub async fn env_with(quiet: Option<QuietHours>, timeout_ms: u64) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("pi.sqlite")).unwrap();
    let clock = FakeClock::at(T0);
    let push = FakePush::start().await;
    let notifier = build_notifier(&store, &clock, push.base(), quiet, timeout_ms);
    Env {
        dir,
        store,
        clock,
        push,
        notifier,
    }
}

pub async fn env() -> Env {
    env_with(None, 2_000).await
}

impl Env {
    /// 구독자를 만들어 구독으로 둔다.
    pub fn subscribe(&self, seed: u8) -> Subscriber {
        let subscriber = Subscriber::new(seed);
        self.notifier
            .subscribe(subscriber.new_subscription())
            .unwrap();
        subscriber
    }
}

/// 구독자 경로(가짜 서버가 보는 경로).
pub fn path_of(subscriber: &Subscriber) -> String {
    subscriber
        .endpoint
        .strip_prefix(PUSH_ORIGIN)
        .unwrap()
        .to_owned()
}

/// 알림 하나(주소 없음). 일이 생긴 시각은 `at`.
pub fn notice(kind: NoticeKind, at: i64) -> NewNotice {
    NewNotice {
        kind,
        title: "제목".to_owned(),
        body: "본문".to_owned(),
        link: None,
        occurred_at: UnixMillis(at),
    }
}

/// 받은 요청 하나를 풀어 JSON으로.
pub fn open_json(subscriber: &Subscriber, received: &Received) -> serde_json::Value {
    serde_json::from_str(&subscriber.open(&received.body)).unwrap()
}

/// 이 날 0시(UTC)의 시각. `T0`는 그날 08:00 UTC다.
pub const DAY0: i64 = T0 - 8 * 3_600 * SEC;

/// 한국 시각(UTC+9) `hour:minute`의 시각. 하루 앞뒤로 넘어가도 된다.
pub fn kst(hour: i64, minute: i64) -> i64 {
    DAY0 + (hour * 60 + minute - 540) * 60 * SEC
}

/// 한국 시각 밤 10시~아침 7시(자정을 넘는 조용한 시간).
pub fn night_quiet() -> QuietHours {
    QuietHours::new(22 * 60, 7 * 60, 540).unwrap()
}
