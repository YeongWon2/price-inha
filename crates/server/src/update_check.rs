//! (65차) 새 판 알리기 — 조정자 추천 "알리기만"(`docs/dev/decisions.md` "아직 정해지지 않은 것"의 추천안).
//!
//! 원본 동작(Hermes, `docs/research/install-and-cli.md` 2.5): 배경에서 새 판을 묻고 24시간 기억하며, **알림만 한다.
//! 자동 설치는 없다.** 우리도 같다:
//! - 확인 주소(릴리스의 `LATEST` 파일 — `scripts/release.sh`가 판 글자 하나를 쓴다)에서 판 글자 한 줄만 읽어
//!   이 실행 파일의 판([`VERSION`])과 견준다. 내려받거나 바꾸지 않는다.
//! - 답은 보관 폴더의 [`UPDATE_CHECK_FILE`]에 남는다(비밀이 아니다). `doctor`는 이 기록만 읽는다(바깥에 묻지 않는다).
//!   새 판이 있으면 서버 기록에 한 줄 남긴다.
//! - 확인 주소는 **조립 값**이다([`update_check_url`]): 빌드 때 `PRICE_INHA_UPDATE_URL`을 주면 그것(빈 글자면 끔),
//!   주지 않으면 릴리스 빌드만 [`UPDATE_CHECK_DEFAULT_URL`], 개발 · 시험 빌드는 없음. 시연 모드는 띄우지 않는다.
//!   실행 때 `PRICE_INHA_UPDATE_CHECK=0`이면 끈다(Hermes `updates.check false`).
//! - 주소는 `https`만(시험의 루프백 `http`는 받는다). 리다이렉트도 `https`로만 따라간다. 답은 작게만 읽는다.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

use crate::cli::VERSION;

/// 보관 폴더 안 확인 기록 파일 이름.
pub const UPDATE_CHECK_FILE: &str = "update-check.json";
/// 릴리스 빌드의 기본 확인 주소(설치 스크립트가 받는 곳과 같은 릴리스의 `LATEST`).
pub const UPDATE_CHECK_DEFAULT_URL: &str =
    "https://github.com/YeongWon2/price-inha/releases/latest/download/LATEST";
/// 실행 때 확인을 끄는 환경 변수(`0` · `off` · `false`).
pub const ENV_UPDATE_CHECK: &str = "PRICE_INHA_UPDATE_CHECK";

/// 다시 묻는 간격(원본과 같은 24시간).
const RECHECK: Duration = Duration::from_secs(24 * 3600);
/// 묻지 못했을 때 다시 묻기까지.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(6 * 3600);
/// 때를 보는 간격.
const TICK: Duration = Duration::from_secs(3600);
/// 요청 한 번의 시간 제한.
const TIMEOUT: Duration = Duration::from_secs(10);
/// 읽는 답의 상한(판 글자 한 줄이면 충분하다).
const MAX_BODY: usize = 256;

/// 확인 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateNotice {
    /// 이 실행 파일의 판.
    pub current: String,
    /// 확인 주소가 답한 판(받은 글자 그대로, 예: `v0.2.0`).
    pub latest: String,
    /// 더 새 판인가.
    pub newer: bool,
    /// 확인한 시각(유닉스 밀리초).
    pub checked_at: i64,
}

/// 확인하지 못한 까닭.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UpdateCheckError {
    /// https(또는 루프백 http)가 아니다. 요청을 만들지 않는다.
    #[error("확인 주소는 https만 받습니다")]
    InsecureUrl,
    /// 닿지 못했거나 성공 답이 아니다.
    #[error("확인 주소에 닿지 못했습니다")]
    Unreachable,
    /// 답이 판 글자가 아니다(기록하지 않는다).
    #[error("확인 주소의 답이 판 글자가 아닙니다")]
    NotAVersion,
    /// 기록을 남기지 못했다.
    #[error("확인 기록을 남기지 못했습니다")]
    Save,
}

/// 조립 값: 이 실행 파일이 새 판을 묻는 주소. 없으면 묻지 않는다.
pub fn update_check_url() -> Option<&'static str> {
    match option_env!("PRICE_INHA_UPDATE_URL") {
        Some("") => None,
        Some(url) => Some(url),
        None if cfg!(debug_assertions) => None,
        None => Some(UPDATE_CHECK_DEFAULT_URL),
    }
}

/// `v1.2.3` · `1.2.3` → (1, 2, 3). 그 밖은 없음.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim();
    let text = text.strip_prefix('v').unwrap_or(text);
    let mut parts = text.split('.');
    let mut next = || -> Option<u64> {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// `latest`가 `current`보다 새 판인가(숫자 차례로 견준다). 어느 쪽이든 판 글자가 아니면 거짓.
pub fn is_newer_version(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

/// 받는 주소인가: `https`, 또는 루프백 `http`(시험의 가짜 서버).
fn allowed(url: &reqwest::Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost")),
        _ => false,
    }
}

/// 확인 주소에 한 번 묻고, 판 글자면 기록을 남긴 뒤 돌려준다. 내려받거나 바꾸지 않는다.
pub async fn check_for_update(
    url: &str,
    home: &Path,
    now: i64,
) -> Result<UpdateNotice, UpdateCheckError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| UpdateCheckError::InsecureUrl)?;
    if !allowed(&parsed) {
        return Err(UpdateCheckError::InsecureUrl);
    }
    let latest = fetch_line(parsed).await?;
    if parse_version(&latest).is_none() {
        return Err(UpdateCheckError::NotAVersion);
    }
    let notice = UpdateNotice {
        current: VERSION.to_string(),
        newer: is_newer_version(&latest, VERSION),
        latest,
        checked_at: now,
    };
    save(home, &notice).map_err(|_| UpdateCheckError::Save)?;
    Ok(notice)
}

