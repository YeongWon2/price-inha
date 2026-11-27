//! 진단 8 (10차 10c 13번): 뜻 검색(Ollama). 설정 파일을 읽기만 하고 만들거나 고치지 않는다.
//! 꺼져 있으면 Ollama에 아무것도 묻지 않는다(V23). 켜져 있으면 그 루프백 주소의 `GET /api/tags` 한 번으로
//! 닿는지 · 모델을 받아 두었는지 본다.

use std::path::Path;
use std::time::Duration;

use pi_ai::AiError;
use serde_json::Value;

use super::{Check, CheckItem, bad, good};
use crate::reach::SemanticSettings;

/// (round64) Ollama 확인 한 번을 기다리는 가장 긴 시간(진단 전체가 멈추지 않게).
const CHECK_LIMIT: Duration = Duration::from_secs(5);

/// (round64) 설정 파일에 켜짐으로 남은 주소가 루프백이 아니면 그 주소(읽을 때는 꺼짐으로 본다).
fn outside_address(home: &Path) -> Option<String> {
    let text = std::fs::read_to_string(home.join(crate::SEMANTIC_SETTINGS_FILE)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let stored = SemanticSettings::from_json(&value)?;
    (stored.enabled && stored.provider().is_err()).then_some(stored.base_url)
}

/// 진단 8: 뜻 검색이 꺼져 있으면 문제가 아니다. 켜져 있으면 Ollama에 닿고 모델이 있어야 한다.
pub(super) fn check_ollama(home: &Path) -> Check {
    let item = CheckItem::Ollama;
    if let Some(address) = outside_address(home) {
        return bad(
            item,
            format!(
                "뜻 검색 설정 파일의 Ollama 주소({address})가 이 컴퓨터(루프백)가 아니라 뜻 검색을 꺼 두었습니다"
            ),
            "설정 화면의 뜻 검색에서 루프백 주소(예: http://127.0.0.1:11434)로 다시 저장하세요",
        );
    }
    let settings = SemanticSettings::load(home);
    if !settings.enabled {
        return good(item, "뜻 검색 꺼짐(Ollama에 묻지 않습니다)");
    }
    let (base_url, model) = (settings.base_url.clone(), settings.model.clone());
    match ask(&settings) {
        Some(Ok(())) => good(
            item,
            format!("뜻 검색 켜짐: Ollama {base_url} · 모델 {model}"),
        ),
        Some(Err(AiError::InvalidRequest { .. })) => bad(
            item,
            format!("뜻 검색 켜짐: Ollama({base_url})에 모델 {model}이 없습니다"),
            format!("ollama pull {model}"),
        ),
        _ => bad(
            item,
            format!("뜻 검색 켜짐: Ollama({base_url})에 닿지 않습니다"),
            "Ollama를 켜세요(ollama serve). 주소가 맞는지 설정 화면의 뜻 검색에서 확인하세요",
        ),
    }
}

/// 따로 된 실행기에서 한 번 묻는다(진단은 막는 함수다). 물을 수 없으면 없음.
fn ask(settings: &SemanticSettings) -> Option<Result<(), AiError>> {
    let provider = settings.provider().ok()?;
    let model = settings.model.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        // 시간 제한은 실행기 안에서 만든다(밖에서 만들면 시계가 없다).
        let asked = runtime
            .block_on(async { tokio::time::timeout(CHECK_LIMIT, provider.check(&model)).await });
        // 시간 안에 답하지 않으면 닿지 않은 것으로 본다(물을 수 없음).
        asked.ok()
    })
    .join()
    .ok()
    .flatten()
}
