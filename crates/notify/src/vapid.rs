//! VAPID(RFC 8292): ES256 서명과 키 파일.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use p256::SecretKey;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use pi_store::UnixMillis;
use url::Url;

use crate::Error;
use crate::webpush::random_secret_key;

/// 비밀 폴더 안 VAPID 키 파일 이름. 백업은 이 파일을 대상에서 뺀다(E39).
pub const VAPID_KEY_FILE: &str = "vapid.key";

/// JWT 만료: 서명 시각부터 12시간(RFC 8292 2: 24시간을 넘지 않는다).
const JWT_LIFETIME_SECS: i64 = 12 * 3_600;

/// VAPID 서명 키(P-256). `Debug`에 비밀 값이 나오지 않는다.
pub struct VapidKey {
    secret: SecretKey,
}

impl std::fmt::Debug for VapidKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VapidKey").finish_non_exhaustive()
    }
}

impl VapidKey {
    /// 메모리에만 있는 새 키.
    pub fn generate() -> VapidKey {
        VapidKey {
            secret: random_secret_key(),
        }
    }

    /// `secret_dir/`[`VAPID_KEY_FILE`]에서 키를 읽는다. 없으면 만들어 0600(Unix)으로 쓴다
    /// (폴더가 없으면 만든다). 있으면 그대로 읽고 파일을 바꾸지 않는다(다시 열어도 같은 키).
    /// 권한이 0600보다 넓으면 0600으로 좁힌다. 파일 내용이 키가 아니면 [`Error::InvalidKey`]이고
    /// 파일을 덮어쓰지 않는다. 읽고 쓰지 못하면 [`Error::KeyFile`].
    /// 새 키는 같은 폴더의 임시 파일(0600)에 다 쓴 뒤 이름을 바꿔 둔다(쓰다 꺼져도 빈 키 파일이 남지 않는다).
    /// 키 파일이 비었거나 공백뿐이면(예전에 쓰다 꺼진 흔적) 없는 것으로 보고 새로 만든다.
    pub fn load_or_create(secret_dir: &Path) -> Result<VapidKey, Error> {
        fs::create_dir_all(secret_dir)
            .map_err(|e| Error::KeyFile(format!("비밀 폴더를 만들지 못했다: {}", e.kind())))?;
        remove_leftover_temp_files(secret_dir);
        let path = secret_dir.join(VAPID_KEY_FILE);
        if path.exists() {
            restrict_permissions(&path)?;
            let text = fs::read_to_string(&path)
                .map_err(|e| Error::KeyFile(format!("키 파일을 읽지 못했다: {}", e.kind())))?;
            if !text.trim().is_empty() {
                return Self::from_text(&text);
            }
        }
        let key = Self::generate();
        let text = format!("{}\n", URL_SAFE_NO_PAD.encode(key.secret.to_bytes()));
        write_private_file_atomically(&path, text.as_bytes())?;
        Ok(key)
    }

    /// 공개키: 비압축 점 65바이트(첫 바이트 0x04). 브라우저의 `applicationServerKey`다.
    pub fn public_key(&self) -> Vec<u8> {
        self.secret.public_key().to_sec1_bytes().to_vec()
    }

