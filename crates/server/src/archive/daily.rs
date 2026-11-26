//! (61차) 하루 한 번 덜어 내기: 한국 시각 16시(장 마감 · 하루 정리 뒤) 이후에 그날 아직 돌지 않았으면 돈다.
//! 켜졌을 때 이미 그 시각이 지났으면 바로 한 번 돈다. 꺼질 때(멈춤 알림) 그만둔다.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, NaiveDate, Timelike, Utc};
use pi_market::Clock;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::{ArchiveJob, run_archive};

/// 이 시각(한국, 시) 뒤에 돈다.
const AFTER_HOUR_KST: u32 = 16;
/// 때가 됐는지 다시 보는 간격.
const POLL: Duration = Duration::from_secs(600);

/// 지금 돌 때인가: 한국 시각으로 16시 뒤이고 그날 아직 돌지 않았다. 돌 때면 그날을 준다.
fn due(now: DateTime<Utc>, last: Option<NaiveDate>) -> Option<NaiveDate> {
    let kst = now.with_timezone(&FixedOffset::east_opt(9 * 3600)?);
    let today = kst.date_naive();
    (kst.hour() >= AFTER_HOUR_KST && last != Some(today)).then_some(today)
}

/// 뒤에서 하루 한 번 덜어 내기를 돈다.
pub(crate) fn spawn_daily(
    job: ArchiveJob,
    clock: Arc<dyn Clock>,
    mut stop: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut last: Option<NaiveDate> = None;
        loop {
            if *stop.borrow() {
                return;
            }
            let now = clock.now();
            if let Some(today) = due(now, last) {
                last = Some(today);
                let work = job.clone();
                match tokio::task::spawn_blocking(move || run_archive(&work, now)).await {
                    Ok(Ok(run)) => tracing::info!(
                        periods = run.periods.len(),
                        knowledge = run.knowledge_records,
                        "오래된 기록을 보관 파일로 옮겼다"
                    ),
                    Ok(Err(error)) => tracing::warn!(%error, "오래된 기록을 옮기지 못했다"),
                    Err(error) => tracing::warn!(%error, "오래된 기록 옮기기가 멈췄다"),
                }
            }
            tokio::select! {
                changed = stop.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
                () = clock.sleep(POLL) => {}
            }
        }
    })
}
