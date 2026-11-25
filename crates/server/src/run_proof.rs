//! (38차) 실행 증표: CLI(`setup` · `access-key` · `open`)가 설정의 포트에서 답하는 것이 **이 보관 폴더의
//! 서버**인지 비밀값을 보내기 전에 확인한다. 같은 컴퓨터의 다른 프로세스가 그 포트를 먼저 차지하고 세션 모양
//! (`POST /api/session`에 204 + 쿠키)만 흉내 내도 키를 넘기지 않게 하는 우리 안전장치다(원본에 없음 —
//! 37차 보안 검토, `docs/research/remote-access-security.md`. 원본 OpenClaw의 게이트웨이 토큰 파일과 같은 생각).
//!
//! # 계약
//!
//! ## 증표 파일 ([`RUN_PROOF_FILE`])
//!
//! - 서버([`crate::App`])는 **켤 때마다** 운영체제 난수 32바이트로 새 증표를 만들어 보관 폴더
//!   (`settings.home`)의 [`RUN_PROOF_FILE`]에 쓴다. 내용은 그 32바이트의 **소문자 16진수 64글자**뿐이다(줄바꿈 없음).
//!   쓰기는 원자적이다(같은 폴더의 임시 파일에 쓰고 이름을 바꾼다). 권한은 유닉스에서 0600. 앞서 남은 파일이
//!   있으면(끝내 지우지 못하고 죽은 서버) 새 증표 · 0600으로 바꿔 쓴다. [`crate::App::start`] ·
//!   [`crate::App::launch`]가 돌려주기 전에 쓴다(실행 파일의 `run`도 그 길이다).
//! - 서버를 끄면([`crate::App::shutdown`], 실행 파일은 끄기 신호) 그 파일을 지운다.
//! - 증표 글자는 기록 · 응답 · 이벤트 · 다른 파일 어디에도 그대로 나오지 않는다.
//!
//! ## 확인 경로 ([`WHOAMI_PATH`])
//!
//! - `POST` [`WHOAMI_PATH`] 본문 `{"nonce": "<CLI가 만든 난수>"}`([`WHOAMI_NONCE_FIELD`]) →
//!   200 `{"proof": "<요약값>"}`([`WHOAMI_PROOF_FIELD`]). 요약값은 [`run_proof`]`(증표, nonce)`다.
//! - nonce는 [`WHOAMI_NONCE_MIN_LEN`] ~ [`WHOAMI_NONCE_MAX_LEN`]글자의 ASCII 16진수(`0-9a-fA-F`)다. 칸이 없거나,
//!   짧거나 길거나, 16진수가 아니거나, 본문이 JSON이 아니면 400이고 요약값을 싣지 않는다.
//! - 세션이 있고 **그 컴퓨터(루프백)에서 온** 요청에만 답한다(`pi_gate::local_only`와 같은 관문 — 원격 세션 ·
//!   전달 헤더가 있는 요청에는 요약값을 주지 않는다).
//! - 서버가 대조할 일은 없지만, 비교가 필요한 쪽(CLI)은 상수 시간으로 비교한다.
//!
//! ## 요약값 ([`run_proof`])
//!
//! `run_proof(token, nonce)` = 소문자 16진수(SHA-256( [`RUN_PROOF_DOMAIN`] ‖ `"\n"` ‖ token ‖ `"\n"` ‖ nonce )).
//! `token`은 증표 파일의 글자(64글자 16진수) 그대로, `nonce`는 보낸 글자 그대로(대소문자를 바꾸지 않는다)의
//! UTF-8 바이트다. 결과는 64글자. 증표를 모르면 만들 수 없고, 요약값에서 증표를 되찾을 수 없다.
//!
//! ## CLI의 확인 (`setup` · `access-key` · `open`)
//!
//! 도는 서버를 찾아 세션을 얻은 **뒤, 그 서버에 다른 요청(특히 비밀값)을 보내기 전에**:
//!
//! 1. 보관 폴더의 [`RUN_PROOF_FILE`]을 읽는다. 없거나, 유닉스에서 주인이 지금 사용자가 아니거나, 권한에
//!    0600 밖의 비트(그룹 · 남 · 실행)가 있으면 실패.
//! 2. 운영체제 난수로 nonce(16진수 [`WHOAMI_NONCE_MIN_LEN`]글자 이상)를 만들어 [`WHOAMI_PATH`]에 보낸다.
//! 3. 답의 `proof`가 [`run_proof`]`(파일의 증표, nonce)`와 같아야 한다(상수 시간 비교). 아니면 실패.
//!
//! 실패하면 아무것도 보내지 않고 멈춘다. 출력에 [`NOT_THIS_HOME_SERVER`] 글이 든다. 종료 코드는 지금의 "그 포트에
//! 다른 프로그램"과 같다: `setup`은 [`crate::SETUP_FAILED_EXIT`], `access-key`는 1(`CliError::NotOurServer`와
//! 같은 길), `open`은 [`crate::OPEN_NO_SERVER_EXIT`]이고 화면 주소(`http://…`)를 내지 않는다.
//! 마법사가 띄운 서버도 같다(자식 프로세스라는 것만으로 믿지 않고, 증표 파일이 생길 때까지 기다렸다가 확인한다).
//! 브라우저 화면의 흐름은 그대로다(이 차수의 범위 밖).

/// 보관 폴더 안의 실행 증표 파일 이름. 서버가 켤 때 쓰고(0600) 끌 때 지운다.
pub const RUN_PROOF_FILE: &str = "run.proof";

/// `POST`: 실행 증표 확인(그 컴퓨터에서만). 본문 `{"nonce"}` → `{"proof"}`.
pub const WHOAMI_PATH: &str = "/api/whoami";

/// 확인 요청 본문의 nonce 칸 이름.
pub const WHOAMI_NONCE_FIELD: &str = "nonce";

/// 확인 답의 요약값 칸 이름.
pub const WHOAMI_PROOF_FIELD: &str = "proof";

/// nonce의 가장 짧은 길이(16진수 글자 수 — 16바이트).
pub const WHOAMI_NONCE_MIN_LEN: usize = 32;

/// nonce의 가장 긴 길이(16진수 글자 수).
pub const WHOAMI_NONCE_MAX_LEN: usize = 128;

/// 요약값 계산의 앞머리(다른 쓰임의 SHA-256과 섞이지 않게).
pub const RUN_PROOF_DOMAIN: &str = "price-inha run proof v1";

/// CLI가 확인에 실패해 멈출 때 출력에 드는 까닭 글.
pub const NOT_THIS_HOME_SERVER: &str = "이 포트에서 답하는 것은 이 보관 폴더의 서버가 아닙니다";

/// 증표와 nonce로 요약값을 만든다(모듈 문서의 식). 소문자 16진수 64글자.
pub fn run_proof(token: &str, nonce: &str) -> String {
    let _ = (token, nonce);
    String::new()
}
