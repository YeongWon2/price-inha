//! pi-ai 7차: CLI 공급자. docs/dev/modules/ai.md "7차 작업" 1 · 2번.
//!
//! 실제 `claude`를 부르지 않는다. 테스트가 임시 폴더에 가짜 실행 파일(셸 스크립트)을 만들어
//! 경로로 준다. 사용자 `~/.claude`를 열지 않는다(자식 환경은 `PATH`만 준다).
//! 가짜는 호출마다 받은 인자(한 줄에 하나) · 환경 · 표준 입력 · 시스템 글 파일 내용을
//! 자기 프로세스 번호가 붙은 파일에 적고, 정해 둔 결과 한 줄을 내고 바로 끝난다.
//!
//! 7차 3 · 5번(호출 쪽 `terminate`의 `set_reaped` 순서, TERM 뒤 KILL 명령만 실패할 때 Drop이 다시 보냄)은
//! 공개 API로 그 틈을 일으킬 수 없어 `src` 안 단위 테스트 몫이다. 4 · 6번은 문서 주석이다.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use pi_ai::{
    AiProvider, ClaudeCliConfig, ClaudeCliProvider, CompletionRequest, Message, NoRedaction,
    Redactor, Role,
};
use serde_json::json;
use tempfile::TempDir;

const TEST_MODEL: &str = "siheom-model-g";
/// 글에 섞여 들어가는 시험용 비밀 글자(진짜 키처럼 보이지 않는다).
const TEST_SECRET: &str = "SIHEOM-GARIM-0717";
/// 시험용 가리기가 바꿔 넣는 글자.
const MASK: &str = "[가림]";
const MAX_TOKENS_ENV: &str = "CLAUDE_CODE_MAX_OUTPUT_TOKENS";

// ---------- 가짜 CLI ----------

struct Fake {
    _root: TempDir,
    log: PathBuf,
    bin: PathBuf,
    work: PathBuf,
}

impl Fake {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let log = root.join("log");
        let work = root.join("work");
        for d in [&log, &work] {
            fs::create_dir(d).unwrap();
        }
        let bin = root.join("claude");
        fs::write(&bin, script(&log)).unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _root: dir,
            log,
            bin,
            work,
        }
    }

    /// `kind-<번호>` 파일들의 (번호, 내용). 번호 순.
    fn records(&self, kind: &str) -> Vec<(String, String)> {
        let prefix = format!("{kind}-");
        let mut out: Vec<(String, String)> = fs::read_dir(&self.log)
            .unwrap()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let id = name.strip_prefix(&prefix)?.to_string();
                Some((id, fs::read_to_string(e.path()).unwrap()))
            })
            .collect();
        out.sort();
        out
    }

    /// 한 번 호출만 했을 때 그 호출이 적은 `kind` 내용.
    fn only(&self, kind: &str) -> String {
        let records = self.records(kind);
        assert_eq!(records.len(), 1, "{kind} 기록 수: {records:?}");
        records.into_iter().next().unwrap().1
    }

    /// 번호 하나의 `kind` 내용.
    fn of(&self, kind: &str, id: &str) -> String {
        fs::read_to_string(self.log.join(format!("{kind}-{id}"))).unwrap()
    }
}

fn result_line() -> String {
    json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": "가짜 답",
        "stop_reason": "end_turn",
        "num_turns": 1,
        "permission_denials": [],
        "usage": {"input_tokens": 3, "output_tokens": 4},
    })
    .to_string()
}

/// 가짜 실행 파일의 셸 글. 부모 환경이 없어도 돌도록 명령은 절대 경로로 부른다.
fn script(log: &Path) -> String {
    let log = log.display();
    let result = result_line();
    format!(
        r#"#!/bin/sh
L='{log}'
: > "$L/args-$$"
for a in "$@"; do printf '%s\n' "$a" >> "$L/args-$$"; done
sys=''
prev=''
for a in "$@"; do
  if [ "$prev" = "--system-prompt-file" ]; then sys="$a"; fi
  prev="$a"
done
if [ -n "$sys" ]; then /bin/cat "$sys" > "$L/system-$$"; fi
/usr/bin/env > "$L/env-$$"
/bin/cat > "$L/stdin-$$"
printf '%s\n' '{result}'
exit 0
"#
    )
}

fn config(f: &Fake) -> ClaudeCliConfig {
    ClaudeCliConfig {
        command: Some(f.bin.clone()),
        config_dir: None,
        work_dir: f.work.clone(),
        base_env: vec![("PATH".to_string(), "/usr/bin:/bin".to_string())],
        timeout: Duration::from_secs(20),
        max_concurrency: 2,
        max_output_bytes: 1_000_000,
        kill_grace: Duration::from_millis(300),
        version_timeout: Duration::from_millis(1500),
        auth_timeout: Duration::from_millis(1500),
    }
}

