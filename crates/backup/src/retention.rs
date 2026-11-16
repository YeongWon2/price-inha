//! 보존 규칙과 백업 파일 이름 (원본 동작: OpenClaw `--keep-daily 7 --keep-weekly 4 --keep-monthly 12`).

use std::collections::BTreeSet;

use pi_store::UnixMillis;

/// 보존 계획. `keep`과 `remove`는 겹치지 않고 합치면 입력 전부다(중복 입력은 하나로 본다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionPlan {
    /// 남길 것(새것부터).
    pub keep: Vec<UnixMillis>,
    /// 지울 것(새것부터).
    pub remove: Vec<UnixMillis>,
}

/// 백업 시각들에 보존 규칙을 적용한다.
///
/// 새것부터 보며, 백업이 있는 최근 [`crate::KEEP_DAILY`]일 · [`crate::KEEP_WEEKLY`]주(ISO, 월요일 시작) ·
/// [`crate::KEEP_MONTHLY`]달마다 그 칸의 가장 새것 하나를 남긴다(세 규칙의 합집합). 날 · 주 · 달의
/// 경계는 한국 시각(UTC+9)으로 센다. 나머지는 지운다.
pub fn plan_retention(times: &[UnixMillis]) -> RetentionPlan {
    let unique: BTreeSet<UnixMillis> = times.iter().copied().collect();
    let newest_first: Vec<UnixMillis> = unique.into_iter().rev().collect();
    let mut keep = BTreeSet::new();
    keep.extend(newest_per(&newest_first, kst_day, crate::KEEP_DAILY));
    keep.extend(newest_per(&newest_first, kst_week, crate::KEEP_WEEKLY));
    keep.extend(newest_per(&newest_first, kst_month, crate::KEEP_MONTHLY));
    let remove = newest_first
        .iter()
        .copied()
        .filter(|t| !keep.contains(t))
        .collect();
    RetentionPlan {
        keep: keep.into_iter().rev().collect(),
        remove,
    }
}

/// 새것부터 보며 칸마다 첫 것(가장 새것)을, 칸이 `n`개 찰 때까지 고른다.
fn newest_per(
    newest_first: &[UnixMillis],
    bucket: fn(UnixMillis) -> i64,
    n: usize,
) -> Vec<UnixMillis> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for &t in newest_first {
        if seen.len() == n {
            break;
        }
        if seen.insert(bucket(t)) {
            out.push(t);
        }
    }
    out
}

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;
const KST_OFFSET_MS: i64 = 9 * HOUR_MS;

/// 한국 시각 날 번호(1970-01-01부터).
fn kst_day(t: UnixMillis) -> i64 {
    (t.0 + KST_OFFSET_MS).div_euclid(DAY_MS)
}

/// 한국 시각 ISO 주 번호(월요일 시작). 1970-01-01은 목요일이다.
fn kst_week(t: UnixMillis) -> i64 {
    (kst_day(t) + 3).div_euclid(7)
}

/// 한국 시각 달 번호(년 × 12 + 월).
fn kst_month(t: UnixMillis) -> i64 {
    let (y, m, _) = civil(kst_day(t));
    y * 12 + m
}

/// 날 번호(1970-01-01 = 0)를 그레고리력 (년, 월, 일)로.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

const NAME_PREFIX: &str = "price-inha-backup-";

/// 그 시각의 백업 파일 이름. `price-inha-backup-YYYYMMDDTHHMMSSmmmZ.age`(UTC)이고 이름 순서가 시각 순서다.
pub fn backup_file_name(at: UnixMillis) -> String {
    let days = at.0.div_euclid(DAY_MS);
    let ms = at.0.rem_euclid(DAY_MS);
    let (y, m, d) = civil(days);
    let (hh, rest) = (ms / HOUR_MS, ms % HOUR_MS);
    let (mm, rest) = (rest / 60_000, rest % 60_000);
    let (ss, milli) = (rest / 1000, rest % 1000);
    format!(
        "{NAME_PREFIX}{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}{milli:03}Z{}",
        crate::FILE_SUFFIX
    )
}

/// [`backup_file_name`]이 만든 이름에서 시각을 읽는다. 그 모양이 아니면 `None`.
pub fn parse_backup_file_name(name: &str) -> Option<UnixMillis> {
    let core = name
        .strip_prefix(NAME_PREFIX)?
        .strip_suffix(crate::FILE_SUFFIX)?
        .strip_suffix('Z')?;
    let (date, time) = core.split_once('T')?;
    if date.len() != 8 || time.len() != 9 {
        return None;
    }
    if !date.bytes().chain(time.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let num = |s: &str| s.parse::<i64>().ok();
    let (y, m, d) = (num(&date[..4])?, num(&date[4..6])?, num(&date[6..])?);
    let (hh, mm) = (num(&time[..2])?, num(&time[2..4])?);
    let (ss, milli) = (num(&time[4..6])?, num(&time[6..])?);
    let ms = days_from_civil(y, m, d) * DAY_MS + hh * HOUR_MS + mm * 60_000 + ss * 1000 + milli;
    let at = UnixMillis(ms);
    // 범위를 벗어난 값(13월 · 25시 등)은 되돌려 만든 이름이 달라진다.
    (backup_file_name(at) == name).then_some(at)
}

/// 그레고리력 (년, 월, 일)을 날 번호(1970-01-01 = 0)로.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::{backup_file_name, civil, days_from_civil, parse_backup_file_name};
    use pi_store::UnixMillis;

    #[test]
    fn 날짜_계산은_되돌아온다() {
        for days in [-800_000, -1, 0, 1, 19_000, 20_000, 800_000] {
            let (y, m, d) = civil(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }

    #[test]
    fn 범위를_벗어난_이름은_읽지_않는다() {
        assert_eq!(
            backup_file_name(UnixMillis(0)),
            "price-inha-backup-19700101T000000000Z.age"
        );
        for bad in [
            "price-inha-backup-19701301T000000000Z.age",
            "price-inha-backup-19700101T250000000Z.age",
            "price-inha-backup-1970010T0000000000Z.age",
        ] {
            assert_eq!(parse_backup_file_name(bad), None, "{bad}");
        }
    }
}
