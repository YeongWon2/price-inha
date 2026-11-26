//! (57차, R18-4 · C32 · V78) 설정 화면에서 담당별 AI 모델 고르기([`SETUP_MODELS_PATH`]).

/// 담당 모델을 읽고(`GET`, 세션) 통째로 바꾸는(`POST`, 이 컴퓨터에서만) 경로.
pub const SETUP_MODELS_PATH: &str = "/api/setup/models";

/// 고른 담당 모델을 두는 파일 이름(보관 폴더 안). 비밀이 아니다.
pub const TEAM_MODELS_FILE: &str = "team-models.json";

/// 모델 이름의 최대 글자 수.
pub const MODEL_NAME_MAX: usize = 64;

/// 고를 수 있는 모델(`id` · `label` · `note`).
pub const MODEL_CHOICES: [(&str, &str, &str); 1] = [(
    crate::team::DEFAULT_TEAM_MODEL,
    "Claude Sonnet 5.5",
    "기본 · 빠름과 깊이의 균형",
)];

/// `price-inha setup ai` 구역 끝의 안내 한 줄.
pub const SETUP_MODELS_HINT: &str = "담당 모델은 설정 화면에서 바꿀 수 있습니다";
