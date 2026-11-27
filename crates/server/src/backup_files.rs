//! (62차) 백업 묶음에 함께 담는 보관 폴더의 설정 파일(backlog 묶음 D "하루 AI 상한 저장이 백업에 드는지").
//!
//! 비밀이 아닌 설정 파일만 담는다: 키(`.env`) · 옛 구독 토큰(`auth.json`) · 웹 푸시 비밀키(`vapid.key`) · 실행 증표
//! (`run.proof`) · 메신저 짝 맺기 장부(코드 · 보낸 사람 ID) · 메신저 읽은 위치는 넣지 않는다. 지금 백업의 규칙을
//! 그대로 따른다: 키 파일은 처음부터 대상이 아니고(`pi_secrets::BACKUP_EXCLUDED_FILES`는 이름을 줘도 백업기가
//! 뺀다), 담는 파일은 등록된 비밀 값 검사(E39)를 거친다. 되살리면 보관 폴더 바로 아래(제자리)로 돌아간다.

/// 백업에 담는 비밀이 아닌 설정 파일 이름들(보관 폴더 바로 아래). 없는 파일은 건너뛴다.
pub const BACKUP_CONFIG_FILES: &[&str] = &[
    crate::desk::AI_LIMIT_FILE,
    crate::setup_models::TEAM_MODELS_FILE,
    crate::team::TEAM_GUIDELINES_FILE,
    crate::notify_settings::NOTIFY_SETTINGS_FILE,
    crate::trading::RULES_CONFIG_FILE,
    crate::setup_sound::SOUND_FILE,
    crate::setup::WATCHLIST_FILE,
    crate::reach::SEMANTIC_SETTINGS_FILE,
    crate::reach::LOOKS_FILE,
    crate::ai_link::AI_CHOICE_FILE,
    crate::reach::BACKUP_SETTINGS_FILE,
];
