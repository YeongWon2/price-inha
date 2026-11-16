//! 보존 규칙(일 7 · 주 4 · 월 12)과 백업 파일 이름. 로컬과 원격에 같게 적용한다.

use std::collections::BTreeSet;

use crate::common::*;
use pi_backup::{
    FILE_SUFFIX, KEEP_DAILY, KEEP_MONTHLY, KEEP_WEEKLY, REMOTE_DIR, backup_file_name,
    parse_backup_file_name, plan_retention,
};
use pi_store::UnixMillis;

/// 칸마다 가장 새것 하나씩, 백업이 있는 최근 `n`칸.
fn newest_per(
    sorted_desc: &[UnixMillis],
    bucket: fn(UnixMillis) -> i64,
    n: usize,
) -> Vec<UnixMillis> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for &t in sorted_desc {
        if seen.len() == n {
            break;
        }
        if seen.insert(bucket(t)) {
            out.push(t);
        }
    }
    out
}

/// 기대값을 따로 센다: 일 · 주 · 달 규칙의 합집합(새것부터).
fn expected_keep(times: &[UnixMillis]) -> Vec<UnixMillis> {
    let mut sorted: Vec<UnixMillis> = times
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    sorted.reverse();
    let mut keep = BTreeSet::new();
    keep.extend(newest_per(&sorted, day_of, KEEP_DAILY));
    keep.extend(newest_per(&sorted, week_of, KEEP_WEEKLY));
    keep.extend(newest_per(&sorted, month_of, KEEP_MONTHLY));
    keep.into_iter().rev().collect()
}

fn others(times: &[UnixMillis], keep: &[UnixMillis]) -> Vec<UnixMillis> {
    let mut rest: Vec<UnixMillis> = times
        .iter()
        .copied()
        .filter(|t| !keep.contains(t))
        .collect();
    rest.sort();
    rest.reverse();
    rest
}

// 검증: D7
#[test]
fn 보존_개수는_일_7_주_4_월_12다() {
    assert_eq!((KEEP_DAILY, KEEP_WEEKLY, KEEP_MONTHLY), (7, 4, 12));
}

// 검증: D7
#[test]
fn 일곱_개보다_적으면_모두_남긴다() {
    let times: Vec<UnixMillis> = (0..5).map(|d| plus_days(kst(2026, 11, 2, 16), d)).collect();

    let plan = plan_retention(&times);

    let mut want = times.clone();
    want.reverse();
    assert_eq!(plan.keep, want);
    assert!(plan.remove.is_empty());
}

// 검증: D7
#[test]
fn 같은_날_여러_개면_그날의_가장_새것만_남긴다() {
    let morning = kst(2026, 11, 16, 1);
    let noon = kst(2026, 11, 16, 12);
    let night = kst(2026, 11, 16, 23);

    let plan = plan_retention(&[noon, night, morning]);

    assert_eq!(plan.keep, vec![night]);
    assert_eq!(plan.remove, vec![noon, morning]);
}

// 검증: D7
#[test]
fn 날의_경계는_한국_시각이다() {
    // 한국 시각 11월 16일 0시 30분과 23시: UTC로는 15일과 16일로 갈리지만 한국에서는 같은 날이다.
    let early = UnixMillis(kst(2026, 11, 16, 0).0 + 30 * 60_000);
    let late = kst(2026, 11, 16, 23);

    let plan = plan_retention(&[early, late]);

    assert_eq!(plan.keep, vec![late]);
    assert_eq!(plan.remove, vec![early]);
}

// 검증: D7
#[test]
fn 오래_쌓이면_일_주_월_규칙의_합집합만_남는다() {
    let start = kst(2025, 9, 1, 16);
    let times: Vec<UnixMillis> = (0..450).map(|d| plus_days(start, d)).collect();

    let plan = plan_retention(&times);

    let want = expected_keep(&times);
    assert_eq!(plan.keep, want);
    assert_eq!(plan.remove, others(&times, &want));
    assert!(plan.keep.len() <= KEEP_DAILY + KEEP_WEEKLY + KEEP_MONTHLY);
    assert!(plan.remove.contains(&start));
}

