//! 이 모듈의 오류 타입. 어떤 변형도 세션 값이나 요청 본문을 담지 않는다.

/// 접속 관문의 오류.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GateError {
    /// 주소를 열지 못했다(이미 쓰는 포트 등). 종류만 싣는다.
    #[error("주소를 열지 못했습니다: {kind}")]
    Bind {
        /// 실패 종류.
        kind: std::io::ErrorKind,
    },
    /// 저장소 오류(이벤트 쌓기 · 읽기 · 접속 기록).
    #[error("저장소 오류: {0}")]
    Store(#[from] pi_store::StoreError),
    /// 관문이 이미 닫혔다. 이벤트를 저장하지 않았다.
    #[error("접속 관문이 닫혔습니다")]
    Closed,
    /// 내용이 크기 상한(`GateConfig::max_frame_bytes`)을 넘는다. 저장하지도 보내지도 않았다.
    /// 내용은 싣지 않는다.
    #[error("내용이 너무 큽니다 (상한 {limit}바이트)")]
    FrameTooLarge {
        /// 상한(바이트).
        limit: usize,
    },
    /// 설정 값을 받을 수 없다(5차: [`crate::GateConfig::keepalive_interval`] ·
    /// [`crate::GateConfig::pong_timeout`]이 0). 주소를 열지 않았다.
    #[error("설정 값을 받을 수 없습니다: {field}")]
    InvalidConfig {
        /// 받을 수 없는 설정 칸의 이름(`GateConfig`의 칸 이름 그대로).
        field: &'static str,
    },
    /// 안에서 돌던 작업이 끝나지 못했다.
    #[error("서버 작업이 비정상으로 끝났습니다")]
    Task,
}
