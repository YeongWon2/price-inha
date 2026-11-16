//! 알림 보내기: 번호 있는 이벤트로 쌓고 구독마다 웹 푸시를 보낸다.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use pi_store::{EventRange, NewPushSubscription, PushSubscription, Store, TimedEvent, UnixMillis};

use crate::dispatch::{Shared, send_all};
use crate::webpush::{check_auth_secret, parse_ua_public};
use crate::{Clock, Error, NoticeKind, PushTransport, QuietHours, Redactor, VapidKey};

/// 일이 생긴 뒤 첫 시도까지 허락하는 시간(밀리초). N5 · V11.
pub const FIRST_ATTEMPT_WITHIN_MS: i64 = 10_000;

/// 푸시 제목(글자 수)의 상한. 넘으면 가린 뒤에 잘라 보낸다.
pub const MAX_PUSH_TITLE_CHARS: usize = 100;

/// 푸시 본문(글자 수)의 상한. 푸시에는 내용을 최소로 싣는다. 넘으면 가린 뒤에 잘라 보낸다
/// (이벤트에는 가린 전체 글이 남는다).
pub const MAX_PUSH_BODY_CHARS: usize = 200;

/// 푸시 주소(`link`, 글자 수)의 상한. 가린 주소가 이보다 길면 푸시에는 `link` 칸을 싣지 않는다
/// (반쯤 자른 주소는 엉뚱한 곳을 연다). 이벤트에는 가린 전체 주소가 남는다.
/// 제목 · 본문 · 주소가 모두 상한이고 글자마다 JSON 이스케이프가 가장 길어도(6바이트)
/// 한 레코드([`crate::MAX_PLAINTEXT`])에 들어간다.
pub const MAX_PUSH_LINK_CHARS: usize = 300;

/// 보낼 알림 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewNotice {
    /// 종류.
    pub kind: NoticeKind,
    /// 제목(쉬운 말).
    pub title: String,
    /// 본문(쉬운 말).
    pub body: String,
    /// 눌렀을 때 열 폰 화면 안의 주소(예: 승인 딥링크). 없을 수 있다.
    pub link: Option<String>,
    /// 일이 생긴 시각. 첫 시도는 이 시각부터 10초 안이어야 한다.
    pub occurred_at: UnixMillis,
}

/// 알림 목록의 한 줄(저장된 이벤트에서 읽는다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeRecord {
    /// 이벤트 번호.
    pub seq: u64,
    /// 종류.
    pub kind: NoticeKind,
    /// 이벤트 시각(알림이 생긴 시각).
    pub at: Option<UnixMillis>,
    /// 가린 제목.
    pub title: String,
    /// 가린 본문(자르지 않은 전체).
    pub body: String,
    /// 주소.
    pub link: Option<String>,
}

/// 푸시를 보내지 않고 둔 까닭.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldReason {
    /// 조용한 시간이다.
    QuietHours,
}

/// 구독 하나로 보낸 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// 푸시 서비스가 받았다(2xx).
    Delivered {
        /// 상태 코드.
        status: u16,
    },
    /// 404 · 410: 구독이 사라졌다. 구독을 지웠다.
    SubscriptionGone {
        /// 상태 코드.
        status: u16,
    },
    /// 그 밖의 거부(4xx · 5xx). 구독은 그대로 둔다.
    Rejected {
        /// 상태 코드.
        status: u16,
    },
    /// 연결 · 읽기 실패. 구독은 그대로 둔다.
    Failed(String),
    /// 제한 시간 안에 응답이 없었다. 구독은 그대로 둔다.
    TimedOut,
}

/// 구독 하나에 한 시도.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    /// 구독 주소.
    pub endpoint: String,
    /// 결과.
    pub outcome: AttemptOutcome,
}

/// 알림 하나를 보낸 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    /// 이벤트 번호. 푸시를 못 보내도(구독 없음 · 조용한 시간 · 실패) 번호는 있다.
    pub seq: u64,
    /// 푸시를 두었으면 까닭.
    pub held: Option<HoldReason>,
    /// 첫 시도를 시작한 시각(시계 포트). 시도하지 않았으면 `None`.
    pub first_attempt_at: Option<UnixMillis>,
    /// 구독마다 한 시도(구독 순서).
    pub attempts: Vec<Attempt>,
}

impl Delivery {
    /// 첫 시도가 `occurred_at`부터 [`FIRST_ATTEMPT_WITHIN_MS`] 안인가. 시도하지 않았으면 거짓.
    pub fn within_deadline(&self, occurred_at: UnixMillis) -> bool {
        self.first_attempt_at
            .is_some_and(|at| at.0.saturating_sub(occurred_at.0) <= FIRST_ATTEMPT_WITHIN_MS)
    }
}