// 검증: D7
#[test]
fn 띄엄띄엄_있어도_백업이_있는_칸만_센다() {
    // 사흘마다 하나, 그리고 몇 달 비었다가 다시.
    let mut times: Vec<UnixMillis> = (0..30)
        .map(|i| plus_days(kst(2025, 1, 5, 16), i * 3))
        .collect();
    times.extend((0..20).map(|i| plus_days(kst(2025, 10, 1, 16), i * 3)));

    let plan = plan_retention(&times);

    let want = expected_keep(&times);
    assert_eq!(plan.keep, want);
    assert_eq!(plan.remove, others(&times, &want));
}

// 검증: D7
#[test]
fn 입력_순서가_달라도_계획은_같다() {
    let times: Vec<UnixMillis> = (0..60).map(|d| plus_days(kst(2026, 6, 1, 16), d)).collect();
    let mut shuffled = times.clone();
    shuffled.reverse();
    shuffled.swap(3, 40);

    assert_eq!(plan_retention(&shuffled), plan_retention(&times));
}

// 검증: D7
#[test]
fn 파일_이름은_시각으로_되돌아오고_이름_순서가_시각_순서다() {
    let times = [
        UnixMillis(0),
        kst(2026, 1, 1, 0),
        UnixMillis(kst(2026, 11, 16, 16).0 + 123),
        UnixMillis(kst(2026, 11, 16, 16).0 + 124),
        kst(2027, 12, 31, 23),
    ];
    let names: Vec<String> = times.iter().map(|&t| backup_file_name(t)).collect();
    for (name, &t) in names.iter().zip(&times) {
        assert!(name.ends_with(FILE_SUFFIX), "{name}");
        assert!(!name.contains('/'), "{name}");
        assert_eq!(parse_backup_file_name(name), Some(t), "{name}");
    }
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(sorted, names);
}

// 검증: D7
#[test]
fn 다른_모양의_이름은_백업으로_읽지_않는다() {
    let good = backup_file_name(kst(2026, 11, 16, 16));
    let no_suffix = good.trim_end_matches(FILE_SUFFIX).to_owned();
    for name in [
        "README.md",
        "notes.txt",
        "",
        ".age",
        no_suffix.as_str(),
        &format!("x{good}"),
    ] {
        assert_eq!(parse_backup_file_name(name), None, "{name:?}");
    }
}

// 검증: D7, C13, V48
#[tokio::test]
async fn 올린_뒤_로컬과_원격에_같은_보존_규칙을_적용하고_다른_파일은_두다() {
    let env = Env::new().await;
    let now = kst(2026, 11, 16, 16);
    std::fs::create_dir_all(&env.settings.local_dir).unwrap();
    let mut times = vec![now];
    for back in 1..=45 {
        let t = plus_days(now, -back);
        let name = backup_file_name(t);
        env.fake.put_file(&format!("{REMOTE_DIR}/{name}"), b"old");
        std::fs::write(env.settings.local_dir.join(&name), b"old").unwrap();
        times.push(t);
    }
    env.fake
        .put_file(&format!("{REMOTE_DIR}/README.md"), b"keep me");
    std::fs::write(env.settings.local_dir.join("notes.txt"), b"keep me").unwrap();

    let report = env.backup().run(now).await.unwrap();

    let want = expected_keep(&times);
    let mut want_names: Vec<String> = want.iter().map(|&t| backup_file_name(t)).collect();
    want_names.sort();
    let remote: Vec<String> = env
        .fake
        .paths()
        .into_iter()
        .filter_map(|p| p.strip_prefix(&format!("{REMOTE_DIR}/")).map(str::to_owned))
        .filter(|n| parse_backup_file_name(n).is_some())
        .collect();
    let local: Vec<String> = names_in(&env.settings.local_dir)
        .into_iter()
        .filter(|n| parse_backup_file_name(n).is_some())
        .collect();
    assert_eq!(remote, want_names);
    assert_eq!(local, want_names);
    assert!(env.fake.file(&format!("{REMOTE_DIR}/README.md")).is_some());
    assert!(env.settings.local_dir.join("notes.txt").exists());
    let mut removed = report.removed.clone();
    removed.sort();
    removed.reverse();
    assert_eq!(removed, others(&times, &want));
}
