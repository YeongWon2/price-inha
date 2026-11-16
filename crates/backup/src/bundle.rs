//! 묶음: manifest와 파일들, 그리고 age 암호화 · 풀기 · 검증 (D7, 원본 동작: OpenClaw manifest · verify).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use pi_secrets::SecretString;
use pi_store::UnixMillis;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{BackupError, BundleProblem};

/// manifest. 묶음 안에 함께 든다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// 판 번호([`crate::MANIFEST_VERSION`]).
    pub version: u32,
    /// 스냅샷을 뜬 시각.
    pub created_at: UnixMillis,
    /// 담긴 파일들.
    pub files: Vec<ManifestFile>,
}

/// manifest의 파일 한 줄.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestFile {
    /// 묶음 안 경로. 상대 경로 한 조각이어야 한다.
    pub path: String,
    /// 바이트 수.
    pub size: u64,
    /// SHA-256(소문자 16진수 64글자).
    pub sha256: String,
}

/// 풀어 놓은 묶음.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// manifest.
    pub manifest: Manifest,
    /// 파일들.
    pub files: Vec<BundleFile>,
}

/// 묶음 안의 파일 하나.
#[derive(Clone, PartialEq, Eq)]
pub struct BundleFile {
    /// 묶음 안 경로.
    pub path: String,
    /// 내용.
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for BundleFile {
    /// 내용은 찍지 않는다(경로와 길이만).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BundleFile")
            .field("path", &self.path)
            .field("len", &self.bytes.len())
            .finish()
    }
}

/// 바이트의 SHA-256을 소문자 16진수 64글자로.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// 묶음을 age 수신자 공개키(`age1…`)로 암호화한다. **검증하지 않고** 그대로 담는다.
///
/// 공개키가 잘못됐으면 [`BackupError::Encrypt`].
pub fn seal(bundle: &Bundle, recipient: &str) -> Result<Vec<u8>, BackupError> {
    let recipient =
        age::x25519::Recipient::from_str(recipient.trim()).map_err(|_| BackupError::Encrypt)?;
    let plain = encode_bundle(bundle)?;
    age::encrypt(&recipient, &plain).map_err(|_| BackupError::Encrypt)
}

/// 암호문을 비밀키 identity(`AGE-SECRET-KEY-1…`)로 풀어 묶음으로 읽는다. **검증하지 않는다.**
///
/// identity가 잘못됐거나 맞지 않으면 [`BackupError::Decrypt`], 묶음 모양을 읽을 수 없으면
/// [`BackupError::InvalidBundle`]([`crate::BundleProblem::Malformed`]). identity는 어디에도 남기지 않는다.
pub fn open(encrypted: &[u8], identity: &SecretString) -> Result<Bundle, BackupError> {
    let identity = age::x25519::Identity::from_str(identity.expose().trim())
        .map_err(|_| BackupError::Decrypt)?;
    let plain = age::decrypt(&identity, encrypted).map_err(|_| BackupError::Decrypt)?;
    decode_bundle(&plain)
}

/// 묶음을 검증한다. 하나라도 걸리면 [`BackupError::InvalidBundle`].
///
/// 판 번호 → 경로(상대 경로 한 조각만) → manifest가 말한 파일이 다 있는가 → manifest에 없는 파일이
/// 없는가 → 크기 · SHA-256 → [`crate::SNAPSHOT_FILE`]이 있는가 → 그 스냅샷이 온전한 SQLite 저장소인가.
pub fn verify(bundle: &Bundle) -> Result<(), BackupError> {
    check(bundle).map_err(BackupError::InvalidBundle)
}

