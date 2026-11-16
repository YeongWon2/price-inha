//! pi-ai 8차: CLI 공급자의 크기 상한과 가리기. docs/dev/modules/ai.md "8차 작업" 2 · 3번.
//!
//! 실제 `claude`를 부르지 않는다. 임시 폴더의 가짜 실행 파일(표준 입력을 버리고 결과 한 줄을 내는
//! 셸 스크립트)만 쓴다. 상한(`CLI_MAX_STDIN_BYTES`)은 가린 뒤의 길이에 걸리고(7차 1),
//! 가리기 전 길이가 상한의 4배를 넘으면 가리기를 부르지 않고 바로 거절한다(8차 3).
//! 8차 1번(남은 손주를 기다리다 취소될 때)은 공개 API로 그 틈을 일으킬 수 없어 `src` 안 단위 테스트 몫이다.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use pi_ai::{
    AiError, AiProvider, CLI_MAX_STDIN_BYTES, ClaudeCliConfig, ClaudeCliProvider,
    CompletionRequest, Message, Redactor, Role,
};
use serde_json::json;
use tempfile::TempDir;

/// 글에 섞여 들어가는 시험용 비밀 글자(17바이트).
const TEST_SECRET: &str = "SIHEOM-GARIM-0717";
/// 시험용 가리기가 바꿔 넣는 글자(8바이트).
const MASK: &str = "[가림]";
/// 가리기 전 상한: 상한의 4배(8차 3).
const PRECHECK_BYTES: usize = 4 * CLI_MAX_STDIN_BYTES;

/// 시험용 가리기: 비밀 글자를 줄이고(`shrink`) 또는 `a`를 두 배로 늘리며(`!shrink`), 불린 횟수를 센다.
struct Counting {
    shrink: bool,
    calls: AtomicUsize,
}

impl Counting {
    fn new(shrink: bool) -> Arc<Self> {
        Arc::new(Self {
            shrink,
            calls: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Redactor for Counting {
    fn redact(&self, text: &str) -> String {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.shrink {
            text.replace(TEST_SECRET, MASK)
        } else {
            text.replace('a', "aa")
        }
    }
}

struct Fake {
    _root: TempDir,
    bin: PathBuf,
    work: PathBuf,
}

impl Fake {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let work = root.join("work");
        fs::create_dir(&work).unwrap();
        let bin = root.join("claude");
        let result = json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "result": "가짜 답",
            "stop_reason": "end_turn",
            "num_turns": 1,
            "permission_denials": [],
            "usage": {"input_tokens": 3, "output_tokens": 4},
        });
        fs::write(
            &bin,
            format!("#!/bin/sh\n/bin/cat > /dev/null\nprintf '%s\\n' '{result}'\nexit 0\n"),
        )
        .unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _root: dir,
            bin,
            work,
        }
    }

    fn provider(&self, redactor: Arc<Counting>) -> ClaudeCliProvider {
        let config = ClaudeCliConfig {
            command: Some(self.bin.clone()),
            config_dir: None,
            work_dir: self.work.clone(),
            base_env: vec![("PATH".to_string(), "/usr/bin:/bin".to_string())],
            timeout: Duration::from_secs(30),
            max_concurrency: 2,
            max_output_bytes: 1_000_000,
            kill_grace: Duration::from_millis(300),
            version_timeout: Duration::from_millis(1500),
            auth_timeout: Duration::from_millis(1500),
        };
        ClaudeCliProvider::new(config).with_redactor(redactor)
    }
}

fn request(system: Option<String>, content: String) -> CompletionRequest {
    CompletionRequest {
        model: "siheom-model-g".to_string(),
        system,
        messages: vec![Message {
            role: Role::User,
            content,
        }],
        max_tokens: 100,
    }
}

fn is_invalid_request(result: &Result<pi_ai::CompletionResponse, AiError>) -> bool {
    matches!(result, Err(AiError::InvalidRequest { .. }))
}

// ---------- 2. 상한은 가린 뒤의 길이에 걸린다(경계) ----------

// 검증: E38 V47
#[tokio::test]
async fn 가린_뒤_길이가_상한과_같으면_통과한다() {
    let f = Fake::new();
    let redactor = Counting::new(true);
    let provider = f.provider(redactor.clone());
    // 비밀 1000개(17000바이트 -> 8000바이트)와 채움 글자: 가린 뒤 길이가 정확히 상한이다.
    let content = format!(
        "{}{}",
        TEST_SECRET.repeat(1000),
        "b".repeat(CLI_MAX_STDIN_BYTES - 8000)
    );
    assert!(
        content.len() > CLI_MAX_STDIN_BYTES,
        "시험 준비: 가리기 전엔 상한을 넘어야 한다"
    );

    let response = provider.complete(request(None, content)).await.unwrap();
    assert_eq!(response.text, "가짜 답");
    assert!(redactor.calls() >= 1, "가리기를 부르지 않았다");
}

