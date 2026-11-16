//! 스냅샷이 온전한 저장소 파일인지 본다(원본 동작: OpenClaw verify의 SQLite 온전함 검사).
//!
//! 먼저 SQLite 파일 머리말(이름표 · 쪽 크기 · 파일 길이 · 첫 쪽 모양)을 보고, 통과하면 잠깐 만든 폴더에
//! 사본을 두고 [`pi_store::Store::open`]으로 열어 본다. 사본 폴더는 끝나면 지운다.

use std::path::Path;

use pi_store::Store;

const MAGIC: &[u8; 16] = b"SQLite format 3\0";
const HEADER_LEN: usize = 100;

/// 온전한 저장소 파일인가.
///
/// `temp_root`가 있으면 열어 보는 사본을 그 안(0700 폴더)에만 두고 끝나면 지운다. 없으면 시스템 임시 폴더.
pub(crate) fn is_sound(bytes: &[u8], temp_root: Option<&Path>) -> bool {
    header_ok(bytes) && opens(bytes, temp_root)
}

/// SQLite 파일 머리말과 길이가 맞는가.
fn header_ok(bytes: &[u8]) -> bool {
    if bytes.len() < HEADER_LEN || !bytes.starts_with(MAGIC) {
        return false;
    }
    let raw = u16::from_be_bytes([bytes[16], bytes[17]]);
    let page = if raw == 1 { 65_536 } else { usize::from(raw) };
    if !(512..=65_536).contains(&page) || !page.is_power_of_two() {
        return false;
    }
    if !bytes.len().is_multiple_of(page) {
        return false;
    }
    let change = &bytes[24..28];
    let valid_for = &bytes[92..96];
    let pages = u32::from_be_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]);
    // 머리말의 쪽 수는 "판 확인 번호"가 변경 번호와 같을 때만 믿을 수 있다.
    if change == valid_for && pages != 0 && (pages as usize).checked_mul(page) != Some(bytes.len())
    {
        return false;
    }
    // 첫 쪽(sqlite_schema)은 표 B-트리(잎 0x0D 또는 안쪽 0x05)다.
    matches!(bytes[HEADER_LEN], 0x0D | 0x05)
}

/// 잠깐 만든 폴더에 사본을 두고 저장소로 열린다.
fn opens(bytes: &[u8], temp_root: Option<&Path>) -> bool {
    let made = match temp_root {
        Some(root) => tempfile::Builder::new().prefix("verify-").tempdir_in(root),
        None => tempfile::tempdir(),
    };
    let Ok(dir) = made else {
        return false;
    };
    let path = dir.path().join(crate::SNAPSHOT_FILE);
    if std::fs::write(&path, bytes).is_err() {
        return false;
    }
    let opened = Store::open(&path).is_ok();
    drop(dir);
    opened
}

#[cfg(test)]
mod tests {
    use super::header_ok;

    #[test]
    fn 머리말이_없거나_짧으면_온전하지_않다() {
        assert!(!header_ok(b""));
        assert!(!header_ok(b"SQLite format 3\0"));
        let mut fake = vec![0u8; 4096];
        fake[..16].copy_from_slice(super::MAGIC);
        fake[16] = 0x10; // 4096
        fake[100] = 0x0D;
        assert!(header_ok(&fake));
        fake.push(0);
        assert!(!header_ok(&fake), "쪽 크기의 배수가 아니다");
    }
}