fn check(bundle: &Bundle) -> Result<(), BundleProblem> {
    let manifest = &bundle.manifest;
    if manifest.version != crate::MANIFEST_VERSION {
        return Err(BundleProblem::UnsupportedVersion(manifest.version));
    }
    let listed = manifest.files.iter().map(|f| f.path.as_str());
    let held = bundle.files.iter().map(|f| f.path.as_str());
    if !listed.chain(held).all(is_safe_path) {
        return Err(BundleProblem::UnsafePath);
    }
    let mut seen = HashSet::new();
    if !manifest.files.iter().all(|f| seen.insert(f.path.as_str())) {
        return Err(BundleProblem::Malformed);
    }
    let mut held: HashMap<&str, &BundleFile> = HashMap::new();
    for file in &bundle.files {
        if held.insert(file.path.as_str(), file).is_some() {
            return Err(BundleProblem::Malformed);
        }
    }
    if manifest
        .files
        .iter()
        .any(|f| !held.contains_key(f.path.as_str()))
    {
        return Err(BundleProblem::MissingFile);
    }
    if bundle.files.iter().any(|f| !seen.contains(f.path.as_str())) {
        return Err(BundleProblem::UnlistedFile);
    }
    for listed in &manifest.files {
        let Some(file) = held.get(listed.path.as_str()) else {
            return Err(BundleProblem::MissingFile);
        };
        if file.bytes.len() as u64 != listed.size || sha256_hex(&file.bytes) != listed.sha256 {
            return Err(BundleProblem::HashMismatch);
        }
    }
    let Some(snapshot) = held.get(crate::SNAPSHOT_FILE) else {
        return Err(BundleProblem::MissingSnapshot);
    };
    if !crate::sqlite::is_sound(&snapshot.bytes) {
        return Err(BundleProblem::CorruptDatabase);
    }
    Ok(())
}

/// 상대 경로 한 조각인가: 비지 않고, `.` · `..`가 아니고, 폴더 나눔 · 드라이브 · NUL이 없다.
fn is_safe_path(path: &str) -> bool {
    !path.is_empty()
        && path != "."
        && path != ".."
        && !path.contains(['/', '\\', ':', '\0'])
        && Path::new(path).components().count() == 1
}

/// 암호화 전 묶음의 모양(JSON). 파일 내용은 Base64다.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireBundle {
    manifest: WireManifest,
    files: Vec<WireFile>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireManifest {
    version: u32,
    created_at: i64,
    files: Vec<WireListed>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireListed {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireFile {
    path: String,
    content: String,
}

/// 묶음을 평문 바이트로 만든다.
pub(crate) fn encode_bundle(bundle: &Bundle) -> Result<Vec<u8>, BackupError> {
    let wire = WireBundle {
        manifest: WireManifest {
            version: bundle.manifest.version,
            created_at: bundle.manifest.created_at.0,
            files: bundle
                .manifest
                .files
                .iter()
                .map(|f| WireListed {
                    path: f.path.clone(),
                    size: f.size,
                    sha256: f.sha256.clone(),
                })
                .collect(),
        },
        files: bundle
            .files
            .iter()
            .map(|f| WireFile {
                path: f.path.clone(),
                content: STANDARD.encode(&f.bytes),
            })
            .collect(),
    };
    serde_json::to_vec(&wire).map_err(|_| BackupError::Encrypt)
}

fn decode_bundle(plain: &[u8]) -> Result<Bundle, BackupError> {
    let malformed = || BackupError::InvalidBundle(BundleProblem::Malformed);
    let wire: WireBundle = serde_json::from_slice(plain).map_err(|_| malformed())?;
    let mut files = Vec::with_capacity(wire.files.len());
    for file in wire.files {
        let bytes = STANDARD.decode(file.content).map_err(|_| malformed())?;
        files.push(BundleFile {
            path: file.path,
            bytes,
        });
    }
    let manifest = Manifest {
        version: wire.manifest.version,
        created_at: UnixMillis(wire.manifest.created_at),
        files: wire
            .manifest
            .files
            .into_iter()
            .map(|f| ManifestFile {
                path: f.path,
                size: f.size,
                sha256: f.sha256,
            })
            .collect(),
    };
    Ok(Bundle { manifest, files })
}

#[cfg(test)]
mod tests {
    use super::is_safe_path;

    #[test]
    fn 한_조각_이름만_안전하다() {
        assert!(is_safe_path("price-inha.sqlite"));
        for bad in ["", ".", "..", "a/b", "a\\b", "/abs", "c:x", "a\0b"] {
            assert!(!is_safe_path(bad), "{bad:?}");
        }
    }
}
