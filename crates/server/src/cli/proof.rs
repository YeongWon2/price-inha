//! (38차) CLI가 "도는 서버"의 신원을 실행 증표로 확인한다(계약은 [`crate::run_proof`]의 모듈 문서).
//!
//! 세션을 얻은 뒤, 그 서버에 다른 요청을 보내기 전에: 보관 폴더의 증표 파일을 읽고(없음 · 링크 · 주인이 내가
//! 아님 · 0600 밖의 비트 · 모양이 아님 → 실패), 운영체제 난수 nonce로 `whoami`를 불러 답의 요약값을 상수 시간으로
//! 대조한다. 증표 · 기대 요약값의 메모리 사본은 놓을 때 0으로 지운다.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::access::Running;
use crate::run_proof::{random_hex, same_text};
use crate::{RUN_PROOF_FILE, WHOAMI_NONCE_FIELD, WHOAMI_PATH, WHOAMI_PROOF_FIELD, run_proof};

/// 확인 요청 하나의 상한(루프백이라 짧다).
const WHOAMI_TIMEOUT: Duration = Duration::from_secs(10);
/// 증표 글자 수(32바이트의 16진수).
const TOKEN_LEN: usize = 64;

/// 그 서버가 이 보관 폴더(`home`)의 서버임을 증표로 확인했나.
pub(crate) async fn confirm(home: &Path, running: &Running) -> bool {
    // 파일부터 본다: 파일이 이상하면 확인 요청도 보내지 않는다.
    let Some(token) = read_token(home) else {
        return false;
    };
    let Some(nonce) = random_hex::<32>() else {
        return false;
    };
    let body = json!({ WHOAMI_NONCE_FIELD: nonce.as_str() })
        .to_string()
        .into_bytes();
    let Some((status, reply)) = running.send(WHOAMI_PATH, Some(body), WHOAMI_TIMEOUT).await else {
        return false;
    };
    let Some(proof) = reply.get(WHOAMI_PROOF_FIELD).and_then(Value::as_str) else {
        return false;
    };
    let wanted = Zeroizing::new(run_proof(&token, &nonce, running.port()));
    status == 200 && same_text(proof, &wanted)
}

/// 증표 파일의 글자(모양 · 주인 · 권한을 따지지 않는다 — 남은 파일을 알아보는 데만 쓴다).
pub(crate) fn raw_proof(home: &Path) -> Option<Zeroizing<String>> {
    std::fs::read_to_string(home.join(RUN_PROOF_FILE))
        .ok()
        .map(Zeroizing::new)
}

/// 증표 파일을 읽는다. 없거나, 보통 파일이 아니거나(링크 등), 주인이 지금 사용자가 아니거나, 권한에 0600 밖의
/// 비트가 있거나, 소문자 16진수 64글자가 아니면 `None`.
fn read_token(home: &Path) -> Option<Zeroizing<String>> {
    let path = home.join(RUN_PROOF_FILE);
    let before = std::fs::symlink_metadata(&path).ok()?;
    if !before.file_type().is_file() {
        return None;
    }
    let mut file = std::fs::File::open(&path).ok()?;
    let opened = file.metadata().ok()?;
    if !private_to_me(home, &before, &opened) {
        return None;
    }
    let mut text = Zeroizing::new(String::new());
    let limit = u64::try_from(TOKEN_LEN + 1).ok()?;
    (&mut file).take(limit).read_to_string(&mut text).ok()?;
    let fits = text.len() == TOKEN_LEN
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    fits.then_some(text)
}

/// 연 파일이 본 파일과 같고, 주인이 지금 사용자이며, 권한이 0600 안인가.
#[cfg(unix)]
fn private_to_me(home: &Path, before: &std::fs::Metadata, opened: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    let same = before.dev() == opened.dev() && before.ino() == opened.ino();
    let narrow = opened.mode() & 0o7777 & !0o600 == 0;
    same && narrow && current_uid(home).is_some_and(|uid| uid == opened.uid())
}

/// (58차) 윈도에서는 주인 · 권한 비트를 검사하지 않는다(그 비트가 없다 — 보관 폴더를 사용자 프로필 아래에 둔다).
#[cfg(not(unix))]
fn private_to_me(_home: &Path, _before: &std::fs::Metadata, _opened: &std::fs::Metadata) -> bool {
    true
}

/// 지금 사용자 번호: 보관 폴더에 빈 파일을 새로 만들어 그 주인을 본다(새 파일의 주인은 이 프로세스의
/// 사용자다). 바로 지운다. 만들 수 없으면 `None`(확인 실패로 본다).
#[cfg(unix)]
fn current_uid(home: &Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    let suffix = random_hex::<8>()?;
    let probe = home.join(format!(
        ".uid-probe.{}.{}",
        std::process::id(),
        suffix.as_str()
    ));
    let made = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe);
    let uid = made.and_then(|file| file.metadata()).map(|meta| meta.uid());
    let _ = std::fs::remove_file(&probe);
    uid.ok()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn home_with(text: &str, mode: u32) -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let file = dir.path().join(RUN_PROOF_FILE);
        let _ = std::fs::write(&file, text);
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode));
        dir
    }

    #[test]
    fn 증표_파일의_모양과_권한을_따진다() {
        let token = "0123456789abcdef".repeat(4);
        assert!(read_token(home_with(&token, 0o600).path()).is_some());
        assert!(read_token(home_with(&token, 0o400).path()).is_some());
        assert!(read_token(home_with(&token, 0o644).path()).is_none());
        assert!(read_token(home_with(&token, 0o700).path()).is_none());
        assert!(read_token(home_with(&format!("{token}\n"), 0o600).path()).is_none());
        assert!(read_token(home_with(&token.to_uppercase(), 0o600).path()).is_none());
        let empty = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        assert!(read_token(empty.path()).is_none());
    }
}
