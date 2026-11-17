//! GitHub 토큰 확인(E4). `GET {api_base}/user` 한 번. 토큰 · 주소는 오류에 담지 않는다(P22).

use std::time::Duration;

use pi_secrets::{SecretString, VerifyFailure};
use reqwest::{Client, redirect::Policy};

const USER_AGENT: &str = "price-inha-backup";
const ACCEPT: &str = "application/vnd.github+json";
const API_VERSION: &str = "2022-11-28";
const TIMEOUT: Duration = Duration::from_secs(10);

/// 토큰이 GitHub에서 받아들여지는지 한 번 확인한다.
///
/// 200이면 받아들임, 401 · 403이면 [`VerifyFailure::Rejected`], 그 밖(닿지 않음 · 시간 초과 · 5xx ·
/// 리다이렉트)은 [`VerifyFailure::Unreachable`]. 다시 시도하지 않고 리다이렉트를 따라가지 않는다.
pub async fn verify_token(api_base: &str, token: &SecretString) -> Result<(), VerifyFailure> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .redirect(Policy::none())
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| VerifyFailure::Unreachable)?;
    let url = format!("{}/user", api_base.trim_end_matches('/'));
    let response = client
        .get(url)
        .bearer_auth(token.expose())
        .header("accept", ACCEPT)
        .header("x-github-api-version", API_VERSION)
        .send()
        .await
        // 닿지 못한 오류에는 주소가 들어 있을 수 있어 담지 않는다.
        .map_err(|_| VerifyFailure::Unreachable)?;
    match response.status().as_u16() {
        200 => Ok(()),
        401 | 403 => Err(VerifyFailure::Rejected),
        _ => Err(VerifyFailure::Unreachable),
    }
}
