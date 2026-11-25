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
//!   200 `{"proof": "<요약값>"}`([`WHOAMI_PROOF_FIELD`]). 요약값은 [`run_proof`]`(증표, nonce, 포트)`다.
//!   포트는 (38차 보강) 서버의 관문이 **실제로 묶인 주소의 포트**다. 요청에서 받은 값(Host 머리글 등)으로 정하지
//!   않는다 — 사칭이 고쳐 보낼 수 있다.
//! - nonce는 [`WHOAMI_NONCE_MIN_LEN`] ~ [`WHOAMI_NONCE_MAX_LEN`]글자의 ASCII 16진수(`0-9a-fA-F`)다. 칸이 없거나,
//!   짧거나 길거나, 16진수가 아니거나, 본문이 JSON이 아니면 400이고 요약값을 싣지 않는다.
//! - 세션이 있고 **그 컴퓨터(루프백)에서 온** 요청에만 답한다(`pi_gate::local_only`와 같은 관문 — 원격 세션 ·
//!   전달 헤더가 있는 요청에는 요약값을 주지 않는다).
//! - 서버가 대조할 일은 없지만, 비교가 필요한 쪽(CLI)은 상수 시간으로 비교한다.
//!
//! ## 요약값 ([`run_proof`])
//!
//! `run_proof(token, nonce, port)` = 소문자 16진수(SHA-256( [`RUN_PROOF_DOMAIN`] ‖ `"\n"` ‖ token ‖ `"\n"` ‖ nonce
//! ‖ `"\n"` ‖ 십진수 포트 )).
//! `token`은 증표 파일의 글자(64글자 16진수) 그대로, `nonce`는 보낸 글자 그대로(대소문자를 바꾸지 않는다)의
//! UTF-8 바이트, 포트는 앞 0 없는 십진수 글자(`u16`의 `to_string`)다. 결과는 64글자. 증표를 모르면 만들 수 없고,
//! 요약값에서 증표를 되찾을 수 없다.
//!
//! (38차 보강 — 중계 사칭) 요약값을 포트와 묶는다: 서버는 자기가 실제로 연 포트로, CLI는 자기가 접속한 포트로
//! 계산한다. 진짜 서버가 다른 포트에서 돌 때 설정 포트를 차지한 사칭이 nonce를 진짜 서버의 `whoami`로 중계해도
//! 포트가 달라 맞지 않는다. 도메인 글자의 판 번호는 v2다(옛 식과 섞이지 않게).
//!
//! ## CLI의 확인 (`setup` · `access-key` · `open`)
//!
//! 도는 서버를 찾아 세션을 얻은 **뒤, 그 서버에 다른 요청(특히 비밀값)을 보내기 전에**:
//!
//! 1. 보관 폴더의 [`RUN_PROOF_FILE`]을 읽는다. 없거나, 유닉스에서 주인이 지금 사용자가 아니거나, 권한에
//!    0600 밖의 비트(그룹 · 남 · 실행)가 있으면 실패.
//! 2. 운영체제 난수로 nonce(16진수 [`WHOAMI_NONCE_MIN_LEN`]글자 이상)를 만들어 [`WHOAMI_PATH`]에 보낸다.
//! 3. 답의 `proof`가 [`run_proof`]`(파일의 증표, nonce, 접속한 포트)`와 같아야 한다(상수 시간 비교). 아니면 실패.
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
pub const RUN_PROOF_DOMAIN: &str = "price-inha run proof v2";

/// CLI가 확인에 실패해 멈출 때 출력에 드는 까닭 글.
pub const NOT_THIS_HOME_SERVER: &str = "이 포트에서 답하는 것은 이 보관 폴더의 서버가 아닙니다";

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use rand::TryRngCore;
use rand::rngs::OsRng;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::AppError;

/// 증표 · nonce · 포트로 요약값을 만든다(모듈 문서의 식). 소문자 16진수 64글자.
pub fn run_proof(token: &str, nonce: &str, port: u16) -> String {
    let mut hasher = Sha256::new();
    hasher.update(RUN_PROOF_DOMAIN.as_bytes());
    hasher.update(b"\n");
    hasher.update(token.as_bytes());
    hasher.update(b"\n");
    hasher.update(nonce.as_bytes());
    hasher.update(b"\n");
    hasher.update(port.to_string().as_bytes());
    lower_hex(&hasher.finalize())
}

/// 바이트 → 소문자 16진수.
pub(crate) fn lower_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

/// 운영체제 난수 `N`바이트의 소문자 16진수(사본은 지울 때 0으로). 난수를 얻지 못하면 `None`.
pub(crate) fn random_hex<const N: usize>() -> Option<Zeroizing<String>> {
    let mut bytes = Zeroizing::new([0u8; N]);
    OsRng.try_fill_bytes(bytes.as_mut()).ok()?;
    Some(Zeroizing::new(lower_hex(bytes.as_ref())))
}