/// 구독 신청(브라우저의 `PushSubscription`이 주는 값).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSubscription {
    /// 푸시 서비스 주소(`https://`).
    pub endpoint: String,
    /// 공개키(base64url, 비압축 점 65바이트).
    pub p256dh: String,
    /// 인증 값(base64url, 16바이트).
    pub auth: String,
    /// 기기 이름표.
    pub device: String,
}

/// [`Notifier`]를 만드는 값.
pub struct NotifierConfig {
    /// 저장소(이벤트, 구독 표).
    pub store: Store,
    /// VAPID 서명 키.
    pub vapid: VapidKey,
    /// VAPID `sub`(예: `mailto:` 또는 `https:` 주소).
    pub subject: String,
    /// 푸시 서비스로 보내는 길.
    pub transport: Arc<dyn PushTransport>,
    /// 시계.
    pub clock: Arc<dyn Clock>,
    /// 가리기.
    pub redactor: Arc<dyn Redactor>,
    /// 조용한 시간. 기본값은 없다(`None`).
    pub quiet_hours: Option<QuietHours>,
    /// 구독 하나로 보내는 시도의 제한 시간. 넘으면 [`AttemptOutcome::TimedOut`].
    pub attempt_timeout: Duration,
}

/// 알림을 쌓고 보낸다. 복제하지 않고 `Arc`로 나눈다.
pub struct Notifier {
    shared: Arc<Shared>,
    quiet_hours: Mutex<Option<QuietHours>>,
}

impl Notifier {
    /// 만든다.
    pub fn new(config: NotifierConfig) -> Notifier {
        Notifier {
            shared: Arc::new(Shared {
                store: config.store,
                vapid: config.vapid,
                subject: config.subject,
                transport: config.transport,
                clock: config.clock,
                redactor: config.redactor,
                attempt_timeout: config.attempt_timeout,
            }),
            quiet_hours: Mutex::new(config.quiet_hours),
        }
    }

    /// 조용한 시간을 바꾼다(`None`이면 없앤다).
    pub fn set_quiet_hours(&self, quiet_hours: Option<QuietHours>) {
        *self
            .quiet_hours
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = quiet_hours;
    }

    /// 브라우저에 줄 VAPID 공개키(base64url).
    pub fn vapid_public_key(&self) -> String {
        self.shared.vapid.public_key_b64url()
    }

    /// 구독을 둔다. 시각은 시계 포트다. `p256dh`가 base64url로 풀어 65바이트 점이 아니거나 `auth`가
    /// 16바이트가 아니면 [`Error::InvalidKey`]이고 아무것도 두지 않는다. 주소 검사는 저장소가 한다.
    pub fn subscribe(&self, new: NewSubscription) -> Result<PushSubscription, Error> {
        let public = decode_b64url(&new.p256dh, "p256dh")?;
        parse_ua_public(&public)?;
        check_auth_secret(&decode_b64url(&new.auth, "auth")?)?;
        let saved = self
            .shared
            .store
            .save_push_subscription(NewPushSubscription {
                endpoint: new.endpoint,
                p256dh: new.p256dh,
                auth: new.auth,
                device: new.device,
                at: self.shared.clock.now(),
            })?;
        Ok(saved)
    }

    /// 구독을 지운다. 있어서 지웠으면 `true`.
    pub fn unsubscribe(&self, endpoint: &str) -> Result<bool, Error> {
        Ok(self.shared.store.remove_push_subscription(endpoint)?)
    }