    /// 공개키를 base64url(패딩 없음)로.
    pub fn public_key_b64url(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.public_key())
    }

    /// 서명된 JWT(`헤더.내용.서명`, 각각 base64url 패딩 없음). 헤더 `{"typ":"JWT","alg":"ES256"}`,
    /// 내용 `aud`(푸시 서비스 주소의 origin) · `exp`(초, `now`보다 뒤이고 24시간 이내) · `sub`.
    /// 서명은 r||s 64바이트(ES256).
    pub fn sign_jwt(&self, audience: &str, subject: &str, now: UnixMillis) -> String {
        let header = serde_json::json!({"typ": "JWT", "alg": "ES256"});
        let exp = now.0.div_euclid(1_000) + JWT_LIFETIME_SECS;
        let claims = serde_json::json!({"aud": audience, "exp": exp, "sub": subject});
        let signing_input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header.to_string()),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        let signing = SigningKey::from(&self.secret);
        let signature: Signature = signing.sign(signing_input.as_bytes());
        format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        )
    }

    /// `Authorization` 머리글 값: `vapid t=<JWT>, k=<공개키 base64url>`.
    /// `aud`는 `endpoint`의 origin(`scheme://host[:port]`)이다. `endpoint`가 주소가 아니면 [`Error::InvalidKey`].
    pub fn authorization(
        &self,
        endpoint: &str,
        subject: &str,
        now: UnixMillis,
    ) -> Result<String, Error> {
        let url = Url::parse(endpoint)
            .map_err(|_| Error::InvalidKey("구독 주소가 주소 모양이 아니다".to_owned()))?;
        let origin = url.origin();
        if !origin.is_tuple() {
            return Err(Error::InvalidKey("구독 주소에 origin이 없다".to_owned()));
        }
        let jwt = self.sign_jwt(&origin.ascii_serialization(), subject, now);
        Ok(format!("vapid t={jwt}, k={}", self.public_key_b64url()))
    }

    fn from_text(text: &str) -> Result<VapidKey, Error> {
        let not_key = || Error::InvalidKey("VAPID 키 파일 내용이 키가 아니다".to_owned());
        let bytes = URL_SAFE_NO_PAD.decode(text.trim()).map_err(|_| not_key())?;
        if bytes.len() != 32 {
            return Err(not_key());
        }
        let secret = SecretKey::from_slice(&bytes).map_err(|_| not_key())?;
        Ok(VapidKey { secret })
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::metadata(path)
        .map_err(|e| Error::KeyFile(format!("키 파일 정보를 읽지 못했다: {}", e.kind())))?;
    if meta.permissions().mode() & 0o077 != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| Error::KeyFile(format!("키 파일 권한을 좁히지 못했다: {}", e.kind())))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), Error> {
    Ok(())
}

/// 임시 키 파일 이름의 앞부분. 백업 · 정리에서 이 모양을 남은 임시 파일로 본다.
const TEMP_PREFIX: &str = "vapid.tmp-";

/// 프로세스 안에서 임시 파일 이름이 겹치지 않게 하는 번호.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 예전에 쓰다 꺼져 남은 `vapid.tmp-*`를 지운다. 심볼릭 링크는 따라가지 않고 링크만 지운다.
/// 폴더는 건드리지 않는다. 지우지 못해도 켜기는 계속한다.
fn remove_leftover_temp_files(secret_dir: &Path) {
    let Ok(entries) = fs::read_dir(secret_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let is_temp = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(TEMP_PREFIX));
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        if is_temp && !is_dir {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// 같은 폴더의 새 임시 파일(`create_new`, Unix에서는 처음부터 0600)에 다 쓰고 맞춘 뒤 이름을 바꿔
/// `path`에 둔다. 쓰다 꺼져도 `path`에는 빈 파일이 남지 않는다. 실패하면 임시 파일을 지운다.
fn write_private_file_atomically(path: &Path, contents: &[u8]) -> Result<(), Error> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!("{TEMP_PREFIX}{}-{unique}", std::process::id()));
    let result = write_and_rename(&tmp, path, contents);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_and_rename(tmp: &Path, path: &Path, contents: &[u8]) -> Result<(), Error> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(tmp)
        .map_err(|e| Error::KeyFile(format!("임시 키 파일을 만들지 못했다: {}", e.kind())))?;
    restrict_permissions(tmp)?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|e| Error::KeyFile(format!("키 파일을 쓰지 못했다: {}", e.kind())))?;
    drop(file);
    fs::rename(tmp, path)
        .map_err(|e| Error::KeyFile(format!("키 파일을 두지 못했다: {}", e.kind())))?;
    if let Some(dir) = path.parent()
        && let Ok(handle) = fs::File::open(dir)
    {
        let _ = handle.sync_all();
    }
    restrict_permissions(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 저장한_글로_같은_키를_다시_만든다() {
        let key = VapidKey::generate();
        let text = URL_SAFE_NO_PAD.encode(key.secret.to_bytes());
        let again = VapidKey::from_text(&format!("{text}\n"));
        assert!(again.is_ok_and(|k| k.public_key() == key.public_key()));
    }

    #[test]
    fn 짧은_글은_키가_아니다() {
        assert!(matches!(
            VapidKey::from_text("AAAA"),
            Err(Error::InvalidKey(_))
        ));
    }
}
