//! (61차) 덜어 내기 한 번: 보관 기간이 지난 주마다 내보내기 → 보관 파일(확인) → 요약 한 행 + 원문 지우기,
//! 오래된 바깥 자료 조각 → 요약 기록 하나, 옮긴 것이 있으면 공간 돌려받기.

use chrono::{DateTime, Datelike, Days, FixedOffset, TimeZone, Utc};
use pi_knowledge::{Knowledge, RecordText};
use pi_store::{ArchiveCategory, KnowledgeKind, KnowledgeSource, NewArchiveBatch, UnixMillis};
use serde_json::json;

use super::file::{ensure_dir, free_name, write_verified};
use super::{ARCHIVE_DIR, ArchiveJob, ArchiveRun, ArchivedPeriod, summary};
use crate::error::ArchiveError;

/// 하루(밀리초).
const DAY_MS: i64 = 86_400_000;
/// 한 번에 옮기는 가장 많은 주(묶음마다). 남은 것은 다음 날 이어서 한다.
const WEEKS_PER_RUN: usize = 26;
/// 한 번에 고르는 바깥 자료 기록의 가장 많은 수(한 주 안에서).
const KNOWLEDGE_BATCH: usize = 500;
/// 공간 돌려받기 한 번에 돌려주는 가장 많은 쪽 수.
const RECLAIM_PAGES: u32 = 8192;

/// 그 시각이 든 주(한국 시각 월요일 0시부터 7일): (시작, 끝, 이름 `YYYY-Www`).
fn week_of(at: i64) -> Option<(i64, i64, String)> {
    let kst = FixedOffset::east_opt(9 * 3600)?;
    let date = kst.timestamp_millis_opt(at).single()?.date_naive();
    let monday =
        date.checked_sub_days(Days::new(u64::from(date.weekday().num_days_from_monday())))?;
    let from = kst
        .from_local_datetime(&monday.and_hms_opt(0, 0, 0)?)
        .single()?
        .timestamp_millis();
    let iso = monday.iso_week();
    Some((
        from,
        from + 7 * DAY_MS,
        format!("{}-W{:02}", iso.year(), iso.week()),
    ))
}

/// 보관 기간이 지난 기록을 덜어 낸다. `now`는 지금 시각. 한 기간이 실패하면 거기서 멈추고 오류를 준다
/// (그 기간의 원문은 DB에 그대로 남고, 앞서 옮긴 기간은 그대로다).
pub fn run_archive(job: &ArchiveJob, now: DateTime<Utc>) -> Result<ArchiveRun, ArchiveError> {
    let now_ms = now.timestamp_millis();
    let cutoff = now_ms.saturating_sub(i64::from(job.keep_days).saturating_mul(DAY_MS));
    let dir = job.home.join(ARCHIVE_DIR);
    let mut periods = Vec::new();
    for category in ArchiveCategory::ALL {
        archive_category(job, category, (cutoff, now_ms), &mut periods)?;
    }
    let knowledge_records = match &job.knowledge {
        Some(knowledge) => archive_knowledge(knowledge, &dir, (cutoff, now_ms))?,
        None => 0,
    };
    let reclaimed = if periods.is_empty() && knowledge_records == 0 {
        None
    } else {
        Some(job.store.reclaim_space(RECLAIM_PAGES)?)
    };
    Ok(ArchiveRun {
        periods,
        knowledge_records,
        reclaimed,
    })
}

fn archive_category(
    job: &ArchiveJob,
    category: ArchiveCategory,
    (cutoff, now_ms): (i64, i64),
    periods: &mut Vec<ArchivedPeriod>,
) -> Result<(), ArchiveError> {
    let dir = job.home.join(ARCHIVE_DIR);
    let taken: Vec<String> = job
        .store
        .archive_batches()?
        .into_iter()
        .filter(|b| b.batch.category == category)
        .map(|b| b.batch.period)
        .collect();
    for _ in 0..WEEKS_PER_RUN {
        let Some(oldest) = job.store.archive_oldest_at(category)? else {
            return Ok(());
        };
        let Some((from, to, week)) = week_of(oldest.0) else {
            return Ok(());
        };
        if to > cutoff {
            return Ok(());
        }
        let export = job
            .store
            .archive_export(category, UnixMillis(from), UnixMillis(to))?;
        if export.lines.is_empty() {
            return Ok(());
        }
        ensure_dir(&dir)?;
        let stem = format!("{}-{week}", category.code());
        let taken_files: Vec<String> = taken
            .iter()
            .map(|p| format!("{}-{p}.jsonl", category.code()))
            .collect();
        let name = free_name(&dir, &stem, &taken_files);
        let period = name
            .trim_start_matches(&format!("{}-", category.code()))
            .trim_end_matches(".jsonl")
            .to_owned();
        let (file, sha) = write_verified(&dir, &name, &export.lines)?;
        let batch = NewArchiveBatch {
            category,
            period: period.clone(),
            from_at: export.from_at,
            to_at: export.to_at,
            upto_rowid: export.upto_rowid,
            row_count: export.lines.len() as u64,
            fingerprint: export.fingerprint.clone(),
            file: name.clone(),
            file_sha256: sha,
            summary: summary::build(category, &period, &export.lines, &name),
            created_at: UnixMillis(now_ms),
        };
        job.store.archive_commit(batch)?;
        periods.push(ArchivedPeriod {
            category,
            period,
            rows: export.lines.len() as u64,
            file,
        });
    }
    Ok(())
}

/// 오래된 바깥 자료 기록을 주마다 보관 파일 + 요약 기록 하나로 바꾼다. 바꾼 기록 수를 준다.
fn archive_knowledge(
    knowledge: &Knowledge,
    dir: &std::path::Path,
    (cutoff, now_ms): (i64, i64),
) -> Result<usize, ArchiveError> {
    let mut moved = 0;
    for _ in 0..WEEKS_PER_RUN {
        let old = knowledge.old_external_records(UnixMillis(cutoff), KNOWLEDGE_BATCH)?;
        let Some(first) = old.first() else {
            return Ok(moved);
        };
        let Some((_, to, week)) = week_of(first.at.0) else {
            return Ok(moved);
        };
        let group: Vec<_> = old.iter().filter(|r| r.at.0 < to).collect();
        ensure_dir(dir)?;
        let name = free_name(dir, &format!("knowledge-{week}"), &[]);
        let period = name
            .trim_start_matches("knowledge-")
            .trim_end_matches(".jsonl")
            .to_owned();
        let lines: Vec<String> = group
            .iter()
            .map(|r| json!({"record_ref": r.record_ref, "text": r.text, "at": r.at.0}).to_string())
            .collect();
        write_verified(dir, &name, &lines)?;
        let titles: Vec<String> = group
            .iter()
            .map(|r| r.text.lines().next().unwrap_or_default().to_owned())
            .collect();
        let refs: Vec<String> = group.iter().map(|r| r.record_ref.clone()).collect();
        knowledge.replace_with_summary(
            &refs,
            RecordText {
                kind: KnowledgeKind::MarketNote,
                source: KnowledgeSource::External,
                record_ref: format!("{}knowledge:{period}", pi_knowledge::ARCHIVE_REF_PREFIX),
                text: summary::knowledge_text(&period, &titles, &name),
                at: UnixMillis(to.min(now_ms) - 1),
            },
        )?;
        moved += refs.len();
    }
    Ok(moved)
}
