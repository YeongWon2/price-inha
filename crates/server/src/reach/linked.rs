//! 3차 조립의 부품 묶음: 알림 · 사무실 · 백업과 그 자리들(사무실 이벤트 포트 · 잠금 알림 · 이상 알림).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use async_trait::async_trait;
use chrono::{FixedOffset, TimeZone};
use pi_backup::{Backup, BackupReport, BackupSettings, BackupStatus};
use pi_gate::{LockAlert, LockNotice, Publisher};
use pi_market::Clock;
use pi_notify::{NewNotice, NoticeKind, Notifier, NotifierConfig, VapidKey};
use pi_office::{EventSink, Office, SinkError};
use pi_secrets::SecretStore;
use pi_store::{Store, UnixMillis};
use serde_json::Value;

use super::{BackupWiring, DEFAULT_PUSH_SUBJECT, PUSH_TIMEOUT, Reach, backup_http};
use crate::AppError;

/// 시계 한 개를 여러 모듈의 시계 자리에 끼운다.
pub(crate) struct Clocked(pub(crate) Arc<dyn Clock>);

impl Clocked {
    fn millis(&self) -> UnixMillis {
        UnixMillis(self.0.now().timestamp_millis())
    }
}

impl pi_notify::Clock for Clocked {
    fn now(&self) -> UnixMillis {
        self.millis()
    }
}

impl pi_office::Clock for Clocked {
    fn now(&self) -> UnixMillis {
        self.millis()
    }
}

/// 알림의 가리기 자리: 비밀 보관소.
struct Hidden(Arc<SecretStore>);

impl pi_notify::Redactor for Hidden {
    fn redact(&self, text: &str) -> String {
        self.0.redact(text)
    }
}

/// 사무실의 이벤트 포트: 접속 관문이 뜬 뒤에 붙는 발행 손잡이.
#[derive(Default)]
pub(crate) struct LateSink(OnceLock<Publisher>);

impl LateSink {
    pub(crate) fn attach(&self, publisher: Publisher) {
        let _ = self.0.set(publisher);
    }
}

#[async_trait]
impl EventSink for LateSink {
    async fn publish(&self, kind: &str, payload: Value) -> Result<u64, SinkError> {
        let Some(publisher) = self.0.get() else {
            return Err(SinkError {
                message: "접속 관문이 아직 뜨지 않았다".to_string(),
            });
        };
        publisher
            .publish(kind, payload)
            .await
            .map_err(|e| SinkError {
                message: e.to_string(),
            })
    }
}

/// 3차 조립의 부품.
pub(crate) struct Linked {
    pub(crate) notifier: Arc<Notifier>,
    pub(crate) office: Arc<Office>,
    pub(crate) sink: Arc<LateSink>,
    /// 백업 조립 값과 백업(설정 화면이 바꾼다).
    backup: Mutex<Option<(BackupWiring, Arc<Backup>)>>,
    store: Store,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) secrets: Arc<SecretStore>,
    /// 지켜보는 종목 가운데 늦은 것이 있다고 알렸는가.
    stale: AtomicBool,
}

fn backup_of(
    wiring: BackupWiring,
    store: &Store,
    secrets: &Arc<SecretStore>,
) -> Result<Backup, AppError> {
    let home = secrets.dir();
    let settings = BackupSettings {
        api_base: wiring.api_base,
        repo: wiring.repo,
        recipient: wiring.recipient,
        local_dir: home.join("backups"),
        temp_root: home.join("backup-work"),
    };
    Backup::new(settings, store.clone(), Arc::clone(secrets))
        .map_err(|e| AppError::Backup(e.to_string()))
}

/// 시각을 한국 시각 `HH:MM`으로.
pub(crate) fn clock_text(at: UnixMillis) -> String {
    let Some(kst) = FixedOffset::east_opt(9 * 3600) else {
        return String::new();
    };
    kst.timestamp_millis_opt(at.0)
        .single()
        .map_or_else(String::new, |t| t.format("%H:%M").to_string())
}

impl Linked {
    /// 만든다. VAPID 키는 비밀 보관소 폴더에 둔다(없으면 만든다).
    pub(crate) fn open(
        store: &Store,
        secrets: &Arc<SecretStore>,
        clock: &Arc<dyn Clock>,
        reach: Reach,
    ) -> Result<Linked, AppError> {
        let notify_error = |e: pi_notify::Error| AppError::Notify(e.to_string());
        let vapid = VapidKey::load_or_create(secrets.dir()).map_err(notify_error)?;
        let transport: Arc<dyn pi_notify::PushTransport> = match reach.push {
            Some(transport) => transport,
            None => Arc::new(pi_notify::HttpTransport::new(PUSH_TIMEOUT).map_err(notify_error)?),
        };
        let notifier = Notifier::new(NotifierConfig {
            store: store.clone(),
            vapid,
            subject: DEFAULT_PUSH_SUBJECT.to_string(),
            transport,
            clock: Arc::new(Clocked(Arc::clone(clock))),
            redactor: Arc::new(Hidden(Arc::clone(secrets))),
            quiet_hours: None,
            attempt_timeout: PUSH_TIMEOUT,
        });
        let sink = Arc::new(LateSink::default());
        let team = store.team_state()?;
        let office_sink: Arc<dyn EventSink> = Arc::clone(&sink) as Arc<dyn EventSink>;
        let office = Office::new(team, office_sink, Arc::new(Clocked(Arc::clone(clock))));
        let backup = backup_http::effective(secrets.dir(), reach.backup)
            .map(|wiring| {
                let backup = backup_of(wiring.clone(), store, secrets)?;
                Ok::<_, AppError>((wiring, Arc::new(backup)))
            })
            .transpose()?;
        Ok(Linked {
            notifier: Arc::new(notifier),
            office: Arc::new(office),
            sink,
            backup: Mutex::new(backup),
            store: store.clone(),
            clock: Arc::clone(clock),
            secrets: Arc::clone(secrets),
            stale: AtomicBool::new(false),
        })
    }

