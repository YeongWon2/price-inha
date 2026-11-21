//! 터미널의 접속 키 만들기(`price-inha access-key [--rotate]`). 화면의 `/api/access/key`와 같은 저장소 ·
//! 같은 규칙(소금 친 SHA-256만 저장, 새 세대, 앞 세대 세션 끊기, 접속 기록). 원본 동작: Hermes 2.3 ·
//! OpenClaw 3.3(화면 없는 컴퓨터의 처음 설정은 터미널에서 일회용 자격을 한 번 보여 준다).

use pi_gate::{Gate, GateConfig};
use pi_secrets::SecretStore;
use pi_store::Store;

use crate::error::CliError;
use crate::{AppError, DB_FILE, Settings};

/// `access-key`의 선택.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessKeyRequest {
    /// 이미 있는 키를 바꾼다.
    pub rotate: bool,
    /// 바깥에서 들어오는 이름(https 프록시 뒤의 원격 이름, 포트 없이). 있으면 `https://<이름>/` 안내를 함께.
    pub remote_name: Option<String>,
}

/// 터미널에 한 번 보여 줄 접속 키. `Debug`는 키를 찍지 않는다.
pub struct ShownAccessKey {
    key: String,
    generation: u64,
    text: String,
}

impl ShownAccessKey {
    /// 만든 키(터미널에 한 번만 보여 준다).
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 새 세대 번호.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// 터미널에 보여 줄 글. 키는 여기에 한 번만 나온다.
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl std::fmt::Debug for ShownAccessKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShownAccessKey")
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

/// 접속 키를 만든다. 이미 있으면 `rotate` 없이는 [`CliError::AccessKeyExists`]이고 그대로 둔다.
/// 키 글자는 답 말고는 어디에도(기록 · 파일) 남기지 않는다.
pub async fn access_key(
    settings: &Settings,
    request: &AccessKeyRequest,
) -> Result<ShownAccessKey, CliError> {
    // 보관 폴더가 없으면 서버가 켤 때처럼 만든다(0700).
    SecretStore::open(settings.home.clone()).map_err(AppError::from)?;
    let store = Store::open(settings.home.join(DB_FILE)).map_err(AppError::from)?;
    if store.access_key().map_err(AppError::from)?.is_some() && !request.rotate {
        return Err(CliError::AccessKeyExists);
    }
    // 화면의 `/api/access/key`와 같은 손잡이(`pi_gate::AccessKeys::issue`)로 만든다: 같은 저장소 · 같은 규칙
    // (운영체제 난수, 소금 친 SHA-256만 저장, 새 세대, 접속 기록). 관문은 루프백 빈 포트에 잠깐만 연다.
    let gate = Gate::builder(store, GateConfig::local(0))
        .build()
        .serve()
        .await
        .map_err(AppError::from)?;
    let issued = gate.access_keys().issue().await;
    let closed = gate.shutdown().await;
    let issued = issued.map_err(AppError::from)?;
    closed.map_err(AppError::from)?;
    let text = shown_text(
        &issued.key,
        issued.generation,
        request.remote_name.as_deref(),
    );
    Ok(ShownAccessKey {
        key: issued.key,
        generation: issued.generation,
        text,
    })
}

/// 터미널에 보여 줄 글. 키는 한 번만 나온다.
fn shown_text(key: &str, generation: u64, remote_name: Option<&str>) -> String {
    let mut text = format!(
        "새 접속 키(세대 {generation}):\n\n    {key}\n\n\
이 키는 지금 한 번만 보여 줍니다. 저장소에는 해시만 남습니다. 앞 세대 키로 들어온 원격 연결은 끊깁니다.\n"
    );
    match remote_name {
        Some(name) => text.push_str(&format!(
            "다른 기기의 브라우저에서 https://{name}/ 로 들어와 이 키를 넣습니다. \
그다음 설정 화면에서 키를 등록합니다.\n"
        )),
        None => text.push_str(
            "다른 기기의 브라우저에서 이 서버로 들어와 이 키를 넣습니다. 그다음 설정 화면에서 키를 등록합니다.\n",
        ),
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 키는_글에_한_번만() {
        let text = shown_text("ABCDEFGHJKMNPQRSTVWX", 3, Some("mini.example.test"));
        assert_eq!(text.matches("ABCDEFGHJKMNPQRSTVWX").count(), 1);
        assert!(text.contains("https://mini.example.test/"));
        assert!(!shown_text("K", 1, None).contains("https://"));
    }
}
