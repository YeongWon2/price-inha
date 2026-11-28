//! (48차) 시연이 아닌 실행의 첫 확인과 다시 확인, 그리고 표시가 남은 칸을 "없음"으로 보는 자료 · 뉴스 키 자리.

use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use pi_data::{DataError, NewsError, NewsKeySource, NewsKeys, SourceName};
use pi_notify::NoticeKind;
use pi_secrets::{Credentials, SecretStore, SecretString, Verifier, VerifyFailure};

use super::{
    Mark, Target, UNVERIFIED_HELD, UNVERIFIED_PENDING, UNVERIFIED_REMOVED, held, load, message,
    slot_item, target_of, update,
};
use crate::Settings;
use crate::desk::Desk;
use crate::reach::Linked;

/// 키 하나를 확인하는 시간 상한. 넘으면 확인할 수 없음(확인 대기)으로 본다.
const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
/// 알림 제목.
const NOTICE_TITLE: &str = "시연에서 받아 둔 키";

/// 키 하나의 확인 결과.
enum Outcome {
    /// 바깥이 받아들였다.
    Passed,
    /// 바깥이 거절했다.
    Rejected,
    /// 확인할 수 없었다(장애 · 연결 안 됨 · 시간 초과 · 보관소를 읽지 못함).
    Unreachable,
    /// 보관소에 그 키가 이제 없다.
    Gone,
}

/// 확인에 쓰는 것. 설정 책상 · 알림은 약하게 쥔다(앱이 꺼져 놓이면 다시 확인도 그만둔다).
struct Inner {
    home: PathBuf,
    secrets: Arc<SecretStore>,
    /// 키 등록과 같은 확인기(같은 바깥 주소)를 쥔 설정 책상.
    desk: Weak<Desk>,
    linked: Option<Weak<Linked>>,
    retry: (Duration, Duration),
}

/// 시연이 아닌 실행이 켜질 때의 확인. 시연이면 아무것도 하지 않는다(시연은 바깥에 닿지 않는다).
pub(crate) struct StartCheck;

impl StartCheck {
    /// 시세 흐름 · AI · 자료를 시작하기 전에 부른다: 표시된 확인 대기 칸마다 한 번 확인하고 결과를 표시에 남기고,
    /// 거절은 알림 한 번(`linked`가 있으면). 확인 대기가 남았으면 시세 흐름의 물러서기(`retry_initial`부터 두 배,
    /// `retry_max`에서 멈춤)로 뒤에서 다시 확인한다(설정 책상이 놓이면 그만둔다).
    pub(crate) async fn run(
        settings: &Settings,
        secrets: &Arc<SecretStore>,
        desk: &Arc<Desk>,
        linked: Option<&Arc<Linked>>,
    ) {
        if settings.demo {
            return;
        }
        let inner = Inner {
            home: settings.home.clone(),
            secrets: Arc::clone(secrets),
            desk: Arc::downgrade(desk),
            linked: linked.map(Arc::downgrade),
            retry: (settings.retry_initial, settings.retry_max),
        };
        if run_once(&inner).await {
            tokio::spawn(retry(inner));
        }
    }
}

/// 확인 대기가 없어지거나 설정 책상이 놓일 때까지 물러서며 다시 확인한다.
async fn retry(inner: Inner) {
    let (mut delay, max) = inner.retry;
    loop {
        tokio::time::sleep(delay).await;
        if inner.desk.strong_count() == 0 || !run_once(&inner).await {
            return;
        }
        delay = (delay * 2).min(max).max(Duration::from_millis(1));
    }
}

fn has_pending(inner: &Inner) -> bool {
    load(&inner.home)
        .unwrap_or_default()
        .iter()
        .any(|mark| mark.state == UNVERIFIED_PENDING)
}

/// 한 번 돈다. 확인 대기가 남았는가.
async fn run_once(inner: &Inner) -> bool {
    let marks = match load(&inner.home) {
        Ok(marks) => marks,
        Err(error) => {
            // 표시를 읽지 못하면 모든 키를 쓰지 않는다([`held`]). 다음 기회에 다시 읽는다.
            tracing::warn!(%error, "시연 표시 파일을 읽지 못해 키를 쓰지 않습니다");
            return true;
        }
    };
    for mark in marks.iter().filter(|m| m.state == UNVERIFIED_PENDING) {
        let outcome = match target_of(&mark.item) {
            Some(target) => check_one(inner, target).await,
            None => Outcome::Gone,
        };
        settle_one(inner, &mark.item, target_of(&mark.item), outcome);
    }
    announce(inner);
    has_pending(inner)
}

