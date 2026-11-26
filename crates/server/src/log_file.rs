//! (65차, 9차 보강 3 · 4) `run --log-file`의 기록 파일을 크기 상한으로 돌려 쓴다 — red 뼈대.

use std::io::Write;
use std::path::Path;

/// 기록 파일 하나의 기본 상한(10 MiB).
pub const LOG_FILE_MAX_BYTES: u64 = 10 * 1024 * 1024;
/// 남기는 옛 기록 파일 수(`<이름>.1` ~ `<이름>.3`).
pub const LOG_FILE_KEEP: usize = 3;

/// 크기 상한을 넘으면 옛 파일로 돌리는 기록 파일.
pub struct RotatingLogFile {
    file: std::fs::File,
}

impl RotatingLogFile {
    /// 연다(red 뼈대).
    pub fn open(path: &Path, max_bytes: u64, keep: usize) -> std::io::Result<RotatingLogFile> {
        let _ = (max_bytes, keep);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(RotatingLogFile { file })
    }
}

impl Write for RotatingLogFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
