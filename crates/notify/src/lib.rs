//! 알림 · 웹 푸시(A3 · A1, T22).
//!
//! 알림은 번호 있는 이벤트로 쌓고(저장소), 구독마다 웹 푸시로 보낸다.
//! 보내기 전에 가린다(E38). 암호화는 순수 Rust(RFC 8291 aes128gcm), 서명은 VAPID ES256(RFC 8292).
//! 폰 화면(`web/phone/`)은 3D를 불러오지 않는 따로 묶음이다.

pub mod error;

mod clock;
mod dispatch;
mod guard;
mod kind;
mod notifier;
mod phone;
mod quiet;
mod release;
mod transport;
mod vapid;
mod webpush;

pub use clock::{Clock, NoRedaction, Redactor};
pub use error::Error;
pub use guard::{PinnedTarget, Resolver, SystemResolver, is_internal_ip, pin_endpoint};
pub use kind::{NoticeKind, Urgency};
pub use notifier::{
    Attempt, AttemptOutcome, Delivery, FIRST_ATTEMPT_WITHIN_MS, HoldReason, MAX_PUSH_BODY_CHARS,
    MAX_PUSH_LINK_CHARS, MAX_PUSH_TITLE_CHARS, NewNotice, NewSubscription, NoticeRecord, Notifier,
    NotifierConfig,
};
pub use phone::{Asset, phone_asset, phone_assets};
pub use quiet::QuietHours;
pub use release::HELD_RELEASE_WINDOW_MS;
pub use transport::{HttpTransport, PushRequest, PushResponse, PushTransport};
pub use vapid::{VAPID_KEY_FILE, VapidKey};
pub use webpush::{MAX_PLAINTEXT, encrypt_aes128gcm, encrypt_aes128gcm_with};
