//! 오류.

use pi_store::StoreError;
use thiserror::Error;

/// 이 모듈의 오류.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// 저장소 오류(이벤트 쌓기, 구독 표).
    #[error("저장소 오류: {0}")]
    Store(#[from] StoreError),
    /// 키 모양이 맞지 않다(구독의 공개키 · 인증 값, VAPID 키). 키 글자는 담지 않는다.
    #[error("키 모양이 맞지 않다: {0}")]
    InvalidKey(String),
    /// 조용한 시간 값이 맞지 않다(하루 분 수를 넘거나, 시작과 끝이 같다).
    #[error("조용한 시간 값이 맞지 않다")]
    InvalidQuietHours,
    /// 암호화 전 본문이 한 레코드(RFC 8291)에 들어가지 않는다.
    #[error("푸시 본문이 너무 길다: {len}바이트 (최대 {max})")]
    PayloadTooLarge {
        /// 받은 길이.
        len: usize,
        /// 넣을 수 있는 가장 큰 길이([`crate::MAX_PLAINTEXT`]).
        max: usize,
    },
    /// VAPID 키 파일을 읽거나 쓰지 못했다.
    #[error("키 파일 오류: {0}")]
    KeyFile(String),
    /// 푸시 서비스로 보내는 길의 오류(연결 · 읽기). 응답 상태 코드는 오류가 아니다.
    #[error("보내기 오류: {0}")]
    Transport(String),
}