/// 시험용 가리기: 비밀 글자를 [`MASK`]로 바꾸고, 불린 횟수를 센다.
#[derive(Default)]
struct MaskSecrets {
    calls: AtomicUsize,
}

impl Redactor for MaskSecrets {
    fn redact(&self, text: &str) -> String {
        self.calls.fetch_add(1, Ordering::SeqCst);
        text.replace(TEST_SECRET, MASK)
    }
}

fn masked(f: &Fake, redactor: Arc<MaskSecrets>) -> ClaudeCliProvider {
    ClaudeCliProvider::new(config(f)).with_redactor(redactor)
}

fn msg(role: Role, content: &str) -> Message {
    Message {
        role,
        content: content.to_string(),
    }
}

fn request(system: Option<String>, messages: Vec<Message>, max_tokens: u32) -> CompletionRequest {
    CompletionRequest {
        model: TEST_MODEL.to_string(),
        system,
        messages,
        max_tokens,
    }
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

/// 인자 목록에서 `name` 바로 뒤의 값.
fn arg_after<'a>(args: &'a str, name: &str) -> Option<&'a str> {
    let all = lines(args);
    all.iter()
        .position(|a| *a == name)
        .and_then(|i| all.get(i + 1).copied())
}

/// 환경 기록에서 `name`의 값.
fn env_value<'a>(env: &'a str, name: &str) -> Option<&'a str> {
    env.lines()
        .find_map(|l| l.strip_prefix(name).and_then(|rest| rest.strip_prefix('=')))
}

/// 작업 폴더에 남은 파일 이름들.
fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

// ---------- 1. 나가는 글 가리기 자리: 표준 입력 · 시스템 글 파일 ----------

// 검증: E38 V47
#[tokio::test]
async fn 가리기를_끼우면_표준_입력의_비밀이_가려진다() {
    let f = Fake::new();
    let redactor = Arc::new(MaskSecrets::default());
    let provider = masked(&f, redactor.clone());

    let response = provider
        .complete(request(
            None,
            vec![msg(Role::User, &format!("이 값을 봐: {TEST_SECRET}"))],
            100,
        ))
        .await
        .unwrap();
    assert_eq!(response.text, "가짜 답");

    let stdin = f.only("stdin");
    assert!(
        !stdin.contains(TEST_SECRET),
        "표준 입력에 비밀 글자: {stdin}"
    );
    assert_eq!(stdin.trim_end(), format!("이 값을 봐: {MASK}"));
    assert!(
        redactor.calls.load(Ordering::SeqCst) >= 1,
        "가리기를 부르지 않았다"
    );
}

// 검증: E38 V47
#[tokio::test]
async fn 여러_줄_대화를_합친_표준_입력도_모두_가려진다() {
    let f = Fake::new();
    let provider = masked(&f, Arc::new(MaskSecrets::default()));

    provider
        .complete(request(
            None,
            vec![
                msg(Role::User, &format!("첫 질문 {TEST_SECRET}")),
                msg(Role::Assistant, &format!("담당 답 {TEST_SECRET}")),
                msg(Role::User, &format!("다음 질문 {TEST_SECRET}")),
            ],
            100,
        ))
        .await
        .unwrap();

    let stdin = f.only("stdin");
    assert!(
        !stdin.contains(TEST_SECRET),
        "표준 입력에 비밀 글자: {stdin}"
    );
    assert!(stdin.contains(&format!("첫 질문 {MASK}")), "{stdin}");
    assert!(stdin.contains(&format!("담당 답 {MASK}")), "{stdin}");
    assert!(stdin.contains(&format!("다음 질문 {MASK}")), "{stdin}");
}

// 검증: E38 V47
#[tokio::test]
async fn 시스템_글_파일의_비밀도_가려지고_파일은_남지_않는다() {
    let f = Fake::new();
    let provider = masked(&f, Arc::new(MaskSecrets::default()));

    provider
        .complete(request(
            Some(format!("너는 위험 담당이다. 참고 {TEST_SECRET}")),
            vec![msg(Role::User, "질문")],
            100,
        ))
        .await
        .unwrap();

    let system = f.only("system");
    assert!(
        !system.contains(TEST_SECRET),
        "시스템 글 파일에 비밀 글자: {system}"
    );
    assert_eq!(
        system.trim_end(),
        format!("너는 위험 담당이다. 참고 {MASK}")
    );
    assert!(
        leftovers(&f.work).is_empty(),
        "작업 폴더에 남은 파일: {:?}",
        leftovers(&f.work)
    );
}

