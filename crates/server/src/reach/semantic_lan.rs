//! (65차, 결정 34) 같은 집 다른 기기의 Ollama: 임베딩을 이 컴퓨터 밖(사설망)으로 보낸다는 한 줄.
//! 설정 화면의 뜻 검색 칸(`GET`의 `outside`)과 `price-inha doctor`의 뜻 검색 줄이 늘 보인다.

use pi_ai::{OllamaReach, ollama_reach};

use super::semantic::SemanticSettings;

/// 사설망 주소면 "임베딩을 이 컴퓨터 밖(사설망 <주소>)으로 보냄"(켜기 전이면 "켜면 …"), 루프백이면 없음.
pub(crate) fn outside_line(settings: &SemanticSettings) -> Option<String> {
    let OllamaReach::PrivateNetwork { host } = ollama_reach(&settings.base_url) else {
        return None;
    };
    if !settings.private_network {
        return None;
    }
    let line = format!("임베딩을 이 컴퓨터 밖(사설망 {host})으로 보냄");
    Some(if settings.enabled {
        line
    } else {
        format!("켜면 {line}")
    })
}