/// 답의 첫 줄(작게만 읽는다).
async fn fetch_line(url: reqwest::Url) -> Result<String, UpdateCheckError> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 5 || !allowed(attempt.url()) {
            attempt.stop()
        } else {
            attempt.follow()
        }
    });
    let client = reqwest::Client::builder()
        .redirect(policy)
        .no_proxy()
        .timeout(TIMEOUT)
        .user_agent(format!("price-inha/{VERSION}"))
        .build()
        .map_err(|_| UpdateCheckError::Unreachable)?;
    let mut reply = client
        .get(url)
        .send()
        .await
        .map_err(|_| UpdateCheckError::Unreachable)?;
    if !reply.status().is_success() {
        return Err(UpdateCheckError::Unreachable);
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = reply
        .chunk()
        .await
        .map_err(|_| UpdateCheckError::Unreachable)?
    {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_BODY {
            return Err(UpdateCheckError::NotAVersion);
        }
    }
    let text = String::from_utf8(body).map_err(|_| UpdateCheckError::NotAVersion)?;
    Ok(text.lines().next().unwrap_or_default().trim().to_string())
}

fn save(home: &Path, notice: &UpdateNotice) -> std::io::Result<()> {
    let body = json!({
        "current": notice.current,
        "latest": notice.latest,
        "newer": notice.newer,
        "checked_at": notice.checked_at,
    });
    let path = home.join(UPDATE_CHECK_FILE);
    let temp = home.join(format!("{UPDATE_CHECK_FILE}.tmp"));
    std::fs::write(&temp, body.to_string())?;
    std::fs::rename(&temp, &path)
}

/// 남은 확인 기록. 판은 이 실행 파일의 것으로 다시 견준다(그 사이 판을 바꿨을 수 있다). 없거나 틀리면 없음.
pub fn read_update_notice(home: &Path) -> Option<UpdateNotice> {
    let text = std::fs::read_to_string(home.join(UPDATE_CHECK_FILE)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let latest = value["latest"].as_str()?.to_string();
    parse_version(&latest)?;
    Some(UpdateNotice {
        current: VERSION.to_string(),
        newer: is_newer_version(&latest, VERSION),
        latest,
        checked_at: value["checked_at"].as_i64()?,
    })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn turned_off() -> bool {
    std::env::var(ENV_UPDATE_CHECK)
        .is_ok_and(|value| matches!(value.trim(), "0" | "off" | "false" | "no"))
}

/// 배경에서 하루 한 번 묻는다. 시연 모드이거나 주소가 없거나 [`ENV_UPDATE_CHECK`]로 껐으면 띄우지 않는다(없음).
/// 새 판이 있으면 기록에 한 줄 남긴다(알리기만).
pub fn spawn_update_check(
    home: PathBuf,
    url: Option<&'static str>,
    demo: bool,
) -> Option<tokio::task::JoinHandle<()>> {
    let url = url.filter(|_| !demo && !turned_off())?;
    Some(tokio::spawn(async move {
        let recheck = i64::try_from(RECHECK.as_millis()).unwrap_or(i64::MAX);
        let mut not_before = 0_i64;
        loop {
            let now = now_ms();
            let fresh = read_update_notice(&home)
                .is_some_and(|notice| now.saturating_sub(notice.checked_at) < recheck);
            if !fresh && now >= not_before {
                match check_for_update(url, &home, now).await {
                    Ok(notice) if notice.newer => tracing::info!(
                        latest = %notice.latest,
                        current = %notice.current,
                        "새 판이 있습니다. 바꾸려면 설치 한 줄(install.sh · install.ps1)을 다시 돌립니다 — 저절로 \
                    내려받거나 바꾸지 않습니다"
                    ),
                    Ok(_) => {}
                    Err(error) => {
                        tracing::debug!(%error, "새 판을 확인하지 못했습니다");
                        let wait =
                            i64::try_from(RETRY_AFTER_FAILURE.as_millis()).unwrap_or(i64::MAX);
                        not_before = now.saturating_add(wait);
                    }
                }
            }
            tokio::time::sleep(TICK).await;
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 판_글자만_읽는다() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version(" 0.1.0\n"), Some((0, 1, 0)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("v1.x.3"), None);
    }

    #[test]
    fn 받는_주소는_https와_루프백_http뿐() {
        let ok = |u: &str| allowed(&reqwest::Url::parse(u).unwrap());
        assert!(ok("https://github.com/a/LATEST"));
        assert!(ok("http://127.0.0.1:9/LATEST"));
        assert!(!ok("http://example.com/LATEST"));
        assert!(!ok("ftp://example.com/LATEST"));
    }
}
