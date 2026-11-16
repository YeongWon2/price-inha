//! (W5 1차, T18) tracing 출력에 비밀키 가리기를 건다(E38 · N8).
//!
//! 실행 파일(`main.rs`)은 표준 오류로 쓰는 tracing 출력을 [`RedactingWriter`]로 감싼다. 한 줄씩 모아
//! `pi_secrets::SecretStore::redact`로 가린 뒤에만 안쪽 출력에 쓴다.

use std::io;
use std::sync::Arc;

use pi_secrets::SecretStore;
use tracing_subscriber::fmt::MakeWriter;

/// tracing 출력의 가리기 자리. `W`는 실제로 쓰는 곳(표준 오류 등)이다.
///
/// 비밀 보관소에 등록된 값(확인에 실패한 값 포함, N8)이 줄 어디에 있든 가려진 뒤에만 `W`에 닿는다.
/// 줄의 나머지 글자는 그대로다.
pub struct RedactingWriter<W> {
    secrets: Arc<SecretStore>,
    inner: W,
}

impl<W> RedactingWriter<W> {
    /// 비밀 보관소와 실제로 쓰는 곳으로 만든다.
    pub fn new(secrets: Arc<SecretStore>, inner: W) -> Self {
        Self { secrets, inner }
    }
}

/// [`RedactingWriter`]가 사건 하나마다 내는 쓰개. 받은 글자를 모았다가 가려서 안쪽에 쓴다
/// (늦어도 버려질 때 쓴다).
pub struct RedactedLine<W: io::Write> {
    secrets: Arc<SecretStore>,
    inner: W,
    /// 아직 줄바꿈을 받지 못한 글자. 줄이 끝나야 가린다(비밀 값이 두 번의 쓰기로 나뉘어 와도 가리게).
    pending: Vec<u8>,
}

impl<W: io::Write> RedactedLine<W> {
    /// 모은 글자 가운데 앞의 `upto`바이트를 가려서 안쪽에 쓴다.
    fn emit(&mut self, upto: usize) -> io::Result<()> {
        let line: Vec<u8> = self.pending.drain(..upto).collect();
        let clean = self.secrets.redact(&String::from_utf8_lossy(&line));
        self.inner.write_all(clean.as_bytes())
    }
}

impl<W: io::Write> io::Write for RedactedLine<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        if let Some(end) = self.pending.iter().rposition(|b| *b == b'\n') {
            self.emit(end + 1)?;
        }
        Ok(buf.len())
    }

    /// 끝나지 않은 줄은 쓰지 않는다(가리기 전에 비밀 값의 앞부분이 나가지 않게). 안쪽만 비운다.
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: io::Write> Drop for RedactedLine<W> {
    fn drop(&mut self) {
        if !self.pending.is_empty() {
            let _ = self.emit(self.pending.len());
        }
        let _ = self.inner.flush();
    }
}

impl<'a, W> MakeWriter<'a> for RedactingWriter<W>
where
    W: MakeWriter<'a>,
{
    type Writer = RedactedLine<W::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactedLine {
            secrets: Arc::clone(&self.secrets),
            inner: self.inner.make_writer(),
            pending: Vec::new(),
        }
    }
}
