//! (6차 · 7차) 설정 책상이 쥐는 AI 연결: 고른 연결의 작업 폴더 준비와 공급자 만들기.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pi_ai::{AiProvider, AnthropicConfig, AnthropicProvider, ClaudeCliProvider, CliStatus};
use pi_secrets::SecretStore;

use crate::ai_link::{Leftovers, prepare_work_dir};
use crate::{AiChoice, AiKeys, AiMethod, AppError, CLI_WORK_DIR, cli_config};

/// 고른 연결의 작업 폴더 준비 결과. (7차 9번 · 10번)
pub(crate) enum Work {
    /// API 키라 작업 폴더가 필요 없다.
    Unneeded,
    /// 준비됐다(비어 있는 0700 폴더).
    Ready(PathBuf),
    /// 이미 있는 작업 폴더가 비어 있지 않다. 안의 것은 지우지 않았고 CLI를 쓰지 않는다.
    NotEmpty,
}

/// 고른 연결의 작업 폴더를 준비한다. 막히는 파일 작업이므로 `spawn_blocking`에서 부른다.
/// (8차 1번) 자기 임시 파일만 남아 있으면 비어 있는 것이고, `leftovers`대로 지우거나 둔다.
pub(crate) fn prepare_work(
    home: &Path,
    choice: &AiChoice,
    leftovers: Leftovers,
) -> Result<Work, AppError> {
    if choice.method != AiMethod::Cli {
        return Ok(Work::Unneeded);
    }
    match prepare_work_dir(home, leftovers) {
        Ok(work) => Ok(Work::Ready(work)),
        Err(AppError::Home {
            kind: ErrorKind::DirectoryNotEmpty,
        }) => Ok(Work::NotEmpty),
        Err(error) => Err(error),
    }
}

/// (6차) AI 연결을 잇는 데 드는 것. 비밀 값은 없다.
pub(crate) struct AiParts {
    /// API 키 공급자 설정. 시간 제한은 CLI 공급자도 같이 쓴다.
    pub(crate) anthropic: AnthropicConfig,
    /// CLI 자식에게 줄 환경(허용 목록만).
    pub(crate) cli_env: Vec<(String, String)>,
    /// 사용자 홈 폴더(CLI 실행 파일을 찾을 때).
    pub(crate) user_home: Option<PathBuf>,
    /// `PATH`와 사용자 홈 다음에 CLI 실행 파일을 찾는 자리.
    pub(crate) cli_locations: Vec<PathBuf>,
    /// 띄울 때 고른 것.
    pub(crate) choice: AiChoice,
}

/// 쥐고 있는 공급자. CLI는 확인을 부르려고 제 모양으로 쥔다.
#[derive(Clone)]
pub(super) enum Held {
    Api(Arc<AnthropicProvider>),
    Cli(Arc<ClaudeCliProvider>),
}

impl Held {
    pub(super) fn provider(&self) -> Arc<dyn AiProvider> {
        match self {
            Held::Api(provider) => Arc::clone(provider) as Arc<dyn AiProvider>,
            Held::Cli(provider) => Arc::clone(provider) as Arc<dyn AiProvider>,
        }
    }
}

/// 지금 고른 AI 연결과 그 공급자, 마지막 CLI 확인 결과.
pub(super) struct AiNow {
    pub(super) choice: AiChoice,
    /// 마지막 CLI 확인 결과. 아직 확인 중이면 없다.
    pub(super) cli: Option<CliStatus>,
    /// 작업 폴더가 비어 있지 않아 CLI를 쓰지 않는다(확인도 하지 않는다).
    pub(super) work_blocked: bool,
    pub(super) held: Held,
}

/// 고른 연결로 공급자를 만든다. 아무것도 띄우지 않고 파일도 건드리지 않는다.
/// 작업 폴더가 준비되지 않았으면(비어 있지 않음) 실행 파일 없는 CLI 공급자를 쥔다:
/// 부르면 띄우지 않고 실패한다(`pi_ai::AiError::NotConfigured`).
pub(super) fn hold(
    parts: &AiParts,
    secrets: &Arc<SecretStore>,
    home: &Path,
    choice: &AiChoice,
    work: &Work,
) -> Held {
    let config = match (choice.method, work) {
        (AiMethod::ApiKey, _) => {
            return Held::Api(Arc::new(AnthropicProvider::new(
                parts.anthropic.clone(),
                Arc::new(AiKeys::new(Arc::clone(secrets))),
            )));
        }
        (AiMethod::Cli, Work::Ready(dir)) => cli_config(
            choice,
            dir.clone(),
            parts.cli_env.clone(),
            parts.anthropic.timeout,
        ),
        (AiMethod::Cli, _) => {
            let unusable = AiChoice {
                command: None,
                ..choice.clone()
            };
            cli_config(
                &unusable,
                home.join(CLI_WORK_DIR),
                Vec::new(),
                parts.anthropic.timeout,
            )
        }
    };
    Held::Cli(Arc::new(ClaudeCliProvider::new(config)))
}
