//! 터미널의 접속 키 만들기(`price-inha access-key [--rotate]`). 화면의 `/api/access/key`와 같은 저장소 ·
//! 같은 규칙(소금 친 SHA-256만 저장, 새 세대, 앞 세대 세션 끊기, 접속 기록). 원본 동작: Hermes 2.3 ·
//! OpenClaw 3.3(화면 없는 컴퓨터의 처음 설정은 터미널에서 일회용 자격을 한 번 보여 준다).

use crate::Settings;
use crate::error::CliError;

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
    let _ = (settings, request);
    todo!()
}
