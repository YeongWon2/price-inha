//! AI 연결 구역(결정 24 · D4, 원본 동작 근거 `docs/research/onboarding-messenger.md` 2.2 — 이미 로그인된 CLI를
//! 먼저 권한다). `claude`를 **`PATH`에서만** 찾아 지금의 확인 길(버전 → 로그인, 모델을 부르지 않는다)로 보고,
//! 고른 것만 도는 서버의 설정 경로로 등록 전 확인을 거쳐 저장한다.
//! 다른 프로그램의 자격 파일 · 키체인을 읽지 않고, 로그인 흐름을 직접 하지 않는다(로그인은 사용자가 다른 창에서
//! 공식 CLI로 한다).

use std::path::{Path, PathBuf};
use std::time::Duration;

use pi_ai::{ClaudeCliConfig, ClaudeCliProvider, CliStatus};
use serde_json::json;

use super::prompt::Prompter;
use super::server::{Link, Reply, gives_up};
use super::summary::Outcome;
use super::{ANSWER_CLI, ANSWER_KEY, ANSWER_SKIP, SETUP_CLI_BILLING_WARNING, SETUP_CLI_LOGIN_HINT};
use crate::setup::{KEY_AI_API_KEY, SETUP_KEYS_PATH};
use crate::{SETUP_AI_PATH, cli_base_env, cli_refusal, find_claude};

/// CLI 확인 한 번의 상한.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// 마법사의 바깥(환경 읽기).
pub(crate) struct AiEnv<'a> {
    pub(crate) env: &'a dyn Fn(&str) -> Option<String>,
}

/// 찾은 CLI와 그 확인 결과.
struct Found {
    command: PathBuf,
    status: CliStatus,
}

/// AI 연결 구역을 돈다.
pub(crate) async fn ai(link: &Link, prompter: &mut Prompter<'_>, outside: &AiEnv<'_>) -> Outcome {
    prompter.title("AI 연결");
    let mut found = match find_claude((outside.env)("PATH").as_deref(), None, &[]) {
        Some(command) => {
            let status = check(&command, outside).await;
            Some(Found { command, status })
        }
        None => None,
    };
    describe(prompter, found.as_ref());
    loop {
        let logged_in = found
            .as_ref()
            .is_some_and(|f| matches!(f.status, CliStatus::LoggedIn { .. }));
        let default = if logged_in { ANSWER_CLI } else { ANSWER_SKIP };
        let question = format!(
            "고르세요 — {ANSWER_CLI}: Claude CLI(구독) / {ANSWER_KEY}: API 키 넣기 / {ANSWER_SKIP}: 건너뛰기 (빈 줄 = {default}):"
        );
        let Some(answer) = prompter.line(&question) else {
            return Outcome::ended();
        };
        let answer = if answer.is_empty() {
            default.to_string()
        } else {
            answer.to_lowercase()
        };
        let outcome = match answer.as_str() {
            ANSWER_CLI => match found.as_mut() {
                Some(cli) => choose_cli(link, prompter, outside, cli).await,
                None => {
                    prompter.say("PATH에서 claude를 찾지 못해 고를 수 없습니다");
                    None
                }
            },
            ANSWER_KEY => Some(api_key(link, prompter).await),
            ANSWER_SKIP => Some(Outcome::skipped("고르지 않음")),
            _ => {
                prompter.say("모르는 답입니다");
                None
            }
        };
        if let Some(outcome) = outcome {
            // (57차) 담당 모델은 마법사에서 고르지 않는다 — 설정 화면으로 안내만 한다.
            prompter.say(crate::SETUP_MODELS_HINT);
            return outcome;
        }
    }
}

/// 찾은 결과를 보인다(경로 · 상태만, 자격 값은 없다).
fn describe(prompter: &mut Prompter<'_>, found: Option<&Found>) {
    let Some(found) = found else {
        prompter.say("Claude CLI: PATH에서 찾지 못함");
        return;
    };
    let state = match &found.status {
        CliStatus::LoggedIn {
            subscription_type, ..
        } => match subscription_type {
            Some(plan) => format!("로그인됨(구독 {plan})"),
            None => "로그인됨(구독)".to_string(),
        },
        CliStatus::NotLoggedIn => "로그인 안 됨".to_string(),
        other => cli_refusal(other).map_or_else(String::new, |why| why.to_string()),
    };
    prompter.say(&format!(
        "Claude CLI: {} — {state}",
        found.command.display()
    ));
}

