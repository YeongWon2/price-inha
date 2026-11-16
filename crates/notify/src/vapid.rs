//! VAPID(RFC 8292): ES256 서명과 키 파일.

use std::path::Path;

use pi_store::UnixMillis;

use crate::Error;

/// 비밀 폴더 안 VAPID 키 파일 이름. 백업은 이 파일을 대상에서 뺀다(E39).
pub const VAPID_KEY_FILE: &str = "vapid.key";

/// VAPID 서명 키(P-256). `Debug`에 비밀 값이 나오지 않는다.
pub struct VapidKey {
    _private: (),
}

impl std::fmt::Debug for VapidKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VapidKey").finish_non_exhaustive()
    }
}

impl VapidKey {
    /// 메모리에만 있는 새 키.
    pub fn generate() -> VapidKey {
        todo!()
    }

    /// `secret_dir/`[`VAPID_KEY_FILE`]에서 키를 읽는다. 없으면 만들어 0600(Unix)으로 쓴다
    /// (폴더가 없으면 만든다). 있으면 그대로 읽고 파일을 바꾸지 않는다(다시 열어도 같은 키).
    /// 권한이 0600보다 넓으면 0600으로 좁힌다. 파일 내용이 키가 아니면 [`Error::InvalidKey`]이고
    /// 파일을 덮어쓰지 않는다. 읽고 쓰지 못하면 [`Error::KeyFile`].
    pub fn load_or_create(secret_dir: &Path) -> Result<VapidKey, Error> {
        let _ = secret_dir;
        todo!()
    }

    /// 공개키: 비압축 점 65바이트(첫 바이트 0x04). 브라우저의 `applicationServerKey`다.
    pub fn public_key(&self) -> Vec<u8> {
        todo!()
    }

    /// 공개키를 base64url(패딩 없음)로.
    pub fn public_key_b64url(&self) -> String {
        todo!()
    }

    /// 서명된 JWT(`헤더.내용.서명`, 각각 base64url 패딩 없음). 헤더 `{"typ":"JWT","alg":"ES256"}`,
    /// 내용 `aud`(푸시 서비스 주소의 origin) · `exp`(초, `now`보다 뒤이고 24시간 이내) · `sub`.
    /// 서명은 r||s 64바이트(ES256).
    pub fn sign_jwt(&self, audience: &str, subject: &str, now: UnixMillis) -> String {
        let _ = (audience, subject, now);
        todo!()
    }

    /// `Authorization` 머리글 값: `vapid t=<JWT>, k=<공개키 base64url>`.
    /// `aud`는 `endpoint`의 origin(`scheme://host[:port]`)이다. `endpoint`가 주소가 아니면 [`Error::InvalidKey`].
    pub fn authorization(
        &self,
        endpoint: &str,
        subject: &str,
        now: UnixMillis,
    ) -> Result<String, Error> {
        let _ = (endpoint, subject, now);
        todo!()
    }
}
