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
}

impl<W: io::Write> io::Write for RedactedLine<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = (&self.secrets, &mut self.inner, buf);
        todo!()
    }

    fn flush(&mut self) -> io::Result<()> {
        todo!()
    }
}

impl<'a, W> MakeWriter<'a> for RedactingWriter<W>
where
    W: MakeWriter<'a>,
{
    type Writer = RedactedLine<W::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        let _ = (&self.secrets, &self.inner);
        todo!()
    }
}