/// CLI를 고른다. 로그인 안 됐으면 안내 → Enter → 다시 확인을 되풀이한다. 다시 물어야 하면 `None`.
async fn choose_cli(
    link: &Link,
    prompter: &mut Prompter<'_>,
    outside: &AiEnv<'_>,
    found: &mut Found,
) -> Option<Outcome> {
    if (outside.env)("ANTHROPIC_API_KEY").is_some_and(|value| !value.is_empty()) {
        prompter.say(SETUP_CLI_BILLING_WARNING);
    }
    while matches!(found.status, CliStatus::NotLoggedIn) {
        prompter.say(SETUP_CLI_LOGIN_HINT);
        if prompter.wait_enter().is_none() {
            return Some(Outcome::ended());
        }
        found.status = check(&found.command, outside).await;
        if matches!(found.status, CliStatus::LoggedIn { .. }) {
            prompter.say("로그인을 확인했습니다");
        }
    }
    if let Some(why) = cli_refusal(&found.status) {
        prompter.say(&why.to_string());
        return None;
    }
    let body = json!({ "method": "cli", "command": found.command.to_str() })
        .to_string()
        .into_bytes();
    match link.post(SETUP_AI_PATH, body).await {
        Reply::Accepted => {
            prompter.say("AI 연결을 Claude CLI로 정했습니다");
            Some(Outcome::done("Claude CLI(로그인됨)"))
        }
        Reply::Refused(refusal) => {
            prompter.say(&refusal.to_string());
            gives_up(refusal).then(|| Outcome::failed(&refusal.to_string()))
        }
        Reply::Broken => Some(Outcome::failed(super::BROKEN)),
    }
}

/// API 키를 받아 등록하고, 됐으면 API 키 방식을 고른다. 빈 줄은 건너뛰기.
async fn api_key(link: &Link, prompter: &mut Prompter<'_>) -> Outcome {
    loop {
        let Some(key) =
            prompter.secret("Anthropic API 키(보이지 않게 읽습니다, 빈 줄 = 건너뛰기):")
        else {
            return Outcome::ended();
        };
        if key.is_empty() {
            return Outcome::skipped("키를 넣지 않음");
        }
        let body = super::broker::key_body(KEY_AI_API_KEY, &[("api_key", key)]);
        match link.post(SETUP_KEYS_PATH, body).await {
            Reply::Accepted => break,
            Reply::Refused(refusal) => {
                prompter.say(&refusal.to_string());
                if gives_up(refusal) {
                    return Outcome::failed(&refusal.to_string());
                }
                prompter.say("다시 넣으세요");
            }
            Reply::Broken => return Outcome::failed(super::BROKEN),
        }
    }
    let body = json!({ "method": "api_key" }).to_string().into_bytes();
    match link.post(SETUP_AI_PATH, body).await {
        Reply::Accepted => {
            prompter.say("API 키를 등록하고 AI 연결을 API 키로 정했습니다");
            Outcome::done("API 키")
        }
        Reply::Refused(refusal) => {
            prompter.say(&refusal.to_string());
            Outcome::failed(&refusal.to_string())
        }
        Reply::Broken => Outcome::failed(super::BROKEN),
    }
}

/// 서버와 같은 확인(버전 → 로그인, 모델을 부르지 않는다). 환경은 허용 목록만, 작업 폴더는 임시 폴더.
async fn check(command: &Path, outside: &AiEnv<'_>) -> CliStatus {
    let config = ClaudeCliConfig::new(
        Some(command.to_path_buf()),
        std::env::temp_dir(),
        cli_base_env(outside.env),
        CHECK_TIMEOUT,
    );
    let provider = ClaudeCliProvider::new(config);
    let status = tokio::time::timeout(CHECK_TIMEOUT, provider.check())
        .await
        .unwrap_or(CliStatus::TimedOut);
    provider.shutdown().await;
    status
}
