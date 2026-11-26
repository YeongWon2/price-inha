//! (65차, 9차 보강 3 · 4 "파일 돌려 쓰기(크기 상한)") `run --log-file`의 기록 파일을 크기 상한으로 돌려 쓴다.
//!
//! 쓰기 한 번(기록 한 줄)이 지금 파일을 상한 너머로 밀면, 먼저 `<이름>.1`로 옮기고(앞의 것은 `.2` …, 남기는 수를
//! 넘는 것은 지운다) 새 파일에 쓴다. 줄을 가운데에서 자르지 않는다. 줄 하나가 상한보다 길면 빈 새 파일에 그대로 쓴다.
//! 유닉스에서는 새 파일도 0600이다(기록은 가린 뒤 글이지만 본인만 읽는다, N8). 원본(Hermes · OpenClaw)에는 이
//! 동작이 없다 — 상한 10 MiB · 옛 파일 3개는 조정자 값이다.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// 기록 파일 하나의 기본 상한(10 MiB).
pub const LOG_FILE_MAX_BYTES: u64 = 10 * 1024 * 1024;
/// 남기는 옛 기록 파일 수(`<이름>.1` ~ `<이름>.3`).
pub const LOG_FILE_KEEP: usize = 3;

/// 크기 상한을 넘으면 옛 파일로 돌리는 기록 파일.
pub struct RotatingLogFile {
    path: PathBuf,
    file: File,
    size: u64,
    max_bytes: u64,
    keep: usize,
}

impl RotatingLogFile {
    /// `path`를 덧붙여 쓰게 연다. 부모 폴더가 없으면 만든다. 한 파일 상한 `max_bytes`, 옛 파일 `keep`개를 남긴다
    /// (`keep`이 0이면 넘칠 때 지금 파일을 비운다).
    pub fn open(path: &Path, max_bytes: u64, keep: usize) -> std::io::Result<RotatingLogFile> {
        if let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let file = open_append(path)?;
        let size = file.metadata()?.len();
        Ok(RotatingLogFile {
            path: path.to_path_buf(),
            file,
            size,
            max_bytes: max_bytes.max(1),
            keep,
        })
    }

    /// `<이름>.n`.
    fn numbered(&self, n: usize) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(format!(".{n}"));
        PathBuf::from(name)
    }

    /// 지금 파일을 `.1`로 옮기고(앞의 것은 한 칸씩 뒤로) 새 파일을 연다. 윈도는 열린 파일의 이름을 바꾸지 못하므로
    /// 옮기기 전에 지금 파일을 닫는다(그 자리에는 잠깐 버림 파일을 둔다).
    fn rotate(&mut self) -> std::io::Result<()> {
        self.file.flush()?;
        drop(std::mem::replace(&mut self.file, placeholder()?));
        let shifted = self.shift();
        // 옮기지 못했어도 지금 파일에 이어 쓴다(기록을 잃지 않는다).
        self.file = open_append(&self.path)?;
        self.size = self.file.metadata().map_or(0, |meta| meta.len());
        shifted
    }

    /// 옛 파일을 한 칸씩 뒤로 옮기고 지금 파일을 `.1`로(`keep`이 0이면 지운다).
    fn shift(&self) -> std::io::Result<()> {
        if self.keep == 0 {
            return std::fs::remove_file(&self.path);
        }
        let _ = std::fs::remove_file(self.numbered(self.keep));
        for n in (1..self.keep).rev() {
            let from = self.numbered(n);
            if from.exists() {
                std::fs::rename(&from, self.numbered(n + 1))?;
            }
        }
        std::fs::rename(&self.path, self.numbered(1))
    }
}

/// 옮기는 동안 잠깐 쥐는 버림 파일(유닉스 `/dev/null` · 윈도 `NUL`). 아무 데도 쓰지 않는다.
fn placeholder() -> std::io::Result<File> {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    OpenOptions::new().write(true).open(null)
}

/// 덧붙여 쓰게 연다. 유닉스에서는 0600으로 만들고, 이미 있던 파일이 넓게 열려 있으면 좁힌다.
fn open_append(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

impl Write for RotatingLogFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
        if self.size > 0 && self.size.saturating_add(len) > self.max_bytes {
            // 돌리지 못했으면(다른 프로그램이 옛 파일을 쥐고 있는 등) 지금 파일에 그대로 이어 쓴다.
            let _ = self.rotate();
        }
        // 한 번에 다 써서 줄이 두 파일로 갈리지 않게 한다.
        self.file.write_all(buf)?;
        self.size = self.size.saturating_add(len);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
