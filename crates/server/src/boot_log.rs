//! (68차, 백로그 U 5번) 서버가 켜지기 전 오류 파일(macOS 서비스의 표준 오류 자리)을 작은 상한으로 줄인다.

use std::path::Path;

/// 켜지기 전 오류 파일의 상한(바이트).
pub const BOOT_LOG_MAX_BYTES: u64 = 64 * 1024;

/// `path`가 `max`보다 크면 마지막 줄들만 남긴다. 줄였으면 참. 없는 파일은 만들지 않는다.
pub fn cap_boot_log(path: &Path, max: u64) -> std::io::Result<bool> {
    let _ = (path, max);
    todo!()
}
