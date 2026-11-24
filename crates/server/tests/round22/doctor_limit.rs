//! 19차 3번: 진단(`doctor`)의 끝(뜻 검색 뒤)에 "하루 AI 호출 상한" 항목. 늘 문제 아님(✓).
//! 상한이 없으면 "하루 AI 호출 상한 없음", 있으면 "하루 AI 호출 상한 N번". `--json`의 항목 이름은 `ai_daily_limit`.

use std::path::{Path, PathBuf};

use price_inha::{
    Check, CheckItem, CommandLine, CommandOutput, CommandRunner, DoctorReport, Os, Service,
    ServiceSetup, Settings, doctor,
};

/// 아무 명령도 부르지 않는 시험용 실행기(서비스는 등록하지 않은 것으로 보인다).
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

fn no_env(_name: &str) -> Option<String> {
    None
}

/// 그 보관 폴더를 주어진 인자(상한 등)로 진단한다.
fn diagnose(home: &Path, extra: &[&str]) -> DoctorReport {
    let mut args: Vec<String> = ["--home", home.to_str().unwrap(), "--port", "0"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    args.extend(extra.iter().map(|s| (*s).to_string()));
    let settings = Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다");
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

fn limit_check(report: &DoctorReport) -> &Check {
    report
        .check(CheckItem::AiDailyLimit)
        .expect("진단에 하루 AI 호출 상한 항목이 있다")
}

fn json_entry(report: &DoctorReport) -> serde_json::Value {
    report.to_json()["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["item"] == "ai_daily_limit")
        .cloned()
        .expect("--json에 ai_daily_limit 항목이 있다")
}

// 검증: D10 D1
#[test]
fn 진단에_하루_ai_호출_상한이_없으면_없음이라고_알리고_문제가_아니다() {
    let home = tempfile::tempdir().unwrap();
    let report = diagnose(home.path(), &[]);
    let check = limit_check(&report);
    assert!(check.ok, "상한 없음은 문제가 아니다: {check:?}");
    assert_eq!(check.fix, None);
    assert_eq!(check.what, "하루 AI 호출 상한 없음");
    assert!(
        report.render().contains("✓ 하루 AI 호출 상한 없음"),
        "{}",
        report.render()
    );
    let entry = json_entry(&report);
    assert_eq!(entry["ok"], true);
    assert_eq!(entry["what"], "하루 AI 호출 상한 없음");
}

// 검증: D10 D1
#[test]
fn 진단에_하루_ai_호출_상한이_있으면_그_번수를_알리고_문제가_아니다() {
    let home = tempfile::tempdir().unwrap();
    let report = diagnose(home.path(), &["--ai-daily-limit", "25"]);
    let check = limit_check(&report);
    assert!(check.ok, "상한이 있어도 문제가 아니다: {check:?}");
    assert_eq!(check.what, "하루 AI 호출 상한 25번");
    let entry = json_entry(&report);
    assert_eq!(entry["ok"], true);
    assert_eq!(entry["what"], "하루 AI 호출 상한 25번");
}

// 검증: D10 D1
#[test]
fn 하루_ai_호출_상한_항목은_뜻_검색_항목_바로_뒤_끝에_온다() {
    let home = tempfile::tempdir().unwrap();
    let report = diagnose(home.path(), &[]);
    let items: Vec<CheckItem> = report.checks.iter().map(|c| c.item).collect();
    assert!(
        items.ends_with(&[CheckItem::Ollama, CheckItem::AiDailyLimit]),
        "뜻 검색 뒤 끝에 온다: {items:?}"
    );
}