/// 두 글자를 상수 시간으로 비교한다(길이가 다르면 바로 거짓 — 길이는 계약으로 정해져 있다).
pub(crate) fn same_text(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 서버가 쥔 이번 실행의 증표. 메모리의 사본은 놓을 때 0으로 지운다. `Debug`는 글자를 찍지 않는다.
#[derive(Clone)]
pub(crate) struct Token(Arc<Zeroizing<String>>);

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(..)")
    }
}

impl Token {
    /// 운영체제 난수 32바이트로 새 증표를 만든다.
    pub(crate) fn fresh() -> Result<Token, AppError> {
        random_hex::<32>()
            .map(|text| Token(Arc::new(text)))
            .ok_or(AppError::RunProof {
                kind: std::io::ErrorKind::Other,
            })
    }

    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// 보관 폴더에 쓴 증표 파일. 놓으면(서버를 끄면) 파일이 아직 이 증표일 때만 지운다
/// (같은 보관 폴더로 뒤에 뜬 다른 서버의 파일은 건드리지 않는다).
#[derive(Debug)]
pub(crate) struct RunProofFile {
    path: PathBuf,
    token: Token,
}

impl RunProofFile {
    /// `home`의 [`RUN_PROOF_FILE`]에 증표를 원자적으로(같은 폴더의 임시 파일 → 이름 바꾸기) 0600으로 쓴다.
    /// 앞서 남은 파일은 바꿔 쓴다.
    pub(crate) fn write(home: &Path, token: Token) -> Result<RunProofFile, AppError> {
        let path = home.join(RUN_PROOF_FILE);
        let temp = home.join(format!(".{RUN_PROOF_FILE}.{}.tmp", std::process::id()));
        let written = write_private(&temp, token.as_str().as_bytes())
            .and_then(|()| std::fs::rename(&temp, &path));
        if let Err(error) = written {
            let _ = std::fs::remove_file(&temp);
            return Err(AppError::RunProof { kind: error.kind() });
        }
        Ok(RunProofFile { path, token })
    }
}

impl Drop for RunProofFile {
    fn drop(&mut self) {
        let ours = std::fs::read_to_string(&self.path)
            .map(Zeroizing::new)
            .is_ok_and(|text| same_text(&text, self.token.as_str()));
        if ours {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// 새 파일을 주인만 읽고 쓰게(0600) 만들어 `bytes`를 쓰고 디스크에 내린다. 같은 이름이 있으면 지우고 새로 만든다.
fn write_private(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let _ = std::fs::remove_file(file);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut opened = options.open(file)?;
    opened.write_all(bytes)?;
    opened.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        opened.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// 관문이 실제로 묶인 포트를 경로보다 늦게 한 번 채우는 자리. 채우기 전의 확인 요청에는 요약값을 주지 않는다.
pub(crate) type BoundPort = Arc<OnceLock<u16>>;

/// 그 컴퓨터에서만 열리는 확인 경로([`WHOAMI_PATH`]). `GateBuilder::local_routes`에 끼운다.
/// `port`는 관문을 연 뒤 실제로 묶인 포트로 채운다(요청의 Host 머리글 등은 쓰지 않는다).
pub(crate) fn routes(token: Token, port: BoundPort) -> Router {
    Router::new()
        .route(WHOAMI_PATH, post(whoami))
        .with_state((token, port))
}

/// 본문에서 계약의 모양인 nonce를 읽는다.
fn nonce_of(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let nonce = value.get(WHOAMI_NONCE_FIELD)?.as_str()?;
    let fits = (WHOAMI_NONCE_MIN_LEN..=WHOAMI_NONCE_MAX_LEN).contains(&nonce.len())
        && nonce.bytes().all(|byte| byte.is_ascii_hexdigit());
    fits.then(|| nonce.to_string())
}

async fn whoami(State((token, port)): State<(Token, BoundPort)>, body: Bytes) -> Response {
    let (status, body) = match (nonce_of(&body), port.get()) {
        (None, _) => (StatusCode::BAD_REQUEST, json!({ "error": "invalid_input" })),
        // 포트를 아직 모른다(관문이 막 열리는 중): 요약값을 주지 않는다.
        (Some(_), None) => (
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error": "not_ready" }),
        ),
        (Some(nonce), Some(port)) => (
            StatusCode::OK,
            json!({ WHOAMI_PROOF_FIELD: run_proof(token.as_str(), &nonce, *port) }),
        ),
    };
    (status, [(CACHE_CONTROL, "no-store")], axum::Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 상수_시간_비교() {
        assert!(same_text("abc", "abc"));
        assert!(!same_text("abc", "abd"));
        assert!(!same_text("abc", "ab"));
    }

    #[test]
    fn 난수_글자는_소문자_16진수() {
        let made = random_hex::<32>().unwrap_or_default();
        assert_eq!(made.len(), 64);
        assert!(
            made.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }

    #[test]
    fn nonce_모양() {
        assert!(nonce_of(br#"{"nonce":"0123456789abcdefABCDEF0123456789"}"#).is_some());
        assert!(nonce_of(br#"{"nonce":"0123"}"#).is_none());
        assert!(nonce_of(b"nonce=1").is_none());
    }
}