// 검증: E38 V47
#[tokio::test]
async fn 가린_뒤_길이가_상한보다_한_바이트_크면_잘못된_요청이다() {
    let f = Fake::new();
    let provider = f.provider(Counting::new(true));
    let content = format!(
        "{}{}",
        TEST_SECRET.repeat(1000),
        "b".repeat(CLI_MAX_STDIN_BYTES - 8000 + 1)
    );

    let result = provider.complete(request(None, content)).await;
    assert!(is_invalid_request(&result), "{result:?}");
}

// 검증: E38 V47
#[tokio::test]
async fn 가리기_전에는_상한을_넘어도_가린_뒤_상한_안이면_통과한다() {
    let f = Fake::new();
    let redactor = Counting::new(true);
    let provider = f.provider(redactor.clone());
    // 15MB가 7MB 남짓으로 줄어든다(상한의 4배보다 훨씬 작으므로 가리기 전에 거절하지 않는다).
    let content = TEST_SECRET.repeat(882_353);
    assert!(content.len() > CLI_MAX_STDIN_BYTES);

    let response = provider.complete(request(None, content)).await.unwrap();
    assert_eq!(response.text, "가짜 답");
    assert_eq!(redactor.calls(), 1, "대화 글 하나만 가려야 한다");
}

// 검증: E38 V47
#[tokio::test]
async fn 가리기_전에는_상한_안이어도_가린_뒤_상한을_넘으면_잘못된_요청이다() {
    let f = Fake::new();
    let redactor = Counting::new(false);
    let provider = f.provider(redactor.clone());
    // 6MB가 12MB로 늘어난다.
    let content = "a".repeat(6_000_000);

    let result = provider.complete(request(None, content)).await;
    assert!(is_invalid_request(&result), "{result:?}");
    assert!(redactor.calls() >= 1, "가리기를 부르지 않았다");
}

// ---------- 3. 아주 큰 입력은 가리기 전에 거절한다 ----------

// 검증: E38 V47
#[tokio::test]
async fn 가리기_전_길이가_상한의_네_배를_넘으면_가리지_않고_거절한다() {
    let f = Fake::new();
    let redactor = Counting::new(true);
    let provider = f.provider(redactor.clone());
    let content = "b".repeat(PRECHECK_BYTES + 1);

    let result = provider.complete(request(None, content)).await;
    assert!(is_invalid_request(&result), "{result:?}");
    assert_eq!(redactor.calls(), 0, "거절하기 전에 가리기를 불렀다");
}

// 검증: E38 V47
#[tokio::test]
async fn 시스템_글과_대화_글을_합친_길이도_가리기_전_상한에_든다() {
    let f = Fake::new();
    let redactor = Counting::new(true);
    let provider = f.provider(redactor.clone());
    let system = "b".repeat(PRECHECK_BYTES / 2 + 1);
    let content = "c".repeat(PRECHECK_BYTES / 2);

    let result = provider.complete(request(Some(system), content)).await;
    assert!(is_invalid_request(&result), "{result:?}");
    assert_eq!(redactor.calls(), 0, "거절하기 전에 가리기를 불렀다");
}

// 검증: E38 V47
#[tokio::test]
async fn 가리기_전_길이가_상한의_네_배와_같으면_가린_뒤_길이로_판단한다() {
    let f = Fake::new();
    let redactor = Counting::new(true);
    let provider = f.provider(redactor.clone());
    // 정확히 상한의 4배의 비밀 글자들: 가리기 전엔 거절하지 않고, 가린 뒤(약 18.8MB)에 상한을 넘어 거절한다.
    let unit = TEST_SECRET.len();
    let content = TEST_SECRET.repeat(PRECHECK_BYTES / unit);
    let pad = "b".repeat(PRECHECK_BYTES - content.len());
    let content = format!("{content}{pad}");
    assert_eq!(content.len(), PRECHECK_BYTES);

    let result = provider.complete(request(None, content)).await;
    assert!(is_invalid_request(&result), "{result:?}");
    assert!(
        redactor.calls() >= 1,
        "가리기 전 길이가 4배와 같은데 가리기 없이 거절했다"
    );
}
