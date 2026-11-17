//! 실행 파일 띄우기를 잠깐 다시 해 보는 곳(ai.md 9차).
//!
//! Linux에서는 다른 스레드가 fork한 순간 실행 파일이 쓰기용으로 열려 있으면 띄우기가
//! `ETXTBSY`(`ExecutableFileBusy`)로 실패한다. 이때만 잠깐 쉬고 다시 띄운다.

use std::io;
use std::time::Duration;

/// 띄우기를 모두 몇 번까지 해 보는가.
pub(super) const SPAWN_ATTEMPTS: u32 = 5;

/// 다시 띄우기 전에 쉬는 시간의 단위. `n`번째 실패 뒤에는 이 값 × `n`만큼 쉰다.
pub(super) const SPAWN_BUSY_STEP: Duration = Duration::from_millis(20);

/// `spawn`을 부르고, 실행 파일이 바쁘다는 오류면 잠깐 쉬고 다시 부른다.
/// 다른 오류는 바로 돌려준다. [`SPAWN_ATTEMPTS`]번 모두 바쁘면 마지막 오류를 돌려준다.
pub(super) async fn spawn_retrying<T>(mut spawn: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut attempt = 1;
    loop {
        match spawn() {
            Err(e) if e.kind() == io::ErrorKind::ExecutableFileBusy && attempt < SPAWN_ATTEMPTS => {
                tokio::time::sleep(SPAWN_BUSY_STEP * attempt).await;
                attempt += 1;
            }
            done => return done,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn busy() -> io::Error {
        io::Error::from(io::ErrorKind::ExecutableFileBusy)
    }

    #[tokio::test]
    async fn 실행_파일이_바쁘면_다시_띄워_성공한다() {
        let calls = Cell::new(0u32);
        let got = spawn_retrying(|| {
            calls.set(calls.get() + 1);
            if calls.get() < 3 { Err(busy()) } else { Ok(7) }
        })
        .await;
        assert_eq!(got.unwrap(), 7);
        assert_eq!(calls.get(), 3);
    }

    #[tokio::test]
    async fn 끝까지_바쁘면_정한_횟수만_해_보고_그_오류를_돌려준다() {
        let calls = Cell::new(0u32);
        let got: io::Result<()> = spawn_retrying(|| {
            calls.set(calls.get() + 1);
            Err(busy())
        })
        .await;
        assert_eq!(got.unwrap_err().kind(), io::ErrorKind::ExecutableFileBusy);
        assert_eq!(calls.get(), SPAWN_ATTEMPTS);
    }

    #[tokio::test]
    async fn 다른_오류는_다시_띄우지_않는다() {
        let calls = Cell::new(0u32);
        let got: io::Result<()> = spawn_retrying(|| {
            calls.set(calls.get() + 1);
            Err(io::Error::from(io::ErrorKind::NotFound))
        })
        .await;
        assert_eq!(got.unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn 다시_띄우기_전에_시도_번호만큼_쉰다() {
        let calls = Cell::new(0u32);
        let started = tokio::time::Instant::now();
        let _: io::Result<()> = spawn_retrying(|| {
            calls.set(calls.get() + 1);
            Err(busy())
        })
        .await;
        // 1 + 2 + 3 + 4 단위(마지막 실패 뒤에는 쉬지 않는다).
        assert_eq!(started.elapsed(), SPAWN_BUSY_STEP * 10);
    }
}