/// 보관소의 값으로 지금의 키 확인을 돌린다(값은 여기서만 쥐고 기록하지 않는다).
async fn check_one(inner: &Inner, target: Target) -> Outcome {
    let read = match target {
        Target::Kind(kind) => inner.secrets.read(kind),
        Target::Slot(slot) => inner.secrets.read_slot(slot),
        Target::Provider(keys) => inner.secrets.read_provider(keys),
    };
    let values: Credentials = match read {
        Ok(Some(values)) => values,
        Ok(None) => return Outcome::Gone,
        Err(_) => return Outcome::Unreachable,
    };
    let Some(desk) = inner.desk.upgrade() else {
        return Outcome::Unreachable;
    };
    let verifier = desk.key_verifier();
    let verifying = async {
        match target {
            Target::Kind(kind) => verifier.verify(kind, &values).await,
            Target::Slot(slot) => verifier.verify_slot(slot, &values).await,
            Target::Provider(keys) => verifier.verify_provider(keys, &values).await,
        }
    };
    match tokio::time::timeout(CHECK_TIMEOUT, verifying).await {
        Ok(Ok(())) => Outcome::Passed,
        Ok(Err(VerifyFailure::Rejected)) => Outcome::Rejected,
        Ok(Err(_)) | Err(_) => Outcome::Unreachable,
    }
}

/// 결과를 표시에 남긴다: 통과 · 없음 → 지움, 거절 → 지울 수 있으면 지우고(`removed`) 아니면 `held`,
/// 확인할 수 없음 → 그대로(확인 대기).
fn settle_one(inner: &Inner, item: &str, target: Option<Target>, outcome: Outcome) {
    let state = match outcome {
        Outcome::Passed | Outcome::Gone => None,
        Outcome::Unreachable => {
            tracing::info!(
                item,
                "시연에서 받아 둔 키를 확인하지 못해 쓰지 않고 다시 확인합니다"
            );
            return;
        }
        Outcome::Rejected => Some(remove(inner, target)),
    };
    let result = update(&inner.home, |marks| match state {
        None => marks.retain(|mark| mark.item != item),
        Some(state) => {
            for mark in marks.iter_mut().filter(|mark| mark.item == item) {
                mark.state = state;
            }
        }
    });
    match (result, state) {
        (Err(error), _) => tracing::warn!(item, %error, "시연 표시를 고치지 못했습니다"),
        (Ok(()), None) => tracing::info!(item, "시연에서 받아 둔 키가 확인되어 씁니다"),
        (Ok(()), Some(state)) => tracing::warn!(item, state, "시연에서 받아 둔 키가 거절됐습니다"),
    }
}

/// 거절된 키를 지운다. 비밀 보관소에 지우는 길이 있는 것은 공급자 키뿐이다(`remove_provider`).
fn remove(inner: &Inner, target: Option<Target>) -> &'static str {
    let Some(Target::Provider(keys)) = target else {
        return UNVERIFIED_HELD;
    };
    match inner.secrets.remove_provider(keys) {
        Ok(_) => UNVERIFIED_REMOVED,
        Err(error) => {
            tracing::warn!(%error, "거절된 공급자 키를 지우지 못해 쓰지 않고 둡니다");
            UNVERIFIED_HELD
        }
    }
}

/// 아직 알리지 않은 거절을 알림 한 번으로 낸다(알린 것은 표시에 남겨 다시 켜도 또 내지 않는다).
fn announce(inner: &Inner) {
    let Some(linked) = inner.linked.as_ref().and_then(Weak::upgrade) else {
        return;
    };
    let mut fresh: Vec<Mark> = Vec::new();
    let result = update(&inner.home, |marks| {
        for mark in marks
            .iter_mut()
            .filter(|m| m.state != UNVERIFIED_PENDING && !m.announced)
        {
            mark.announced = true;
            fresh.push(mark.clone());
        }
    });
    if let Err(error) = result {
        tracing::warn!(%error, "시연 표시를 고치지 못해 알림을 다음에 냅니다");
        return;
    }
    for mark in fresh {
        let body = message(&mark);
        linked.notice_later(NoticeKind::Anomaly, NOTICE_TITLE.to_string(), body, None);
    }
}

/// 자료 칸(DART · ECOS · FRED)과 뉴스 키를 주는 자리: 표시가 남은 칸은 "없음"으로 본다(바깥에 묻지 않는다).
pub(crate) struct HeldKeys(pub(crate) Arc<SecretStore>);

impl HeldKeys {
    fn held(&self, item: &str) -> bool {
        held(self.0.dir(), item)
    }
}

impl pi_data::KeySource for HeldKeys {
    fn key(&self, source: SourceName) -> Option<SecretString> {
        if self.held(slot_item(source.slot())) {
            return None;
        }
        self.0.key(source)
    }

    fn read_key(&self, source: SourceName) -> Result<Option<SecretString>, DataError> {
        if self.held(slot_item(source.slot())) {
            return Ok(None);
        }
        self.0.read_key(source)
    }
}

impl NewsKeySource for HeldKeys {
    fn read_news_keys(&self, keys: NewsKeys) -> Result<Option<Credentials>, NewsError> {
        if self.held(keys.provider_keys().provider()) {
            return Ok(None);
        }
        self.0.read_news_keys(keys)
    }
}