    pub(crate) fn now(&self) -> UnixMillis {
        UnixMillis(self.clock.now().timestamp_millis())
    }

    /// 알림 하나를 보낸다. 보냈으면(구독이 하나라도 받았거나 구독이 없으면) 참.
    pub(crate) async fn notice(
        &self,
        kind: NoticeKind,
        title: &str,
        body: &str,
        link: Option<String>,
    ) -> bool {
        let notice = NewNotice {
            kind,
            title: title.to_string(),
            body: body.to_string(),
            link,
            occurred_at: self.now(),
        };
        match self.notifier.notify(notice).await {
            Ok(delivery) => {
                delivery.attempts.is_empty()
                    || delivery
                        .attempts
                        .iter()
                        .any(|a| matches!(a.outcome, pi_notify::AttemptOutcome::Delivered { .. }))
            }
            Err(error) => {
                tracing::warn!(%error, "알림을 보내지 못했습니다");
                false
            }
        }
    }

    /// 알림을 뒤에서 보낸다(요청 처리 · 잠금을 붙잡지 않는다).
    pub(crate) fn notice_later(
        self: &Arc<Self>,
        kind: NoticeKind,
        title: String,
        body: String,
        link: Option<String>,
    ) {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            me.notice(kind, &title, &body, link).await;
        });
    }

    /// 늦은 종목 알림을 본다: 없음 → 있음으로 바뀔 때만 이상 알림을 낸다(N5 · V11).
    pub(crate) fn on_stale(self: &Arc<Self>, notice: &Value) {
        let symbols: Vec<&str> = notice["symbols"]
            .as_array()
            .map(|list| list.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if symbols.is_empty() {
            self.stale.store(false, Ordering::SeqCst);
            return;
        }
        if self.stale.swap(true, Ordering::SeqCst) {
            return;
        }
        let body = format!("시세가 30초 넘게 오지 않습니다: {}", symbols.join(", "));
        self.notice_later(NoticeKind::Anomaly, "시세 이상".to_string(), body, None);
    }

    /// 조용한 시간이 아니면 미뤄 둔 알림을 보낸다(구독이 생겼을 때 · 조용한 시간이 끝났을 때).
    pub(crate) async fn release_held(&self) {
        match self.notifier.release_held().await {
            Ok(sent) if !sent.is_empty() => {
                tracing::info!(count = sent.len(), "미뤄 둔 알림을 보냈습니다")
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "미뤄 둔 알림을 보내지 못했습니다"),
        }
    }

    /// 지금의 백업(설정했으면).
    pub(crate) fn backup(&self) -> Option<Arc<Backup>> {
        let backup = self.backup.lock().unwrap_or_else(PoisonError::into_inner);
        backup.as_ref().map(|(_, backup)| Arc::clone(backup))
    }

    /// 지금의 백업 조립 값(설정했으면).
    pub(crate) fn backup_wiring(&self) -> Option<BackupWiring> {
        let backup = self.backup.lock().unwrap_or_else(PoisonError::into_inner);
        backup.as_ref().map(|(wiring, _)| wiring.clone())
    }

    /// 설정 화면이 저장소 · 공개키를 바꾼다: 파일에 남기고 다음 백업부터 쓴다. 값은 부르는 쪽이 검사했다.
    pub(crate) fn set_backup(&self, repo: &str, recipient: &str) -> Result<BackupWiring, AppError> {
        let api_base = self
            .backup_wiring()
            .map_or_else(|| backup_http::GITHUB_API_BASE.to_string(), |w| w.api_base);
        let wiring = BackupWiring {
            api_base,
            repo: repo.to_string(),
            recipient: recipient.to_string(),
        };
        let backup = backup_of(wiring.clone(), &self.store, &self.secrets)?;
        backup_http::save(self.secrets.dir(), repo, recipient)?;
        let mut held = self.backup.lock().unwrap_or_else(PoisonError::into_inner);
        *held = Some((wiring.clone(), Arc::new(backup)));
        Ok(wiring)
    }

    /// 지금 한 번 백업한다. 실패하면 백업 실패 알림을 낸다(E15).
    pub(crate) async fn back_up(&self) -> Result<BackupReport, AppError> {
        let backup = self.backup().ok_or(AppError::BackupNotConfigured)?;
        match backup.run(self.now()).await {
            Ok(report) => Ok(report),
            Err(failure) => {
                let text = self.secrets.redact(&failure.error.to_string());
                let body = format!("백업을 올리지 못했습니다. 다음 마감에 다시 합니다({text})");
                self.notice(NoticeKind::BackupFailure, "백업 실패", &body, None)
                    .await;
                Err(AppError::Backup(text))
            }
        }
    }

    /// 백업 상태.
    pub(crate) fn backup_status(&self) -> Result<BackupStatus, AppError> {
        let backup = self.backup().ok_or(AppError::BackupNotConfigured)?;
        backup.status().map_err(|e| AppError::Backup(e.to_string()))
    }
}

/// 접속 잠금 알림(E2 · V39).
pub(crate) struct LockNotices(pub(crate) Arc<Linked>);

impl LockAlert for LockNotices {
    fn locked(&self, notice: LockNotice) {
        let body = format!(
            "{} 기기가 접속 키를 {}번 틀려 {}까지 막았습니다",
            notice.device,
            notice.failures,
            clock_text(notice.until)
        );
        self.0
            .notice_later(NoticeKind::AccessLock, "접속 잠금".to_string(), body, None);
    }
}
