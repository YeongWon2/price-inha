//! 3차 조립의 부품 묶음: 알림 · 사무실 · 백업과 그 자리들(사무실 이벤트 포트 · 잠금 알림 · 이상 알림).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use async_trait::async_trait;
use chrono::{FixedOffset, TimeZone};
use pi_backup::{Backup, BackupReport, BackupSettings, BackupStatus};
use pi_gate::{LockAlert, LockNotice, Publisher};
use pi_market::{Clock, Market};
use pi_notify::{NewNotice, NoticeKind, Notifier, NotifierConfig, VapidKey};
use pi_office::{EventSink, Office, SinkError};
use pi_secrets::SecretStore;
use pi_store::{Store, UnixMillis};
use serde_json::{Value, json};

use super::{BackupWiring, DEFAULT_PUSH_SUBJECT, PUSH_TIMEOUT, Reach, backup_http};
use crate::AppError;
use crate::notify_settings::NotifyBook;

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

/// (33차 3번) 사무실 공지 글의 가리기 자리: 비밀 보관소.
impl pi_office::Redactor for Hidden {
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
    /// (33차 3번) 공지 확인 답 · 이어 말하기의 하루 AI 호출 판정(팀이 붙으면 잇는다).
    pub(crate) budget: Arc<super::SpareBudget>,
    /// (22차 1번) 논의 방의 담당 글을 사무실 팀 대화로 낸다.
    pub(crate) talk: Arc<super::TalkFeed>,
    /// 백업 조립 값과 백업(설정 화면이 바꾼다).
    backup: Mutex<Option<(BackupWiring, Arc<Backup>)>>,
    store: Store,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) secrets: Arc<SecretStore>,
    /// 지켜보는 종목 가운데 늦은 것이 있다고 알렸는가.
    stale: AtomicBool,
    /// (39차) 알림을 허용된 메신저 대화에도 보내는 자리(메신저를 이었을 때만).
    messenger: OnceLock<Weak<crate::messenger::Messenger>>,
    /// (40차) 알림 전달처 설정 · 조용한 시간(보관 폴더의 파일). 전달처는 [`Linked::deliver`]에서 고른다.
    pub(crate) notify: NotifyBook,
    /// (51차) 논의 진행 단계 · 하루 AI 잔량 표시판(팀이 붙으면 팀이 쓴다).
    pub(crate) progress: Arc<super::ProgressBoard>,
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
        // (61차, 결정 31) 보관 파일도 백업에 담는다.
        .map(|backup| backup.with_archive_dir(home.join(crate::ARCHIVE_DIR)))
        // (62차) 비밀이 아닌 설정 파일도 담는다(되살리면 제자리).
        .map(|backup| backup.with_config_files(home.to_path_buf(), crate::BACKUP_CONFIG_FILES))
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
        market: Market,
    ) -> Result<Linked, AppError> {
        let notify_error = |e: pi_notify::Error| AppError::Notify(e.to_string());
        let vapid = VapidKey::load_or_create(secrets.dir()).map_err(notify_error)?;
        let transport: Arc<dyn pi_notify::PushTransport> = match reach.push {
            Some(transport) => transport,
            None => Arc::new(pi_notify::HttpTransport::new(PUSH_TIMEOUT).map_err(notify_error)?),
        };
        // (40차) 저장한 알림 설정을 읽는다(읽지 못하면 기본). 조용한 시간은 그 값으로 notify에 넘긴다.
        let notify = NotifyBook::open(secrets.dir(), market);
        let now = UnixMillis(clock.now().timestamp_millis());
        let notifier = Notifier::new(NotifierConfig {
            store: store.clone(),
            vapid,
            subject: DEFAULT_PUSH_SUBJECT.to_string(),
            transport,
            clock: Arc::new(Clocked(Arc::clone(clock))),
            redactor: Arc::new(Hidden(Arc::clone(secrets))),
            quiet_hours: notify.quiet_at(now),
            attempt_timeout: PUSH_TIMEOUT,
        });
        let sink = Arc::new(LateSink::default());
        let team = store.team_state()?;
        let office_sink: Arc<dyn EventSink> = Arc::new(super::SummonSink::new(
            Arc::clone(&sink) as Arc<dyn EventSink>
        ));
        let budget = Arc::new(super::SpareBudget::default());
        let sink_for_board = Arc::clone(&sink) as Arc<dyn EventSink>;
        // (33차) 공지 저장소 · 가리기와 하루 AI 호출 판정을 끼운다. 부르기 단계는 감싼 포트가 따로 낸다.
        let office = Office::new(team, office_sink, Arc::new(Clocked(Arc::clone(clock))))
            .with_notices(store.clone(), Arc::new(Hidden(Arc::clone(secrets))))
            .with_ai_budget(Arc::clone(&budget) as Arc<dyn pi_office::AiBudget>);
        let backup = backup_http::effective(secrets.dir(), reach.backup)
            .map(|wiring| {
                let backup = backup_of(wiring.clone(), store, secrets)?;
                Ok::<_, AppError>((wiring, Arc::new(backup)))
            })
            .transpose()?;
        let talk = Arc::new(super::TalkFeed::new(
            store.clone(),
            Arc::clone(&sink),
            Arc::clone(secrets),
        ));
        Ok(Linked {
            notifier: Arc::new(notifier),
            office: Arc::new(office),
            sink,
            budget,
            talk,
            backup: Mutex::new(backup),
            store: store.clone(),
            clock: Arc::clone(clock),
            secrets: Arc::clone(secrets),
            stale: AtomicBool::new(false),
            messenger: OnceLock::new(),
            notify,
            progress: Arc::new(super::ProgressBoard::new(sink_for_board)),
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
        let sent = self.deliver(kind, (title, body), link, None).await;
        sent.is_some_and(|(attempts, reached)| attempts == 0 || reached)
    }

    /// (10차 10a 5번) 알림 하나를 보내, 구독이 하나라도 **받았을 때만** 참이다(구독이 없으면 거짓).
    /// 승인 요청의 "알림이 갔다"는 사람에게 닿았다는 뜻이어야 한다(P20: 닿지 않았으면 줄여서도 사지 않는다).
    /// (39차) `short`는 승인 요청의 짧은 번호(메신저 글의 거절 안내에 붙는다).
    pub(crate) async fn notice_reached(
        &self,
        kind: NoticeKind,
        (title, body): (&str, &str),
        link: Option<String>,
        short: Option<String>,
    ) -> bool {
        let sent = self.deliver(kind, (title, body), link, short).await;
        sent.is_some_and(|(_, reached)| reached)
    }

    /// 알림을 보낸다: (보내 본 구독 수, 하나라도 받았는가). 보내지 못했으면 없음.
    /// (39차) 같은 알림을 허용된 메신저 대화에도 뒤에서 보낸다(폰 푸시의 결과와 상관없다).
    /// (40차 4 · 5 · 7번) 전달처는 여기서 고른다: 종류별로 끈 전달처로는 보내지 않고, 조용한 시간에는 급하지 않은
    /// 알림을 메신저로도 보내지 않는다(폰 푸시는 notify가 미룬다). 폰 푸시를 끈 종류도 알림 기록은 남긴다.
    async fn deliver(
        &self,
        kind: NoticeKind,
        (title, body): (&str, &str),
        link: Option<String>,
        short: Option<String>,
    ) -> Option<(usize, bool)> {
        let plan = self.notify.plan(kind, self.now());
        self.notifier.set_quiet_hours(plan.quiet);
        if plan.messenger
            && let Some(messenger) = self.messenger()
        {
            messenger.forward(kind, title, body, short);
        }
        if !plan.push {
            return self.record_only(kind, (title, body), link);
        }
        let notice = NewNotice {
            kind,
            title: title.to_string(),
            body: body.to_string(),
            link,
            occurred_at: self.now(),
        };
        match self.notifier.notify(notice).await {
            Ok(delivery) => {
                let reached = delivery
                    .attempts
                    .iter()
                    .any(|a| matches!(a.outcome, pi_notify::AttemptOutcome::Delivered { .. }));
                Some((delivery.attempts.len(), reached))
            }
            Err(error) => {
                tracing::warn!(%error, "알림을 보내지 못했습니다");
                None
            }
        }
    }

    /// (40차) 폰 푸시를 끈 종류: 보내지 않고 알림 기록만 남긴다(notify가 남기는 것과 같은 모양 · 같은 가리기).
    /// 보내 본 구독이 없으니 "받았다"는 아니다.
    fn record_only(
        &self,
        kind: NoticeKind,
        (title, body): (&str, &str),
        link: Option<String>,
    ) -> Option<(usize, bool)> {
        let hidden = Hidden(Arc::clone(&self.secrets));
        let hide = |text: &str| pi_notify::Redactor::redact(&hidden, text);
        let mut event = json!({ "title": hide(title), "body": hide(body) });
        if let Some(link) = link {
            event["link"] = json!(hide(&link));
        }
        match self.store.append_at(kind.event_kind(), &event, self.now()) {
            Ok(_) => Some((0, false)),
            Err(error) => {
                tracing::warn!(%error, "알림 기록을 남기지 못했습니다");
                None
            }
        }
    }

    /// (40차 4번) 알림 설정을 통째로 바꾼다(파일에 남기고 바로 쓴다 — 조용한 시간은 notify에도 곧바로).
    pub(crate) fn change_notify(
        &self,
        next: crate::notify_settings::NotifySettings,
    ) -> std::io::Result<()> {
        self.notify.replace(next)?;
        self.notifier
            .set_quiet_hours(self.notify.quiet_at(self.now()));
        Ok(())
    }

    /// (39차) 메신저를 잇는다(알림을 허용된 대화에도 보낸다). 한 번만.
    /// (41차 1번) 사무실로 나가는 논의 줄도 그 메신저로 흘려보낸다.
    pub(crate) fn attach_messenger(&self, messenger: &Arc<crate::messenger::Messenger>) {
        let _ = self.messenger.set(Arc::downgrade(messenger));
        let weak = Arc::downgrade(messenger);
        self.talk.listen(Box::new(move |talk| {
            if let Some(messenger) = weak.upgrade() {
                messenger.on_talk(talk);
            }
        }));
    }

    /// (39차) 이은 메신저(없거나 이미 놓였으면 없음).
    pub(crate) fn messenger(&self) -> Option<Arc<crate::messenger::Messenger>> {
        self.messenger.get().and_then(Weak::upgrade)
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

    /// (41차 6번) 전체 정지 알림 자리: 멈춤 알림(급함 — 조용한 시간에도 간다)을 뒤에서 낸다. 전달처는 설정대로.
    pub(crate) fn full_stop_hook(self: &Arc<Self>) -> crate::trading::FullStopHook {
        let linked = Arc::clone(self);
        Box::new(move |body| {
            let title = crate::STOP_NOTICE_TITLE.to_string();
            linked.notice_later(NoticeKind::Stall, title, body, None);
        })
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
        // (40차) 시장 시간대의 시차가 바뀌었을 수 있다(미국 서머타임): 지금 값으로 맞춘 뒤 본다.
        self.notifier
            .set_quiet_hours(self.notify.quiet_at(self.now()));
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
        // (48차) 시연에서 받아 둔 채 아직 확인되지 않은 토큰으로는 올리지 않는다.
        if crate::unverified::held(self.secrets.dir(), crate::KEY_BACKUP_GITHUB_TOKEN) {
            return Err(AppError::BackupNotConfigured);
        }
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