    /// 알림을 보낸다.
    ///
    /// 1. 제목 · 본문 · 주소를 가린다(E38). 가린 글만 쓴다(자르기 전에 가린다).
    /// 2. 시각 `occurred_at`으로 이벤트(`notice.<이름>`)를 쌓아 번호를 받는다(구독이 없어도, 조용한 시간이어도).
    ///    이벤트 내용은 JSON `{"title","body"}`에 주소가 있으면 `"link"`를 더한 것이다(가린 전체 글, 자르지 않음).
    ///    조용한 시간은 시계의 지금으로 판단한다.
    /// 3. 조용한 시간이고 [`NoticeKind::ignores_quiet_hours`]가 거짓이면 푸시를 보내지 않는다
    ///    (`held = Some(QuietHours)`).
    /// 4. 아니면 모든 구독으로 **동시에** 한 번씩 보낸다. 지금(시계)을 첫 시도 시각으로 남긴다(구독이 없으면 `None`).
    ///    본문(암호화 전)은 JSON `{"kind","title","body","seq"}`에 주소가 있으면 `"link"`를 더한 것이다.
    ///    `kind`는 [`NoticeKind::name`], `seq`는 이벤트 번호, 제목 · 본문은 각각
    ///    [`MAX_PUSH_TITLE_CHARS`] · [`MAX_PUSH_BODY_CHARS`]글자까지. 주소가 [`MAX_PUSH_LINK_CHARS`]글자를
    ///    넘으면 푸시에는 `link` 칸을 싣지 않는다.
    ///    머리글: `Authorization`(VAPID) · `Content-Encoding: aes128gcm` · `Content-Type: application/octet-stream`
    ///    · `TTL`(초, 양의 정수) · `Urgency`([`NoticeKind::urgency`]).
    ///    구독 하나가 느려도 다른 구독의 첫 시도를 늦추지 않는다.
    ///    404 · 410이면 그 구독을 지운다. 다른 실패는 지우지 않는다.
    pub async fn notify(&self, notice: NewNotice) -> Result<Delivery, Error> {
        let redactor = &self.shared.redactor;
        let title = redactor.redact(&notice.title);
        let body = redactor.redact(&notice.body);
        let link = notice.link.as_deref().map(|l| redactor.redact(l));

        let mut event = serde_json::json!({"title": title, "body": body});
        if let Some(link) = &link {
            event["link"] = serde_json::Value::from(link.as_str());
        }
        let seq =
            self.shared
                .store
                .append_at(notice.kind.event_kind(), &event, notice.occurred_at)?;

        let now = self.shared.clock.now();
        let quiet = *self
            .quiet_hours
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !notice.kind.ignores_quiet_hours() && quiet.is_some_and(|q| q.contains(now)) {
            return Ok(Delivery {
                seq,
                held: Some(HoldReason::QuietHours),
                first_attempt_at: None,
                attempts: Vec::new(),
            });
        }

        let subscriptions = self.shared.store.push_subscriptions()?;
        if subscriptions.is_empty() {
            return Ok(Delivery {
                seq,
                held: None,
                first_attempt_at: None,
                attempts: Vec::new(),
            });
        }
        let mut push = serde_json::json!({
            "kind": notice.kind.name(),
            "title": truncate_chars(&title, MAX_PUSH_TITLE_CHARS),
            "body": truncate_chars(&body, MAX_PUSH_BODY_CHARS),
            "seq": seq,
        });
        if let Some(link) = link.filter(|l| l.chars().count() <= MAX_PUSH_LINK_CHARS) {
            push["link"] = serde_json::Value::from(link);
        }
        let attempts = send_all(
            &self.shared,
            subscriptions,
            notice.kind.urgency(),
            push.to_string().into_bytes(),
            now,
        )
        .await;
        Ok(Delivery {
            seq,
            held: None,
            first_attempt_at: Some(now),
            attempts,
        })
    }

    /// 번호가 `after`보다 큰 알림을 번호 순서로 `max`개까지(알림 목록). 알림이 아닌 이벤트는 건너뛰고
    /// 건너뛴 만큼 더 읽어 `max`개를 채운다.
    pub fn notices_after(&self, after: u64, max: usize) -> Result<Vec<NoticeRecord>, Error> {
        let mut found = Vec::new();
        let mut cursor = after;
        let batch = max.clamp(1, NOTICE_READ_BATCH);
        while found.len() < max {
            let range = EventRange {
                after_seq: cursor,
                ..EventRange::default()
            };
            let events = self.shared.store.read_events(range, batch)?;
            let Some(last) = events.last() else {
                break;
            };
            cursor = last.seq;
            let read = events.len();
            found.extend(events.into_iter().filter_map(notice_record));
            if read < batch {
                break;
            }
        }
        found.truncate(max);
        Ok(found)
    }
}

/// 알림 목록을 읽을 때 한 번에 읽는 이벤트 수의 상한.
const NOTICE_READ_BATCH: usize = 256;

/// 저장된 이벤트가 알림(`notice.` + 아는 이름)이면 목록의 한 줄로 바꾼다.
fn notice_record(event: TimedEvent) -> Option<NoticeRecord> {
    let kind = NoticeKind::from_name(event.kind.strip_prefix("notice.")?)?;
    let text = |key: &str| event.payload[key].as_str().unwrap_or_default().to_owned();
    Some(NoticeRecord {
        seq: event.seq,
        kind,
        at: event.at,
        title: text("title"),
        body: text("body"),
        link: event.payload["link"].as_str().map(str::to_owned),
    })
}

/// 글자(유니코드 스칼라) 수로 자른다.
fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// base64url(패딩은 있어도 없어도)을 푼다. 실패하면 [`Error::InvalidKey`](글자는 담지 않는다).
fn decode_b64url(text: &str, field: &str) -> Result<Vec<u8>, Error> {
    URL_SAFE_NO_PAD
        .decode(text.trim_end_matches('='))
        .map_err(|_| Error::InvalidKey(format!("{field}가 base64url이 아니다")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 글자_수로_자르고_짧으면_그대로다() {
        assert_eq!(truncate_chars("가나다라", 2), "가나");
        assert_eq!(truncate_chars("가", 5), "가");
    }

    #[test]
    fn 기한은_첫_시도가_없으면_거짓이다() {
        let delivery = Delivery {
            seq: 1,
            held: None,
            first_attempt_at: None,
            attempts: Vec::new(),
        };
        assert!(!delivery.within_deadline(UnixMillis(0)));
    }
}
