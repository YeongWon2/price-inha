//! 쓰임 재기: `GET /api/setup/storage` · 설정 화면 진단의 `storage` · `price-inha doctor`의 기록 줄.

use std::path::{Path, PathBuf};

use pi_store::Store;
use price_inha::{
    CheckItem, CommandLine, CommandOutput, CommandRunner, DB_FILE, DIAGNOSIS_ITEMS, DoctorReport,
    Os, SETUP_DOCTOR_PATH, SETUP_STORAGE_PATH, Service, ServiceSetup, doctor, storage_report,
};
use serde_json::Value;

use crate::common::{Bench, DAY, in_session, seed, settings};

struct NoCommands;

impl CommandRunner for NoCommands {
    fn run(&self, _command: &CommandLine) -> std::io::Result<CommandOutput> {
        Ok(CommandOutput {
            success: false,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

fn diagnose(home: &Path) -> DoctorReport {
    let settings = settings(home, &[]);
    let defs = tempfile::tempdir().unwrap();
    let runner = NoCommands;
    let service = Service::new(
        ServiceSetup {
            os: Os::MacOs,
            exe: PathBuf::from("/opt/price-inha/bin/price-inha"),
            home: home.to_path_buf(),
            definition_dir: defs.path().to_path_buf(),
            uid: 501,
            path_env: "/usr/bin:/bin".to_string(),
            run_args: Vec::new(),
        },
        &runner,
    );
    doctor(&settings, &service)
}

fn group<'a>(body: &'a Value, table: &str, kind: Option<&str>) -> &'a Value {
    body["groups"]
        .as_array()
        .expect("groups는 목록이다")
        .iter()
        .find(|g| g["table"] == table && g["kind"].as_str() == kind)
        .unwrap_or_else(|| panic!("{table} {kind:?} 줄이 있어야 한다: {body}"))
}

// 검증: V78 C32 D6
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 쓰임_표는_종류마다_줄_수_바이트_하루_평균과_10년_추정을_낸다() {
    let now = in_session();
    let bench = Bench::start(now, |store| {
        seed(store, now);
    })
    .await;
    let reply = bench.get(SETUP_STORAGE_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let body = &reply.body;
    let progress = group(body, "events", Some("office.progress"));
    assert!(progress["rows"].as_u64().unwrap() >= 23, "{progress}");
    assert!(progress["bytes"].as_u64().unwrap() > 0);
    assert_eq!(progress["archivable"], true);
    let rules = group(body, "events", Some("rules_config"));
    assert_eq!(rules["archivable"], false, "규칙 변경은 감사 기록이다");
    let member = group(body, "room_events", Some("message.member"));
    assert_eq!(member["archivable"], true);
    let user = group(body, "room_events", Some("message.user"));
    assert_eq!(user["archivable"], false);
    group(body, "trades", None);
    let daily = body["daily_bytes"].as_u64().expect("하루 평균");
    assert!(daily > 0);
    assert_eq!(body["ten_year_bytes"].as_u64().unwrap(), daily * 3650);
    assert!(body["file_bytes"].as_u64().unwrap() > 0);
    let line = body["line"].as_str().expect("한 줄");
    assert!(line.starts_with("기록: "), "{line}");
    assert!(
        line.contains("하루 약") && line.contains("10년 약"),
        "{line}"
    );
    let anonymous = bench.get_without_session(SETUP_STORAGE_PATH).await;
    assert_eq!(anonymous.status, 401, "세션이 없으면 열리지 않는다");
    bench.close().await;
}

// 검증: V78 C32 R18-4
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 설정_화면_진단에_기록_용량_한_줄이_저장소_바로_뒤에_온다() {
    assert_eq!(DIAGNOSIS_ITEMS[0], "store");
    assert_eq!(DIAGNOSIS_ITEMS[1], "storage");
    let now = in_session();
    let bench = Bench::start(now, |store| {
        seed(store, now);
    })
    .await;
    let reply = bench.get(SETUP_DOCTOR_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let items = reply.body["items"].as_array().expect("items");
    let storage = items
        .iter()
        .find(|item| item["key"] == "storage")
        .expect("storage 줄");
    assert_eq!(storage["status"], "info");
    assert_eq!(storage["todo"], Value::Null);
    let detail = storage["detail"].as_str().unwrap();
    assert!(
        detail.starts_with("기록: ") && detail.contains("10년 약"),
        "{detail}"
    );
    bench.close().await;
}

// 검증: D1 D6
#[test]
fn 진단은_백업_뒤에_기록_줄을_늘_문제_아님으로_낸다() {
    let home = tempfile::tempdir().unwrap();
    let empty = diagnose(home.path());
    let line = empty.check(CheckItem::Storage).expect("기록 줄");
    assert!(line.ok);
    assert_eq!(line.what, "기록: 아직 없음");
    let items: Vec<CheckItem> = empty.checks.iter().map(|c| c.item).collect();
    let at = items.iter().position(|i| *i == CheckItem::Backup).unwrap();
    assert_eq!(items[at + 1], CheckItem::Storage);
    assert_eq!(empty.to_json()["checks"][at + 1]["item"], "storage");

    let store = Store::open(home.path().join(DB_FILE)).unwrap();
    seed(&store, in_session());
    drop(store);
    let report = diagnose(home.path());
    let line = report.check(CheckItem::Storage).expect("기록 줄");
    assert!(line.ok);
    assert!(line.what.starts_with("기록: "), "{}", line.what);
    assert!(line.what.contains(" MB, 하루 약 "), "{}", line.what);
    assert!(
        line.what.contains(", 10년 약 ") && line.what.ends_with(" GB"),
        "{}",
        line.what
    );
    assert!(report.render().contains(&line.what));
}

// 검증: D6
#[test]
fn 하루_평균은_묶음마다_처음과_마지막_시각_사이의_날로_나눈다() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path().join("pi.sqlite")).unwrap();
    for day in 0..10_i64 {
        let at = pi_store::UnixMillis(1_800_000_000_000 + day * DAY);
        store
            .append_at(
                "office.progress",
                &serde_json::json!({"p": "x".repeat(90)}),
                at,
            )
            .unwrap();
    }
    let usage = store.storage_usage().unwrap();
    let report = storage_report(&usage);
    let progress = report
        .groups
        .iter()
        .find(|g| g.kind.as_deref() == Some("office.progress"))
        .expect("진행 묶음");
    assert_eq!(progress.rows, 10);
    assert_eq!(
        progress.daily_bytes,
        progress.bytes / 10,
        "10일에 걸쳐 쌓였다"
    );
    assert!(report.daily_bytes >= progress.daily_bytes);
    assert_eq!(report.ten_year_bytes, report.daily_bytes * 3650);
    assert_eq!(report.file_bytes, usage.file_bytes());
}
