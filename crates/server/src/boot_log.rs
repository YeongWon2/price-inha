//! (68차, 백로그 U 5번) 서버가 켜지기 전 오류 파일을 작은 상한으로 줄인다.
//!
//! macOS 서비스 정의(launchd)는 표준 출력을 버리고(`/dev/null`) 서버가 `--log-file`로 기록을 직접 돌려 쓴다
//! ([`crate::RotatingLogFile`]). 표준 오류 자리(`<보관 폴더>/logs/server.error.log`)는 서버가 켜지기 전 실패(설정
//! 오류 · 바로 죽음)를 알 수 있게 남기는데, launchd는 그 파일을 끝없이 덧붙여 쓴다. 그래서 서버가 기록 파일로 뜰
//! 때마다 그 파일이 상한을 넘었으면 마지막 줄들만 남긴다(launchd는 덧붙이기로 열어 두므로 줄인 뒤에도 끝에 이어
//! 쓴다).

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// 켜지기 전 오류 파일의 상한(바이트). 돌려 쓰는 기록 파일([`crate::LOG_FILE_MAX_BYTES`])보다 훨씬 작다.
pub const BOOT_LOG_MAX_BYTES: u64 = 64 * 1024;

/// `path`가 `max`보다 크면 마지막 줄들(상한의 절반 안, 줄 가운데에서 자르지 않는다)만 남긴다. 줄였으면 참.
/// 없는 파일은 만들지 않고 거짓. 권한은 그대로다(같은 파일을 다시 쓴다).
pub fn cap_boot_log(path: &Path, max: u64) -> std::io::Result<bool> {
    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let len = file.metadata()?.len();
    if len <= max {
        return Ok(false);
    }
    let keep = max / 2;
    file.seek(SeekFrom::Start(len - keep))?;
    let mut tail = Vec::new();
    Read::by_ref(&mut file).take(keep).read_to_end(&mut tail)?;
    // 앞의 잘린 줄은 버린다(첫 줄바꿈 뒤부터).
    let start = tail
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(tail.len(), |at| at + 1);
    let tail = &tail[start..];
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(tail)?;
    file.flush()?;
    Ok(true)
}