// 검증: E38 V47 D24
#[tokio::test]
async fn 가리기는_인자의_모델과_환경의_최대_토큰을_바꾸지_않는다() {
    let f = Fake::new();
    let provider = masked(&f, Arc::new(MaskSecrets::default()));

    provider
        .complete(request(
            Some(format!("시스템 {TEST_SECRET}")),
            vec![msg(Role::User, &format!("질문 {TEST_SECRET}"))],
            321,
        ))
        .await
        .unwrap();

    let args = f.only("args");
    assert!(!args.contains(TEST_SECRET), "인자에 비밀 글자: {args}");
    assert_eq!(arg_after(&args, "--model"), Some(TEST_MODEL));
    let env = f.only("env");
    assert!(!env.contains(TEST_SECRET), "환경에 비밀 글자");
    assert_eq!(env_value(&env, MAX_TOKENS_ENV), Some("321"));
    assert_eq!(env_value(&env, "PATH"), Some("/usr/bin:/bin"));
}

// 검증: E38 V47
#[tokio::test]
async fn 아무것도_가리지_않는_자리를_끼우면_표준_입력과_시스템_글이_그대로_간다() {
    let f = Fake::new();
    let provider = ClaudeCliProvider::new(config(&f)).with_redactor(Arc::new(NoRedaction));

    provider
        .complete(request(
            Some("시스템 글 그대로".to_string()),
            vec![msg(Role::User, "대화 글 그대로")],
            100,
        ))
        .await
        .unwrap();

    assert_eq!(f.only("stdin").trim_end(), "대화 글 그대로");
    assert_eq!(f.only("system").trim_end(), "시스템 글 그대로");
}

// ---------- 2. 요청별 모델 · 최대 토큰 ----------

// 검증: D24
#[tokio::test]
async fn 한_공급자에서_요청마다_다른_모델과_최대_토큰이_인자와_환경에_실린다() {
    let f = Fake::new();
    let provider = ClaudeCliProvider::new(config(&f));
    let ask = |model: &str, max_tokens: u32| CompletionRequest {
        model: model.to_string(),
        system: None,
        messages: vec![msg(Role::User, model)],
        max_tokens,
    };

    let (a, b) = tokio::join!(
        provider.complete(ask("siheom-model-analyst", 111)),
        provider.complete(ask("siheom-model-trader", 2222)),
    );
    a.unwrap();
    b.unwrap();

    let records = f.records("args");
    assert_eq!(records.len(), 2, "호출 기록: {records:?}");
    let mut seen: Vec<(String, String, String)> = records
        .iter()
        .map(|(id, args)| {
            let model = arg_after(args, "--model").unwrap().to_string();
            let env = f.of("env", id);
            let tokens = env_value(&env, MAX_TOKENS_ENV).unwrap().to_string();
            let stdin = f.of("stdin", id).trim_end().to_string();
            (model, tokens, stdin)
        })
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        vec![
            (
                "siheom-model-analyst".to_string(),
                "111".to_string(),
                "siheom-model-analyst".to_string()
            ),
            (
                "siheom-model-trader".to_string(),
                "2222".to_string(),
                "siheom-model-trader".to_string()
            ),
        ]
    );
}

// 검증: D24 E38
#[tokio::test]
async fn 가리기를_끼워도_요청마다_모델과_최대_토큰이_따로_실린다() {
    let f = Fake::new();
    let provider = masked(&f, Arc::new(MaskSecrets::default()));
    let ask = |model: &str, max_tokens: u32| CompletionRequest {
        model: model.to_string(),
        system: None,
        messages: vec![msg(Role::User, &format!("{model} {TEST_SECRET}"))],
        max_tokens,
    };

    provider
        .complete(ask("siheom-model-analyst", 111))
        .await
        .unwrap();
    provider
        .complete(ask("siheom-model-risk", 4096))
        .await
        .unwrap();

    let records = f.records("args");
    assert_eq!(records.len(), 2, "호출 기록: {records:?}");
    let mut seen: Vec<(String, String, String)> = records
        .iter()
        .map(|(id, args)| {
            let model = arg_after(args, "--model").unwrap().to_string();
            let env = f.of("env", id);
            let tokens = env_value(&env, MAX_TOKENS_ENV).unwrap().to_string();
            let stdin = f.of("stdin", id).trim_end().to_string();
            (model, tokens, stdin)
        })
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        vec![
            (
                "siheom-model-analyst".to_string(),
                "111".to_string(),
                format!("siheom-model-analyst {MASK}")
            ),
            (
                "siheom-model-risk".to_string(),
                "4096".to_string(),
                format!("siheom-model-risk {MASK}")
            ),
        ]
    );
}
