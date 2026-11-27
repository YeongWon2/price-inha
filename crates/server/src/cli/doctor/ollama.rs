//! 진단 8 (10차 10c 13번): 뜻 검색(Ollama). 설정 파일을 읽기만 하고 만들거나 고치지 않는다.
//! 꺼져 있으면 Ollama에 아무것도 묻지 않는다(V23). 켜져 있으면 그 루프백 주소의 `GET /api/tags` 한 번으로
//! 닿는지 · 모델을 받아 두었는지 본다.

use std::path::Path;

use pi_ai::AiError;

use super::{Check, CheckItem, bad, good};
use crate::reach::{SemanticSettings, outside_line};

/// 진단 8: 뜻 검색이 꺼져 있으면 문제가 아니다. 켜져 있으면 Ollama에 닿고 모델이 있어야 한다.
pub(super) fn check_ollama(home: &Path) -> Check {
    let item = CheckItem::Ollama;
    let settings = SemanticSettings::load(home);
    // (65차, 결정 34) 같은 집 다른 기기의 Ollama면 "임베딩을 이 컴퓨터 밖(사설망 …)으로 보냄"을 늘 덧붙인다.
    let outside = outside_line(&settings)
        .map(|line| format!(" · {line}"))
        .unwrap_or_default();
    if !settings.enabled {
        return good(
            item,
            format!("뜻 검색 꺼짐(Ollama에 묻지 않습니다){outside}"),
        );
    }
    let (base_url, model) = (settings.base_url.clone(), settings.model.clone());
    match ask(&settings) {
        Some(Ok(())) => good(
            item,
            format!("뜻 검색 켜짐: Ollama {base_url} · 모델 {model}{outside}"),
        ),
        Some(Err(AiError::InvalidRequest { .. })) => bad(
            item,
            format!("뜻 검색 켜짐: Ollama({base_url})에 모델 {model}이 없습니다{outside}"),
            format!("ollama pull {model}"),
        ),
        _ => bad(
            item,
            format!("뜻 검색 켜짐: Ollama({base_url})에 닿지 않습니다{outside}"),
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
        Some(runtime.block_on(provider.check(&model)))
    })
    .join()
    .ok()
    .flatten()
}
