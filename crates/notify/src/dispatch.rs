//! 구독마다 푸시 한 번씩: 동시에 보내고, 구독 순서로 결과를 모은다.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use pi_store::{PushSubscription, Store, UnixMillis};

use crate::{
    Attempt, AttemptOutcome, Clock, Error, PushRequest, PushTransport, Redactor, Urgency, VapidKey,
    encrypt_aes128gcm,
};

/// 푸시 서비스가 전하지 못한 알림을 들고 있을 시간(초). `TTL` 머리글.
const PUSH_TTL_SECS: u32 = 24 * 3_600;

/// 알림 보내기가 나누어 쓰는 것(보내는 일마다 따로 띄운다).
pub(crate) struct Shared {
    pub(crate) store: Store,
    pub(crate) vapid: VapidKey,
    pub(crate) subject: String,
    pub(crate) transport: Arc<dyn PushTransport>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) redactor: Arc<dyn Redactor>,
    pub(crate) attempt_timeout: Duration,
}

/// 모든 구독으로 동시에 보낸다. 결과는 구독 순서다.
pub(crate) async fn send_all(
    shared: &Arc<Shared>,
    subscriptions: Vec<PushSubscription>,
    urgency: Urgency,
    plaintext: Vec<u8>,
    now: UnixMillis,
) -> Vec<Attempt> {
    let plaintext = Arc::new(plaintext);
    let tasks: Vec<_> = subscriptions
        .into_iter()
        .map(|subscription| {
            let endpoint = subscription.endpoint.clone();
            let task = tokio::spawn(send_one(
                Arc::clone(shared),
                subscription,
                urgency,
                Arc::clone(&plaintext),
                now,
            ));
            (endpoint, task)
        })
        .collect();
    let mut attempts = Vec::with_capacity(tasks.len());
    for (endpoint, task) in tasks {
        let attempt = task.await.unwrap_or_else(|_| Attempt {
            endpoint,
            outcome: AttemptOutcome::Failed("보내는 일이 중간에 멈췄다".to_owned()),
        });
        attempts.push(attempt);
    }
    attempts
}

/// 구독 하나로 한 번 보낸다.
async fn send_one(
    shared: Arc<Shared>,
    subscription: PushSubscription,
    urgency: Urgency,
    plaintext: Arc<Vec<u8>>,
    now: UnixMillis,
) -> Attempt {
    let endpoint = subscription.endpoint.clone();
    let outcome = match build_request(&shared, &subscription, urgency, &plaintext, now) {
        Ok(request) => {
            let sent = tokio::time::timeout(shared.attempt_timeout, shared.transport.post(request));
            match sent.await {
                Err(_) => AttemptOutcome::TimedOut,
                Ok(Err(error)) => {
                    AttemptOutcome::Failed(shared.redactor.redact(&error.to_string()))
                }
                Ok(Ok(response)) => judge(&shared, &endpoint, response.status),
            }
        }
        Err(error) => AttemptOutcome::Failed(shared.redactor.redact(&error.to_string())),
    };
    Attempt { endpoint, outcome }
}

/// 상태 코드로 결과를 정한다. 404 · 410이면 구독을 지운다.
fn judge(shared: &Shared, endpoint: &str, status: u16) -> AttemptOutcome {
    match status {
        200..=299 => AttemptOutcome::Delivered { status },
        404 | 410 => match shared.store.remove_push_subscription(endpoint) {
            Ok(_) => AttemptOutcome::SubscriptionGone { status },
            Err(error) => AttemptOutcome::Failed(format!(
                "구독이 사라졌다는 답({status})을 받았지만 지우지 못했다: {error}"
            )),
        },
        _ => AttemptOutcome::Rejected { status },
    }
}

/// 암호화한 본문과 머리글로 요청을 만든다.
fn build_request(
    shared: &Shared,
    subscription: &PushSubscription,
    urgency: Urgency,
    plaintext: &[u8],
    now: UnixMillis,
) -> Result<PushRequest, Error> {
    let bad_key =
        |field: &str| Error::InvalidKey(format!("저장된 구독의 {field}가 base64url이 아니다"));
    let ua_public = URL_SAFE_NO_PAD
        .decode(subscription.p256dh.trim_end_matches('='))
        .map_err(|_| bad_key("p256dh"))?;
    let auth = URL_SAFE_NO_PAD
        .decode(subscription.auth.trim_end_matches('='))
        .map_err(|_| bad_key("auth"))?;
    let body = encrypt_aes128gcm(plaintext, &ua_public, &auth)?;
    let authorization = shared
        .vapid
        .authorization(&subscription.endpoint, &shared.subject, now)?;
    Ok(PushRequest {
        endpoint: subscription.endpoint.clone(),
        headers: vec![
            ("Authorization".to_owned(), authorization),
            ("Content-Encoding".to_owned(), "aes128gcm".to_owned()),
            (
                "Content-Type".to_owned(),
                "application/octet-stream".to_owned(),
            ),
            ("TTL".to_owned(), PUSH_TTL_SECS.to_string()),
            ("Urgency".to_owned(), urgency.header_value().to_owned()),
        ],
        body,
    })
}
